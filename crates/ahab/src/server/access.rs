//! Who may use the server: the guards every request passes after
//! the host check, and the client credentials the API asks for.
//!
//! In order: where the request comes from (forwarding headers are believed
//! only from `trusted_proxies`); the connections rule (plain HTTP to a
//! public address is refused); the Origin check (other websites get 403);
//! and the credential: every route under `/api/` needs a paired client's,
//! except the deliberately public ones in [`PUBLIC`], and the routes a
//! daemon's plugins call ([`MACHINE`]), which also take a connected
//! machine's token.
//!
//! A new route is therefore authenticated by default. Making one public
//! means adding it to [`PUBLIC`], which `tests/server/access.rs` checks
//! against the security docs.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::body::{Body, HttpBody};
use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use tokio::sync::watch;
use uuid::Uuid;

use crate::modules::clients::{Client, Seen};
use crate::server::error::ApiError;
use crate::server::state::AppState;
use crate::shared::net::{IpRange, Network};

/// The name of the cookie browsers send their credential in.
pub const COOKIE: &str = "ahab_device";

/// Who may reach the API without a client credential, by method and path
/// (`{…}` matches one segment). Everything else under `/api/` needs one.
pub const PUBLIC: &[(&str, &str, Public)] = &[
    ("GET", "/api/v1/health", Public::Health),
    // Pairing: a new client has no credential yet.
    ("POST", "/api/v1/pair", Public::Pairing),
    ("POST", "/api/v1/pairing-requests", Public::Pairing),
    ("GET", "/api/v1/pairing-requests/{id}", Public::Pairing),
    // Daemons prove themselves with their machine key.
    ("POST", "/api/v1/enrollments", Public::Daemon),
    ("GET", "/api/v1/enrollments/{id}", Public::Daemon),
    ("GET", "/api/v1/daemon/connect", Public::Daemon),
];

/// Why a route is public, which decides the guards it still passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Public {
    /// Version and API version only: no guard but the host check.
    Health,
    /// Hands out credentials: the connections rule and the Origin check.
    Pairing,
    /// A daemon's own proof: the Origin check only.
    Daemon,
}

/// The routes a daemon calls for its plugins (`commands.call`), with the
/// token the server gave its connection; paired clients may call them too.
/// A machine's token reaches only its own machine (`Caller::may_reach`):
/// the routes check where the project, terminal or command is.
pub const MACHINE: &[(&str, &str)] = &[
    ("POST", "/api/v1/projects/{id}/terminals"),
    ("GET", "/api/v1/terminals"),
    ("GET", "/api/v1/terminals/{id}"),
    ("POST", "/api/v1/terminals/{id}/kill"),
    ("POST", "/api/v1/plugins/{id}/commands/{name}"),
    ("POST", "/api/v1/plugins/{id}/notifications"),
];

/// The guards' settings.
#[derive(Debug, Clone)]
pub struct Access {
    pub trusted_proxies: Vec<IpRange>,
    /// Origins besides the server's own, as `scheme://host[:port]`.
    pub allowed_origins: Vec<String>,
    /// The address people use, without a trailing slash.
    pub public_url: Option<String>,
    pub allow_insecure_http: bool,
    /// Where pairing links point when neither `public_url` nor the request
    /// says better: the address the server listens on, reachable from
    /// other devices. `None` when it listens on loopback only.
    pub outward_url: Option<String>,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            trusted_proxies: crate::shared::net::LOOPBACK.to_vec(),
            allowed_origins: Vec::new(),
            public_url: None,
            allow_insecure_http: false,
            outward_url: None,
        }
    }
}

impl Access {
    /// The base of links for another client: `public_url`, else the
    /// address `requester` used unless that is this computer's own.
    pub fn link_base(&self, requester: &Requester) -> String {
        if let Some(url) = &self.public_url {
            return url.clone();
        }
        let own = requester.own_origin();
        let local = requester.host.as_deref().is_some_and(is_loopback_host);
        match (&self.outward_url, local) {
            (Some(outward), true) => outward.clone(),
            _ => own,
        }
    }

    fn origin_allowed(&self, origin: &str, requester: &Requester) -> bool {
        let origin = origin.trim_end_matches('/').to_ascii_lowercase();
        origin == requester.own_origin().to_ascii_lowercase()
            || self
                .public_url
                .as_deref()
                .is_some_and(|url| origin_of(url) == origin)
            || self
                .allowed_origins
                .iter()
                .any(|allowed| origin_of(allowed) == origin)
    }
}

/// `scheme://host[:port]` of a URL, in lower case.
fn origin_of(url: &str) -> String {
    let url = url.trim().to_ascii_lowercase();
    match url.split_once("://") {
        Some((scheme, rest)) => {
            format!("{scheme}://{}", rest.split('/').next().unwrap_or_default())
        }
        None => url,
    }
}

fn is_loopback_host(host: &str) -> bool {
    let name = host_name(host);
    name == "localhost"
        || name.ends_with(".localhost")
        || name
            .parse::<IpAddr>()
            .is_ok_and(|address| Network::of(address) == Network::Loopback)
}

/// A `Host` without its port; IPv6 addresses lose their brackets.
fn host_name(host: &str) -> String {
    let host = host.trim();
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        match host.rsplit_once(':') {
            Some((name, port))
                if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) =>
            {
                name
            }
            _ => host,
        }
    };
    name.trim_end_matches('.').to_ascii_lowercase()
}

/// Where a request came from, after trusted proxies.
#[derive(Debug, Clone)]
pub struct Requester {
    /// The requester's address; `None` when unknown.
    pub address: Option<IpAddr>,
    /// Whether it reached the server (or the trusted proxy) over HTTPS.
    pub https: bool,
    /// The host it asked for (`Host`, or a trusted proxy's
    /// `X-Forwarded-Host`).
    pub host: Option<String>,
    pub user_agent: Option<String>,
}

impl Requester {
    /// The network the requester is on; an unknown address counts as public.
    pub fn network(&self) -> Network {
        self.address.map_or(Network::Public, Network::of)
    }

    /// HTTPS, this computer, or a tailnet (which `WireGuard` encrypts).
    pub fn encrypted(&self) -> bool {
        self.https || matches!(self.network(), Network::Loopback | Network::Tailnet)
    }

    /// May a credential travel over this connection? Encrypted, or plain
    /// HTTP on a home or office network.
    pub fn safe_for_credentials(&self) -> bool {
        self.encrypted() || self.network() == Network::Private
    }

    /// What a client's record keeps of this request.
    pub fn seen(&self) -> Seen {
        Seen {
            address: self
                .address
                .map(|address| address.to_canonical().to_string()),
            encrypted: self.encrypted(),
        }
    }

    /// This request's own origin, such as `https://ahab.example`.
    pub fn own_origin(&self) -> String {
        format!(
            "{}://{}",
            if self.https { "https" } else { "http" },
            self.host.as_deref().unwrap_or("localhost")
        )
    }

    /// Reads where `headers` came from, `peer` being the connection's
    /// address, believing forwarding headers only from `trusted` peers.
    pub fn read(headers: &HeaderMap, peer: Option<IpAddr>, trusted: &[IpRange]) -> Self {
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let mut requester = Self {
            address: peer,
            https: false,
            host: text(header::HOST.as_str()),
            user_agent: text(header::USER_AGENT.as_str()),
        };
        let is_trusted = |address: IpAddr| trusted.iter().any(|range| range.contains(address));
        if !peer.is_some_and(is_trusted) {
            return requester;
        }
        if let Some(forwarded) = text("forwarded") {
            let elements: Vec<Vec<(String, String)>> = forwarded
                .split(',')
                .map(|element| {
                    element
                        .split(';')
                        .filter_map(|pair| pair.split_once('='))
                        .map(|(key, value)| {
                            (
                                key.trim().to_ascii_lowercase(),
                                value.trim().trim_matches('"').to_owned(),
                            )
                        })
                        .collect()
                })
                .collect();
            let find = |element: &Vec<(String, String)>, key: &str| {
                element
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.clone())
            };
            let hops: Vec<String> = elements.iter().filter_map(|e| find(e, "for")).collect();
            if let Some(address) = requester_of(&hops, &is_trusted) {
                requester.address = Some(address);
            }
            if let Some(proto) = elements.first().and_then(|e| find(e, "proto")) {
                requester.https = proto.eq_ignore_ascii_case("https");
            }
            if let Some(host) = elements.first().and_then(|e| find(e, "host")) {
                requester.host = Some(host);
            }
            return requester;
        }
        if let Some(chain) = text("x-forwarded-for") {
            let hops: Vec<String> = chain.split(',').map(|hop| hop.trim().to_owned()).collect();
            if let Some(address) = requester_of(&hops, &is_trusted) {
                requester.address = Some(address);
            }
        }
        if let Some(proto) = text("x-forwarded-proto") {
            let first = proto.split(',').next().unwrap_or_default().trim();
            requester.https = first.eq_ignore_ascii_case("https");
        }
        if let Some(host) = text("x-forwarded-host") {
            let first = host.split(',').next().unwrap_or_default().trim();
            if !first.is_empty() {
                requester.host = Some(first.to_owned());
            }
        }
        requester
    }
}

/// The requester in a chain of hops (`X-Forwarded-For`, `Forwarded: for=`):
/// the nearest one that is not a trusted proxy, reading from the right.
fn requester_of(hops: &[String], is_trusted: &impl Fn(IpAddr) -> bool) -> Option<IpAddr> {
    let addresses: Vec<IpAddr> = hops.iter().filter_map(|hop| parse_hop(hop)).collect();
    addresses
        .iter()
        .rev()
        .find(|address| !is_trusted(**address))
        .or_else(|| addresses.first())
        .copied()
}

/// An address in a forwarding header: `192.0.2.1`, `[2001:db8::1]:4711`,
/// `192.0.2.1:4711`.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim().trim_matches('"');
    if let Ok(address) = hop.parse::<IpAddr>() {
        return Some(address);
    }
    if let Some(rest) = hop.strip_prefix('[') {
        return rest.split(']').next()?.parse().ok();
    }
    hop.parse::<SocketAddr>().ok().map(|address| address.ip())
}

/// Who made a request: a paired client, a connected machine, or nobody (a
/// public route).
#[derive(Debug, Clone)]
pub enum Caller {
    Client {
        client: Arc<Client>,
        /// Turns true when the client is revoked.
        revoked: watch::Receiver<bool>,
    },
    Machine(Uuid),
    Nobody,
}

impl Caller {
    /// The client making the request.
    pub fn client(&self) -> Option<&Client> {
        match self {
            Self::Client { client, .. } => Some(client),
            _ => None,
        }
    }

    pub fn client_id(&self) -> Option<Uuid> {
        self.client().map(|client| client.id)
    }

    /// The machine whose connection's token made the request.
    pub fn machine(&self) -> Option<Uuid> {
        match self {
            Self::Machine(machine) => Some(*machine),
            _ => None,
        }
    }

    /// Whether the caller may act on `machine`: a machine's token reaches
    /// only its own machine; clients reach every machine.
    pub fn may_reach(&self, machine: Uuid) -> Result<(), ApiError> {
        match self {
            Self::Machine(own) if *own != machine => {
                tracing::warn!(caller = %own, target = %machine, "refused a machine's call for another machine");
                Err(ApiError::forbidden(
                    "other_machine_not_allowed",
                    "A machine's plugins may only use terminals, projects and plugin commands \
                     on their own machine.",
                ))
            }
            _ => Ok(()),
        }
    }

    /// Completes when the calling client is revoked; never for others.
    pub async fn revoked(&self) {
        match self {
            Self::Client { revoked, .. } => {
                let mut revoked = revoked.clone();
                // The sender gone means the client is gone too.
                let _ = revoked.wait_for(|revoked| *revoked).await;
            }
            _ => std::future::pending().await,
        }
    }

    /// Runs `work` (a stream or socket) until it ends or the calling client
    /// is revoked, whichever comes first.
    pub async fn until_revoked(&self, work: impl Future<Output = ()>) {
        tokio::select! {
            () = work => {}
            () = self.revoked() => {
                tracing::info!(client = ?self.client_id(), "closed a revoked client's stream");
            }
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(parts
            .extensions
            .get::<Self>()
            .cloned()
            .unwrap_or(Self::Nobody)))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Requester {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(parts.extensions.get::<Self>().cloned().unwrap_or(
            Self {
                address: None,
                https: false,
                host: None,
                user_agent: None,
            },
        )))
    }
}

/// Whether `path` matches `pattern`, where `{…}` matches one segment.
pub fn matches(pattern: &str, path: &str) -> bool {
    let mut pattern = pattern.split('/');
    let mut path = path.split('/');
    loop {
        match (pattern.next(), path.next()) {
            (None, None) => return true,
            (Some(expected), Some(actual)) => {
                let wildcard = expected.starts_with('{') && expected.ends_with('}');
                if !(wildcard && !actual.is_empty() || expected == actual) {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

/// The public route `method` and `path` are, if any.
pub fn public(method: &Method, path: &str) -> Option<Public> {
    PUBLIC
        .iter()
        .find(|(m, pattern, _)| method.as_str() == *m && matches(pattern, path))
        .map(|(_, _, kind)| *kind)
}

fn machine_route(method: &Method, path: &str) -> bool {
    MACHINE
        .iter()
        .any(|(m, pattern)| method.as_str() == *m && matches(pattern, path))
}

/// The credential a request carries: `Authorization: Bearer`, else the
/// browser's cookie.
fn credential(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|token| !token.is_empty());
    if let Some(token) = bearer {
        return Some(token.to_owned());
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The Set-Cookie header that gives a browser its credential:
/// `HttpOnly`, `SameSite=Lax`, and `Secure` when it came over HTTPS. It
/// lasts as long as the client: until revoked.
pub fn set_cookie(credential: &str, requester: &Requester) -> String {
    let secure = if requester.https { "; Secure" } else { "" };
    format!("{COOKIE}={credential}; Path=/; HttpOnly; SameSite=Lax; Max-Age=315360000{secure}")
}

/// The Set-Cookie header that removes the credential cookie.
pub fn clear_cookie(requester: &Requester) -> String {
    let secure = if requester.https { "; Secure" } else { "" };
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}")
}

/// The guards, as middleware. See the module's documentation.
pub async fn guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let access = &state.access;
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let requester = Requester::read(request.headers(), peer, &access.trusted_proxies);
    request.extensions_mut().insert(requester.clone());

    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let api = path == "/api" || path.starts_with("/api/");
    let public = public(&method, &path);
    if public == Some(Public::Health) {
        return next.run(request).await;
    }

    // Other websites.
    if api && let Some(refusal) = check_origin(access, &requester, request.headers(), &method) {
        return refusal;
    }

    // Credentials over plain HTTP to public addresses.
    let needs_safe_connection = !matches!(public, Some(Public::Daemon));
    if needs_safe_connection && !requester.safe_for_credentials() && !access.allow_insecure_http {
        return insecure(&requester, api);
    }
    if !api || public.is_some() {
        return next.run(request).await;
    }

    let Some(secret) = credential(request.headers()) else {
        return not_paired();
    };
    if machine_route(&method, &path)
        && let Some(machine) = state.presence.machine_for_token(&secret)
    {
        request.extensions_mut().insert(Caller::Machine(machine));
        return next.run(request).await;
    }
    let client = match state.clients.authenticate(&secret, &requester.seen()).await {
        Ok(Some(client)) => client,
        Ok(None) => return not_paired(),
        Err(error) => return ApiError::from(error).into_response(),
    };
    let revoked = state.clients.revocation(client.id);
    let caller = Caller::Client {
        client: Arc::new(client),
        revoked,
    };
    request.extensions_mut().insert(caller.clone());
    let response = next.run(request).await;
    end_streams_on_revoke(response, caller)
}

/// A streamed answer (server-sent events, a search) ends when its client is
/// revoked. Answers of a known size are left as they are.
fn end_streams_on_revoke(response: Response, caller: Caller) -> Response {
    if response.body().size_hint().exact().is_some() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let stream = body
        .into_data_stream()
        .take_until(async move { caller.revoked().await });
    Response::from_parts(parts, Body::from_stream(stream))
}

fn not_paired() -> Response {
    ApiError::unauthorized(
        "not_paired",
        "This client is not paired with this server. In a browser, open Ahab to pair it; on \
         the command line, run `ahab login <server address>`.",
    )
    .into_response()
}

/// The refusal of a request from another website, if it is one.
fn check_origin(
    access: &Access,
    requester: &Requester,
    headers: &HeaderMap,
    method: &Method,
) -> Option<Response> {
    let refuse = |detail: String| {
        tracing::warn!(%detail, "refused a request from another website");
        Some(ApiError::forbidden("origin_not_allowed", detail).into_response())
    };
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("null");
        if origin == "null" || !access.origin_allowed(origin, requester) {
            return refuse(format!(
                "Requests from {origin} are not allowed. If it is your own app, add it to \
                 allowed_origins in the server's config.toml (or AHAB_ALLOWED_ORIGINS)."
            ));
        }
        return None;
    }
    // Without an Origin, browsers still say when a request crosses sites.
    let changes = !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    let socket = headers
        .get(header::UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let cross_site = headers
        .get("sec-fetch-site")
        .is_some_and(|value| value.as_bytes() == b"cross-site");
    if cross_site && (changes || socket) {
        return refuse("Requests from other websites are not allowed.".to_owned());
    }
    None
}

fn insecure(requester: &Requester, api: bool) -> Response {
    let address = requester
        .address
        .map_or_else(|| "an unknown address".to_owned(), |a| a.to_string());
    tracing::warn!(%address, "refused plain HTTP from a public address");
    let detail = format!(
        "This connection is plain HTTP from {address}, a public address: anyone along the way \
         could read what is sent, including this client's credential. Reach Ahab over HTTPS \
         (for example behind a reverse proxy listed in trusted_proxies), over a VPN or \
         tailnet, or on your home network. To allow it anyway, set allow_insecure_http = true \
         in the server's config.toml (or AHAB_ALLOW_INSECURE_HTTP=true)."
    );
    if api {
        return ApiError::forbidden("insecure_connection", detail).into_response();
    }
    let page = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
         <title>Not a safe connection</title>\
         <body style=\"font-family:system-ui;max-width:40rem;margin:4rem auto;padding:0 1rem;\
         line-height:1.5\"><h1>Not a safe connection</h1><p>{}</p></body>",
        detail
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    );
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(page))
        .unwrap_or_else(|_| StatusCode::FORBIDDEN.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, value.parse().unwrap());
        }
        map
    }

    #[test]
    fn matches_paths_by_segment() {
        assert!(matches(
            "/api/v1/pairing-requests/{id}",
            "/api/v1/pairing-requests/abc"
        ));
        assert!(!matches(
            "/api/v1/pairing-requests/{id}",
            "/api/v1/pairing-requests/abc/approve"
        ));
        assert!(!matches(
            "/api/v1/pairing-requests/{id}",
            "/api/v1/pairing-requests/"
        ));
        assert!(!matches("/api/v1/health", "/api/v1/health/x"));
        assert_eq!(public(&Method::GET, "/api/v1/health"), Some(Public::Health));
        assert_eq!(public(&Method::POST, "/api/v1/health"), None);
        assert_eq!(public(&Method::GET, "/api/v1/pairing-requests"), None);
    }

    #[test]
    fn believes_forwarding_headers_only_from_trusted_proxies() {
        let trusted = crate::shared::net::LOOPBACK.to_vec();
        let forwarded = headers(&[
            ("host", "localhost:8180"),
            ("x-forwarded-for", "203.0.113.9, 127.0.0.1"),
            ("x-forwarded-proto", "https"),
            ("x-forwarded-host", "ahab.example"),
        ]);
        let through_proxy =
            Requester::read(&forwarded, Some("127.0.0.1".parse().unwrap()), &trusted);
        assert_eq!(through_proxy.address, Some("203.0.113.9".parse().unwrap()));
        assert!(through_proxy.https);
        assert_eq!(through_proxy.own_origin(), "https://ahab.example");

        let direct = Requester::read(&forwarded, Some("198.51.100.4".parse().unwrap()), &trusted);
        assert_eq!(direct.address, Some("198.51.100.4".parse().unwrap()));
        assert!(!direct.https);
        assert_eq!(direct.own_origin(), "http://localhost:8180");

        let standard = headers(&[(
            "forwarded",
            "for=\"[2001:db8::17]:4711\";proto=https;host=ahab.example, for=127.0.0.1",
        )]);
        let requester = Requester::read(&standard, Some("::1".parse().unwrap()), &trusted);
        assert_eq!(requester.address, Some("2001:db8::17".parse().unwrap()));
        assert!(requester.https);
        assert_eq!(requester.host.as_deref(), Some("ahab.example"));
    }

    #[test]
    fn applies_the_connections_rule() {
        let requester = |address: &str, https: bool| Requester {
            address: Some(address.parse().unwrap()),
            https,
            host: None,
            user_agent: None,
        };
        assert!(requester("127.0.0.1", false).encrypted());
        assert!(requester("100.64.1.2", false).encrypted());
        assert!(!requester("192.168.1.2", false).encrypted());
        assert!(requester("192.168.1.2", false).safe_for_credentials());
        assert!(!requester("203.0.113.9", false).safe_for_credentials());
        assert!(requester("203.0.113.9", true).safe_for_credentials());
        assert_eq!(
            requester("::ffff:10.1.2.3", false)
                .seen()
                .address
                .as_deref(),
            Some("10.1.2.3")
        );
    }

    #[test]
    fn reads_the_credential_from_the_header_or_the_cookie() {
        assert_eq!(
            credential(&headers(&[("authorization", "Bearer ahab_x")])).as_deref(),
            Some("ahab_x")
        );
        assert_eq!(
            credential(&headers(&[("cookie", "theme=dark; ahab_device=ahab_y")])).as_deref(),
            Some("ahab_y")
        );
        assert_eq!(credential(&headers(&[("cookie", "ahab_device=")])), None);
        assert_eq!(credential(&HeaderMap::new()), None);
    }

    #[test]
    fn allows_the_own_public_and_listed_origins() {
        let access = Access {
            public_url: Some("https://ahab.example".into()),
            allowed_origins: vec!["tauri://localhost".into()],
            ..Access::default()
        };
        let requester = Requester {
            address: None,
            https: false,
            host: Some("127.0.0.1:8180".into()),
            user_agent: None,
        };
        assert!(access.origin_allowed("http://127.0.0.1:8180", &requester));
        assert!(access.origin_allowed("https://AHAB.example", &requester));
        assert!(access.origin_allowed("tauri://localhost", &requester));
        assert!(!access.origin_allowed("https://evil.example", &requester));
        assert!(!access.origin_allowed("http://127.0.0.1:5173", &requester));
    }

    #[test]
    fn links_point_where_another_client_reaches_the_server() {
        let requester = |host: &str| Requester {
            address: None,
            https: false,
            host: Some(host.into()),
            user_agent: None,
        };
        let plain = Access {
            outward_url: Some("http://192.168.1.4:8180".into()),
            ..Access::default()
        };
        assert_eq!(
            plain.link_base(&requester("127.0.0.1:8180")),
            "http://192.168.1.4:8180"
        );
        assert_eq!(
            plain.link_base(&requester("ahab.lan:8180")),
            "http://ahab.lan:8180"
        );
        let public = Access {
            public_url: Some("https://ahab.example".into()),
            ..plain
        };
        assert_eq!(
            public.link_base(&requester("127.0.0.1:8180")),
            "https://ahab.example"
        );
    }
}
