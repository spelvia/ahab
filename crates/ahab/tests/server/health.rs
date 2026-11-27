use axum::http::StatusCode;
use serde_json::json;

use crate::support::TestApp;

#[tokio::test]
async fn answers_with_its_version() {
    let app = TestApp::start().await;

    let response = app.get("/api/v1/health").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body,
        json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })
    );
}
