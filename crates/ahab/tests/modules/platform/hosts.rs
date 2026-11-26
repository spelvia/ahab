//! The host check: the server answers only to the names it is
//! meant to be reached by.

use axum::http::StatusCode;

use crate::support::TestApp;

#[tokio::test]
async fn a_name_not_in_allowed_hosts_is_refused() {
    let app = TestApp::start().await;

    let api = app.get_at("ahab.example.internal", "/api/v1/health").await;
    assert_eq!(api.status, StatusCode::MISDIRECTED_REQUEST);
    assert_eq!(api.body["code"], "host_not_allowed");
    assert!(
        api.body["detail"]
            .as_str()
            .unwrap()
            .contains("allowed_hosts")
    );

    let page = app.get_at("rebound.example:8180", "/").await;
    assert_eq!(page.status, StatusCode::MISDIRECTED_REQUEST);
    assert!(page.text.contains(r#"allowed_hosts = ["rebound.example"]"#));
}

#[tokio::test]
async fn localhost_addresses_and_allowed_names_are_answered() {
    let app = TestApp::with_allowed_hosts(&["ahab.example.internal"]).await;
    for host in [
        "localhost:8180",
        "127.0.0.1:8180",
        "[::1]:8180",
        "192.168.178.22:8180",
        "ahab.example.internal",
    ] {
        let response = app.get_at(host, "/api/v1/health").await;
        assert_eq!(response.status, StatusCode::OK, "{host}");
    }
    let other = app.get_at("evil.example", "/api/v1/health").await;
    assert_eq!(other.status, StatusCode::MISDIRECTED_REQUEST);
}
