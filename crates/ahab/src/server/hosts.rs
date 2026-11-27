//! The host check: the server answers only to the names it is
//! meant to be reached by.
//!
//! A web page can make a browser talk to `localhost` under a name of its own
//! (DNS rebinding), and a reverse proxy forwards whatever name it serves. Both
//! arrive with a `Host` this server was never told about, and are refused
//! with `421 host_not_allowed`. Answered: `localhost` and its subdomains,
//! IP addresses (rebinding needs a name), and the names in `allowed_hosts`.

use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ApiError;

/// The names the server answers to besides `localhost` and IP addresses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedHosts {
    names: Vec<String>,
}

impl AllowedHosts {
    /// `names` as given in `allowed_hosts`; case and a trailing dot are
    /// ignored, as DNS ignores them.
    pub fn new(names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            names: names
                .into_iter()
                .map(|name| normalise(name.as_ref()))
                .filter(|name| !name.is_empty())
                .collect(),
        }
    }

    /// Whether a request with this `Host` (with or without a port) may be
    /// answered.
    pub fn allows(&self, host: &str) -> bool {
        let name = normalise(strip_port(host));
        if name.is_empty() {
            return false;
        }
        if name == "localhost" || name.ends_with(".localhost") {
            return true;
        }
        if name.parse::<IpAddr>().is_ok() {
            return true;
        }
        self.names.contains(&name)
    }
}

/// `host` without its port; IPv6 addresses lose their brackets.
fn strip_port(host: &str) -> &str {
    let host = host.trim();
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    match host.rsplit_once(':') {
        // A bare IPv6 address has several colons and no port.
        Some((name, port)) if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => {
            name
        }
        _ => host,
    }
}

fn normalise(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Refuses requests addressed to a name the server does not answer to.
/// Requests without any `Host` pass: a browser always sends one.
pub async fn check(
    State(allowed): State<Arc<AllowedHosts>>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            request
                .uri()
                .authority()
                .map(axum::http::uri::Authority::as_str)
        });
    match host {
        Some(host) if !allowed.allows(host) => refuse(host, request.uri().path()),
        _ => next.run(request).await,
    }
}

fn refuse(host: &str, path: &str) -> Response {
    let name = strip_port(host).to_owned();
    tracing::warn!(host = %name, "refused a request for a host not in allowed_hosts");
    let detail = format!(
        "This Ahab server does not answer to \"{name}\". If you reach it through \
         that name on purpose, add it to allowed_hosts in the server's config.toml \
         (or AHAB_ALLOWED_HOSTS) and restart it."
    );
    if path.starts_with("/api/") {
        return ApiError::misdirected("host_not_allowed", detail).into_response();
    }
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>Host not allowed</title>\
         <body style=\"font-family:system-ui;max-width:40rem;margin:4rem auto;line-height:1.5\">\
         <h1>Host not allowed</h1><p>{}</p>\
         <pre>allowed_hosts = [\"{}\"]</pre></body>",
        html_escape(&detail),
        html_escape(&name)
    );
    Response::builder()
        .status(StatusCode::MISDIRECTED_REQUEST)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(page))
        .unwrap_or_else(|_| StatusCode::MISDIRECTED_REQUEST.into_response())
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::AllowedHosts;

    #[test]
    fn localhost_and_addresses_are_always_answered() {
        let hosts = AllowedHosts::default();
        for host in [
            "localhost",
            "localhost:8180",
            "LOCALHOST.",
            "app.localhost:5173",
            "127.0.0.1:8180",
            "192.168.178.22",
            "[::1]:8180",
            "::1",
            "100.101.102.103:8180",
        ] {
            assert!(hosts.allows(host), "{host}");
        }
    }

    #[test]
    fn other_names_only_when_listed() {
        let hosts = AllowedHosts::new(["Ahab.Example.Internal."]);
        assert!(hosts.allows("ahab.example.internal"));
        assert!(hosts.allows("ahab.example.internal:443"));
        assert!(!hosts.allows("evil.example"));
        assert!(!hosts.allows("docs.ahab.example.internal"));
        assert!(!hosts.allows("localhost.evil.example"));
        assert!(!hosts.allows(""));
    }
}
