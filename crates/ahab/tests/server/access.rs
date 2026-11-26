//! The guards: every route needs a paired client's credential
//! unless it is deliberately public; other websites, plain HTTP from public
//! addresses and forwarding headers from strangers are refused or ignored.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use ahab::server::access::{Access, MACHINE, PUBLIC, Public};
use ahab::shared::net::LOOPBACK;

use crate::support::{TestApp, request_from as from};

/// Every route under `/api/` as (method, path): the `OpenAPI` document's,
/// and the plain routes (`.route("/api/…", get(…))`) found in the source,
/// so a new stream or socket is checked too.
fn every_route() -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    let document = serde_json::to_value(ahab::server::router::openapi()).unwrap();
    for (path, operations) in document["paths"].as_object().unwrap() {
        for method in operations.as_object().unwrap().keys() {
            routes.insert((method.to_uppercase(), path.clone()));
        }
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in rust_files(&source) {
        let text: String = std::fs::read_to_string(&file)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut rest = text.as_str();
        while let Some(at) = rest
            .find(".route( \"/api/")
            .or_else(|| rest.find(".route(\"/api/"))
        {
            rest = &rest[at..];
            let start = rest.find('"').unwrap() + 1;
            let end = start + rest[start..].find('"').unwrap();
            let path = rest[start..end].to_owned();
            let after = &rest[end..];
            let method = ["get", "post", "put", "patch", "delete", "any"]
                .into_iter()
                .filter_map(|method| after.find(&format!("{method}(")).map(|i| (i, method)))
                .min()
                .map(|(_, method)| method.to_uppercase())
                .unwrap();
            routes.insert((method, path));
            rest = &rest[end..];
        }
    }
    // Mounted by libraries.
    routes.insert(("GET".to_owned(), "/api/docs".to_owned()));
    routes
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// `path` with every `{…}` segment filled in.
fn concrete(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.starts_with("{*") {
                "anything"
            } else if segment.starts_with('{') {
                "0196a1b2-0000-7000-8000-000000000001"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn the_public_routes_are_the_ones_the_rfc_lists() {
    let public: Vec<(&str, &str)> = PUBLIC.iter().map(|(m, p, _)| (*m, *p)).collect();
    assert_eq!(
        public,
        vec![
            ("GET", "/api/v1/health"),
            ("POST", "/api/v1/pair"),
            ("POST", "/api/v1/pairing-requests"),
            ("GET", "/api/v1/pairing-requests/{id}"),
            ("POST", "/api/v1/enrollments"),
            ("GET", "/api/v1/enrollments/{id}"),
            ("GET", "/api/v1/daemon/connect"),
        ],
        "a public route is a deliberate choice: change the security docs and this list together"
    );
    let routes = every_route();
    for (method, path) in PUBLIC
        .iter()
        .map(|(m, p, _)| (*m, *p))
        .chain(MACHINE.iter().copied())
    {
        assert!(
            routes.contains(&(method.to_owned(), path.to_owned())),
            "{method} {path} is listed but is not a route"
        );
    }
}

#[tokio::test]
async fn every_other_route_needs_a_paired_client() {
    let app = TestApp::start().await;
    let routes = every_route();
    assert!(routes.len() > 100, "found {} routes", routes.len());
    for (method, path) in routes {
        let public = PUBLIC.iter().any(|(m, p, _)| *m == method && *p == path);
        let method = if method == "ANY" {
            "GET".to_owned()
        } else {
            method
        };
        let response = app
            .anonymous(
                Method::from_bytes(method.as_bytes()).unwrap(),
                &concrete(&path),
                Some(json!({})),
            )
            .await;
        if public {
            assert_ne!(
                response.status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} is public"
            );
        } else {
            assert_eq!(
                (response.status, response.body["code"].as_str()),
                (StatusCode::UNAUTHORIZED, Some("not_paired")),
                "{method} {path} answers without a credential: make it authenticated, or \
                 add it to PUBLIC in server/access.rs on purpose"
            );
        }
    }
}

#[tokio::test]
async fn the_web_app_and_health_need_no_credential() {
    let app = TestApp::start().await;
    let health = app.anonymous(Method::GET, "/api/v1/health", None).await;
    assert_eq!(health.status, StatusCode::OK);
    let page = app.anonymous(Method::GET, "/pair", None).await;
    assert_ne!(page.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn other_websites_are_refused() {
    let app = TestApp::with_access(Access {
        allowed_origins: vec!["tauri://localhost".to_owned()],
        public_url: Some("https://ahab.example".to_owned()),
        ..Access::default()
    })
    .await;
    let bearer = format!("Bearer {}", app.credential);
    let post = |origin: &'static str, headers: Vec<(&'static str, String)>| {
        let mut all: Vec<(&str, String)> = vec![
            ("host", "localhost:8180".to_owned()),
            ("authorization", bearer.clone()),
        ];
        if !origin.is_empty() {
            all.push(("origin", origin.to_owned()));
        }
        all.extend(headers);
        let pairs: Vec<(&str, &str)> = all.iter().map(|(n, v)| (*n, v.as_str())).collect();
        from(
            "127.0.0.1",
            Method::POST,
            "/api/v1/projects",
            &pairs,
            Some(json!({ "name": "x" })),
        )
    };
    let evil = app.raw(post("https://evil.example", vec![])).await;
    assert_eq!(evil.status, StatusCode::FORBIDDEN);
    assert_eq!(evil.body["code"], "origin_not_allowed");
    let null = app.raw(post("null", vec![])).await;
    assert_eq!(null.body["code"], "origin_not_allowed");
    for allowed in [
        "http://localhost:8180",
        "https://ahab.example",
        "tauri://localhost",
    ] {
        let response = app.raw(post(allowed, vec![])).await;
        assert_ne!(response.status, StatusCode::FORBIDDEN, "{allowed}");
    }
    // Without an Origin, a browser still says it crossed sites.
    let cross = app
        .raw(post("", vec![("sec-fetch-site", "cross-site".to_owned())]))
        .await;
    assert_eq!(cross.body["code"], "origin_not_allowed");
    // Scripts send no Origin at all.
    let script = app.raw(post("", vec![])).await;
    assert_ne!(script.status, StatusCode::FORBIDDEN);

    // A stream or socket from another website.
    let socket = app
        .raw(from(
            "127.0.0.1",
            Method::GET,
            "/api/v1/plugin-events",
            &[
                ("host", "localhost:8180"),
                ("origin", "https://evil.example"),
                ("authorization", &bearer),
            ],
            None,
        ))
        .await;
    assert_eq!(socket.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn plain_http_from_a_public_address_is_refused() {
    let app = TestApp::start().await;
    let bearer = format!("Bearer {}", app.credential);
    let api = app
        .raw(from(
            "203.0.113.9",
            Method::GET,
            "/api/v1/machines",
            &[("authorization", &bearer)],
            None,
        ))
        .await;
    assert_eq!(api.status, StatusCode::FORBIDDEN);
    assert_eq!(api.body["code"], "insecure_connection");
    assert!(
        api.body["detail"]
            .as_str()
            .unwrap()
            .contains("allow_insecure_http")
    );
    let pairing = app
        .raw(from(
            "203.0.113.9",
            Method::POST,
            "/api/v1/pairing-requests",
            &[],
            Some(json!({})),
        ))
        .await;
    assert_eq!(pairing.body["code"], "insecure_connection");
    let page = app
        .raw(from("203.0.113.9", Method::GET, "/", &[], None))
        .await;
    assert_eq!(page.status, StatusCode::FORBIDDEN);
    assert!(page.text.contains("Not a safe connection"));

    // Health and daemons are not refused: daemons prove themselves.
    let health = app
        .raw(from(
            "203.0.113.9",
            Method::GET,
            "/api/v1/health",
            &[],
            None,
        ))
        .await;
    assert_eq!(health.status, StatusCode::OK);
    let enroll = app
        .raw(from(
            "203.0.113.9",
            Method::POST,
            "/api/v1/enrollments",
            &[],
            Some(json!({})),
        ))
        .await;
    assert_ne!(enroll.status, StatusCode::FORBIDDEN);

    // Home networks and tailnets are fine.
    for peer in [
        "192.168.178.40",
        "10.0.0.5",
        "100.100.1.1",
        "fd7a:115c:a1e0::5",
    ] {
        let response = app
            .raw(from(
                peer,
                Method::GET,
                "/api/v1/machines",
                &[("authorization", &bearer)],
                None,
            ))
            .await;
        assert_eq!(response.status, StatusCode::OK, "{peer}");
    }

    let allowed = TestApp::with_access(Access {
        allow_insecure_http: true,
        ..Access::default()
    })
    .await;
    let bearer = format!("Bearer {}", allowed.credential);
    let anyway = allowed
        .raw(from(
            "203.0.113.9",
            Method::GET,
            "/api/v1/machines",
            &[("authorization", &bearer)],
            None,
        ))
        .await;
    assert_eq!(anyway.status, StatusCode::OK);
}

#[tokio::test]
async fn forwarding_headers_count_only_from_trusted_proxies() {
    let app = TestApp::with_access(Access {
        trusted_proxies: vec!["10.0.0.2".parse().unwrap()],
        ..Access::default()
    })
    .await;
    let bearer = format!("Bearer {}", app.credential);
    // Through the proxy, over HTTPS, from the internet: allowed, encrypted.
    let proxied = app
        .raw(from(
            "10.0.0.2",
            Method::GET,
            "/api/v1/clients/current",
            &[
                ("authorization", &bearer),
                ("x-forwarded-for", "203.0.113.9"),
                ("x-forwarded-proto", "https"),
            ],
            None,
        ))
        .await;
    assert_eq!(proxied.status, StatusCode::OK, "{}", proxied.body);
    assert_eq!(proxied.body["lastAddress"], "203.0.113.9");
    assert_eq!(proxied.body["encrypted"], true);
    // From a stranger, the same headers are ignored: plain HTTP from a
    // public address.
    let spoofed = app
        .raw(from(
            "198.51.100.7",
            Method::GET,
            "/api/v1/machines",
            &[
                ("authorization", &bearer),
                ("x-forwarded-for", "192.168.1.5"),
                ("x-forwarded-proto", "https"),
            ],
            None,
        ))
        .await;
    assert_eq!(spoofed.body["code"], "insecure_connection");
    // Loopback is no longer trusted here.
    assert!(!LOOPBACK.is_empty());

    // A browser paired through an HTTPS proxy gets a Secure cookie.
    let secret = app.post("/api/v1/pairings", json!({})).await.body["secret"].clone();
    let paired = app
        .raw(from(
            "10.0.0.2",
            Method::POST,
            "/api/v1/pair",
            &[
                ("x-forwarded-for", "203.0.113.9"),
                ("x-forwarded-proto", "https"),
            ],
            Some(json!({ "secret": secret })),
        ))
        .await;
    let cookie = paired.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("; Secure"), "{cookie}");
}

#[tokio::test]
async fn the_rate_limits_count_the_address_behind_a_trusted_proxy() {
    let app = TestApp::start().await;
    let through = |client: &'static str| {
        from(
            "127.0.0.1",
            Method::POST,
            "/api/v1/pair",
            &[("x-forwarded-for", client)],
            Some(json!({ "code": "ZZZZ-ZZZZ" })),
        )
    };
    for _ in 0..10 {
        app.raw(through("192.168.1.80")).await;
    }
    assert_eq!(
        app.raw(through("192.168.1.80")).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        app.raw(through("192.168.1.81")).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn public_is_classified_by_method_and_path() {
    assert_eq!(
        ahab::server::access::public(&Method::GET, "/api/v1/pairing-requests/abc"),
        Some(Public::Pairing)
    );
    assert_eq!(
        ahab::server::access::public(&Method::POST, "/api/v1/pairing-requests/abc/approve"),
        None
    );
}

/// A machine's token (its plugins' calls) reaches only its own machine:
/// its projects, its terminals, its plugin commands. Clients reach all.
#[tokio::test]
async fn a_machine_token_reaches_only_its_own_machine() {
    use crate::support::{FakeDaemon, TestServer};
    use ahab::modules::machines::machine::store::MachineRepository;
    use ahab::modules::machines::{Machine, MachineKey, MachineName};
    use ahab::shared::time::Clock;

    let server = TestServer::start().await;
    let key_a = MachineKey::from_secret([31; 32]);
    let a = server.local_machine(&key_a, "atlas").await;
    let key_b = MachineKey::from_secret([32; 32]);
    let b = Machine::joined(
        MachineName::parse("box").unwrap(),
        key_b.public_key(),
        server.clock.now(),
    );
    server.machines().insert(&b).await.unwrap();
    let daemon_a = FakeDaemon::connect(&server, &key_a).await;
    let _daemon_b = FakeDaemon::connect(&server, &key_b).await;
    server.online(&a).await;
    server.online(&b).await;
    let token = daemon_a.api_token.clone().expect("a token in the welcome");

    let mut projects = Vec::new();
    for (name, machine) in [("on-a", "atlas"), ("on-b", "box")] {
        let (status, project) = server
            .post(
                "/projects",
                json!({ "name": name, "machine": machine, "path": format!("/work/{name}") }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{project}");
        projects.push(project["id"].as_str().unwrap().to_owned());
    }
    let (on_a, on_b) = (&projects[0], &projects[1]);

    let http = reqwest::Client::new();
    let as_a = |method: reqwest::Method, path: String, body: Option<serde_json::Value>| {
        let mut request = http
            .request(method, format!("{}/api/v1{path}", server.url))
            .bearer_auth(&token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        async move {
            let response = request.send().await.unwrap();
            let status = response.status().as_u16();
            (
                status,
                response
                    .json::<serde_json::Value>()
                    .await
                    .unwrap_or_default(),
            )
        }
    };
    let refused = |(status, body): (u16, serde_json::Value)| {
        assert_eq!(status, 403, "{body}");
        assert_eq!(body["code"], "other_machine_not_allowed", "{body}");
    };
    let terminal = json!({ "plugin": "echo", "command": "true" });

    // Terminals: only in its own machine's projects.
    refused(
        as_a(
            reqwest::Method::POST,
            format!("/projects/{on_b}/terminals"),
            Some(terminal.clone()),
        )
        .await,
    );
    let (status, own) = as_a(
        reqwest::Method::POST,
        format!("/projects/{on_a}/terminals"),
        Some(terminal.clone()),
    )
    .await;
    assert_eq!(status, 201, "{own}");
    let own = own["terminal"]["id"].as_str().unwrap().to_owned();

    // A client starts one on the other machine, for the same plugin.
    let (status, other) = server
        .post(&format!("/projects/{on_b}/terminals"), terminal.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{other}");
    let other = other["terminal"]["id"].as_str().unwrap().to_owned();

    // Listing shows only its own machine's; the client sees both.
    let (status, listed) = as_a(reqwest::Method::GET, "/terminals?plugin=echo".into(), None).await;
    assert_eq!(status, 200, "{listed}");
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|terminal| terminal["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![own.as_str()]);
    let (_, all) = server.get("/terminals?plugin=echo").await;
    assert_eq!(all["items"].as_array().unwrap().len(), 2);

    // Getting and killing: its own, not the other machine's.
    refused(as_a(reqwest::Method::GET, format!("/terminals/{other}"), None).await);
    refused(
        as_a(
            reqwest::Method::POST,
            format!("/terminals/{other}/kill"),
            Some(json!({})),
        )
        .await,
    );
    assert_eq!(
        as_a(reqwest::Method::GET, format!("/terminals/{own}"), None)
            .await
            .0,
        200
    );
    assert_eq!(
        as_a(
            reqwest::Method::POST,
            format!("/terminals/{own}/kill"),
            Some(json!({})),
        )
        .await
        .0,
        200
    );
    assert_eq!(
        server.get(&format!("/terminals/{other}")).await.0,
        StatusCode::OK
    );

    // Plugin commands: not on the other machine, nor in its project.
    let command = "/plugins/echo/commands/say".to_owned();
    refused(
        as_a(
            reqwest::Method::POST,
            command.clone(),
            Some(json!({ "machine": b.id })),
        )
        .await,
    );
    refused(
        as_a(
            reqwest::Method::POST,
            command.clone(),
            Some(json!({ "project": on_b })),
        )
        .await,
    );
    // Its own machine (named, or by default) gets past the check: here
    // to "no such plugin".
    for body in [
        json!({ "machine": a.id }),
        json!({ "project": on_a }),
        json!({}),
    ] {
        let (status, answer) = as_a(reqwest::Method::POST, command.clone(), Some(body)).await;
        assert_eq!(status, 404, "{answer}");
        assert_eq!(answer["code"], "plugin_not_found", "{answer}");
    }
}
