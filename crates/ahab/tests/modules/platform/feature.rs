//! Core features: on unless switched off, for everyone; Sessions always on.

use reqwest::StatusCode;
use serde_json::json;

use crate::support::TestServer;

#[tokio::test]
async fn core_features_switch_off_and_on_but_sessions_stays() {
    let server = TestServer::start().await;
    let (status, listed) = server.get("/features").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["sessions", "terminal", "files", "git", "search"]);
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["enabled"] == true)
    );

    let (status, changed) = server
        .put("/features/git", json!({ "enabled": false }))
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["items"][3]["enabled"], false);
    let (_, listed) = server.get("/features").await;
    assert_eq!(listed["items"][3]["enabled"], false, "kept");
    server
        .put("/features/git", json!({ "enabled": true }))
        .await;
    let (_, listed) = server.get("/features").await;
    assert_eq!(listed["items"][3]["enabled"], true);

    let (status, _) = server
        .put("/features/sessions", json!({ "enabled": false }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, problem) = server
        .put("/features/chat", json!({ "enabled": false }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "feature_not_found");
}
