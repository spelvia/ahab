//! Calling the API from tests.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use ahab::modules::machines::machine::store::DbMachineRepository;
use ahab::server::router;
use ahab::server::start::build_state_with;
use ahab::server::state::AppState;
use ahab::server::web::WebApp;
use ahab::shared::time::FixedClock;
use time::macros::datetime;

use super::TestDatabase;

/// The whole HTTP application on a test database, at a fixed time.
pub struct TestApp {
    router: Router,
    /// This computer's credential: every request sends it unless
    /// it says otherwise.
    pub credential: String,
    /// Where the local credential file is.
    pub credential_file: std::path::PathBuf,
    /// The services, to set things up behind the API.
    pub state: AppState,
    /// The time the services see; move it with `advance`.
    pub clock: FixedClock,
    database: TestDatabase,
    /// Where plugins are installed, removed with the test.
    _plugin_dir: tempfile::TempDir,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    /// The JSON body, or `null` when there is none or it is not JSON.
    pub body: Value,
    /// The body as text, for responses that are not JSON.
    pub text: String,
}

impl TestApp {
    /// The application on a fresh, migrated database.
    pub async fn start() -> Self {
        Self::with_web(WebApp::Off).await
    }

    /// The application serving the web app as `web` says.
    pub async fn with_web(web: WebApp) -> Self {
        let database = TestDatabase::new().await;
        let clock = FixedClock::at(datetime!(2026-11-13 9:00 UTC));
        let plugin_dir = tempfile::tempdir().unwrap();
        let state = build_state_with(
            database.db.clone(),
            std::sync::Arc::new(clock.clone()),
            ahab::modules::machines::MachineKey::from_secret([200; 32]),
            ahab::modules::plugins::PluginPaths {
                store: plugin_dir.path().join("store"),
                node: None,
            },
        );
        let credential_file = plugin_dir.path().join("credential");
        state.clients.ensure_local(&credential_file).await.unwrap();
        let credential = std::fs::read_to_string(&credential_file)
            .unwrap()
            .trim()
            .to_owned();
        Self {
            router: router::router(state.clone(), &web),
            credential,
            credential_file,
            state,
            clock,
            database,
            _plugin_dir: plugin_dir,
        }
    }

    /// The machines table, to add machines the API cannot create yet.
    pub fn machines(&self) -> DbMachineRepository {
        DbMachineRepository::new(self.database.db.clone())
    }

    pub async fn post(&self, uri: &str, body: Value) -> TestResponse {
        self.send(Method::POST, uri, Some(body)).await
    }

    pub async fn put(&self, uri: &str, body: Value) -> TestResponse {
        self.send(Method::PUT, uri, Some(body)).await
    }

    pub async fn patch(&self, uri: &str, body: Value) -> TestResponse {
        self.send(Method::PATCH, uri, Some(body)).await
    }

    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.send(Method::DELETE, uri, None).await
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.send(Method::GET, uri, None).await
    }

    /// The application, answering also to `names` (`allowed_hosts`).
    pub async fn with_allowed_hosts(names: &[&str]) -> Self {
        let mut app = Self::start().await;
        app.state.allowed_hosts = std::sync::Arc::new(ahab::server::hosts::AllowedHosts::new(
            names.iter().copied(),
        ));
        app.router = router::router(app.state.clone(), &WebApp::Off);
        app
    }

    /// The application with these guard settings.
    pub async fn with_access(access: ahab::server::access::Access) -> Self {
        let mut app = Self::start().await;
        app.state.access = std::sync::Arc::new(access);
        app.router = router::router(app.state.clone(), &WebApp::Off);
        app
    }

    /// Sends `request` as it is: no credential is added. Without a
    /// `ConnectInfo`, it comes from this computer.
    pub async fn raw(&self, mut request: Request<Body>) -> TestResponse {
        if request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .is_none()
        {
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                std::net::SocketAddr::from(([127, 0, 0, 1], 40_000)),
            ));
        }
        self.respond_raw(request).await
    }

    /// `GET uri`, addressed to `host`.
    pub async fn get_at(&self, host: &str, uri: &str) -> TestResponse {
        let request = Request::builder()
            .uri(uri)
            .header(header::HOST, host)
            .body(Body::empty())
            .unwrap();
        self.respond(request).await
    }

    /// `method uri` with `body`, without a credential.
    pub async fn anonymous(&self, method: Method, uri: &str, body: Option<Value>) -> TestResponse {
        let request = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(json) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => request.body(Body::empty()),
        };
        self.raw(request.unwrap()).await
    }

    /// `method uri` with `body`, sending `credential`.
    pub async fn as_client(
        &self,
        credential: &str,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> TestResponse {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {credential}"));
        let request = match body {
            Some(json) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => request.body(Body::empty()),
        };
        self.raw(request.unwrap()).await
    }

    async fn send(&self, method: Method, uri: &str, body: Option<Value>) -> TestResponse {
        let request = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(json) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => request.body(Body::empty()),
        };
        self.respond(request.unwrap()).await
    }

    /// Sends `request` with this computer's credential, unless it carries
    /// one.
    async fn respond(&self, mut request: Request<Body>) -> TestResponse {
        if !request.headers().contains_key(header::AUTHORIZATION) {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                format!("Bearer {}", self.credential).parse().unwrap(),
            );
        }
        self.raw(request).await
    }

    async fn respond_raw(&self, request: Request<Body>) -> TestResponse {
        let response = self.router.clone().oneshot(request).await.unwrap();

        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|kind| kind.contains("json"));
        let body = if bytes.is_empty() || !json {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        TestResponse {
            status,
            headers,
            body,
            text: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }
}

/// A request from `peer` (`192.168.1.20`), with `headers`, and a JSON body
/// when given. No credential unless a header says so.
pub fn request_from(
    peer: &str,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let address: std::net::IpAddr = peer.parse().unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::new(
            address, 50_000,
        )));
    request
}

pub const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0";
