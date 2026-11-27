use axum::http::StatusCode;
use serde_json::json;

use crate::support::TestApp;

#[tokio::test]
async fn answers_an_unknown_path_with_a_problem() {
    let app = TestApp::start().await;

    let response = app.get("/api/v1/nothing-here").await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.headers["content-type"], "application/problem+json");
    assert_eq!(response.body["code"], "not_found");
    assert!(response.body["requestId"].is_string());
}

#[tokio::test]
async fn serves_the_openapi_document() {
    let app = TestApp::start().await;

    let response = app.get("/api/v1/openapi.json").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["info"]["title"], json!("Ahab API"));
}
