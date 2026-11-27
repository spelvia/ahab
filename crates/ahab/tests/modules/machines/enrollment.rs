//! Joining: `/api/v1/enrollments`, approval and one-time
//! codes.

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use time::Duration;

use ahab::modules::machines::MachineKey;
use ahab::modules::machines::enrollment::model::enroll_message;

use crate::support::TestApp;

/// The request a machine with `key` sends; tests reach the router without a
/// Host header, so it signs an empty host.
fn request(key: &MachineKey, hostname: &str, code: Option<&str>) -> Value {
    let public_key = STANDARD.encode(key.public_key().as_bytes());
    let signature = STANDARD.encode(key.sign(&enroll_message("", &public_key)));
    let mut body = json!({ "publicKey": public_key, "signature": signature, "hostname": hostname });
    if let Some(code) = code {
        body["code"] = json!(code);
    }
    body
}

async fn create_code(app: &TestApp, name: Option<&str>) -> String {
    let body = name.map_or_else(|| json!({}), |name| json!({ "name": name }));
    let response = app.post("/api/v1/join-codes", body).await;
    assert_eq!(response.status, StatusCode::CREATED);
    response.body["code"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_machine_waits_for_approval_then_is_approved() {
    let app = TestApp::start().await;
    let key = MachineKey::from_secret([5; 32]);

    let enrolled = app
        .post("/api/v1/enrollments", request(&key, "Build-Box.lan", None))
        .await;
    assert_eq!(enrolled.status, StatusCode::CREATED);
    assert_eq!(enrolled.body["state"], "pending");
    assert_eq!(enrolled.body["name"], "build-box");
    assert_eq!(enrolled.body["checkCode"], key.public_key().check_code());
    assert_eq!(
        enrolled.body["serverFingerprint"],
        app.state.presence.server_fingerprint()
    );
    let id = enrolled.body["id"].as_str().unwrap().to_owned();
    let status_path = format!("/api/v1/enrollments/{id}");
    assert_eq!(app.get(&status_path).await.body["state"], "pending");

    let approved = app
        .post(&format!("/api/v1/machines/{id}/approve"), json!({}))
        .await;
    assert_eq!(approved.status, StatusCode::OK);
    assert_eq!(approved.body["state"], "active");
    assert_eq!(app.get(&status_path).await.body["state"], "active");

    let again = app
        .post(&format!("/api/v1/machines/{id}/approve"), json!({}))
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["code"], "machine_not_pending");
}

#[tokio::test]
async fn the_same_key_gets_its_request_back() {
    let app = TestApp::start().await;
    let key = MachineKey::from_secret([5; 32]);

    let first = app
        .post("/api/v1/enrollments", request(&key, "box", None))
        .await;
    let second = app
        .post("/api/v1/enrollments", request(&key, "box", None))
        .await;

    assert_eq!(first.body["id"], second.body["id"]);
    assert_eq!(
        app.get("/api/v1/machines").await.body["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn refuses_a_request_not_signed_by_its_key() {
    let app = TestApp::start().await;
    let key = MachineKey::from_secret([5; 32]);
    let mut body = request(&key, "box", None);
    body["signature"] = json!(STANDARD.encode(MachineKey::from_secret([6; 32]).sign(b"x")));

    let response = app.post("/api/v1/enrollments", body).await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.body["code"], "signature_invalid");
}

#[tokio::test]
async fn a_one_time_code_approves_at_once_and_works_once() {
    let app = TestApp::start().await;
    let code = create_code(&app, Some("ci-runner")).await;
    assert_eq!(code.len(), 9);
    assert_eq!(
        app.get("/api/v1/join-codes").await.body["items"][0]["name"],
        "ci-runner"
    );

    // Typed in lower case, as people do.
    let joined = app
        .post(
            "/api/v1/enrollments",
            request(
                &MachineKey::from_secret([5; 32]),
                "whatever",
                Some(&code.to_lowercase()),
            ),
        )
        .await;
    assert_eq!(joined.status, StatusCode::CREATED);
    assert_eq!(joined.body["state"], "active");
    assert_eq!(joined.body["name"], "ci-runner");

    let reused = app
        .post(
            "/api/v1/enrollments",
            request(&MachineKey::from_secret([6; 32]), "other", Some(&code)),
        )
        .await;
    assert_eq!(reused.status, StatusCode::FORBIDDEN);
    assert_eq!(reused.body["code"], "join_code_invalid");
    assert_eq!(app.get("/api/v1/join-codes").await.body["items"], json!([]));
}

#[tokio::test]
async fn a_code_approves_a_machine_that_already_waits() {
    let app = TestApp::start().await;
    let key = MachineKey::from_secret([5; 32]);
    app.post("/api/v1/enrollments", request(&key, "box", None))
        .await;
    let code = create_code(&app, None).await;

    let joined = app
        .post("/api/v1/enrollments", request(&key, "box", Some(&code)))
        .await;

    assert_eq!(joined.body["state"], "active");
}

#[tokio::test]
async fn codes_and_pending_requests_expire() {
    let app = TestApp::start().await;
    let code = create_code(&app, None).await;
    let waiting = app
        .post(
            "/api/v1/enrollments",
            request(&MachineKey::from_secret([5; 32]), "box", None),
        )
        .await;
    let id = waiting.body["id"].as_str().unwrap().to_owned();

    app.clock.advance(Duration::minutes(16));

    let late = app
        .post(
            "/api/v1/enrollments",
            request(&MachineKey::from_secret([6; 32]), "late", Some(&code)),
        )
        .await;
    assert_eq!(late.status, StatusCode::FORBIDDEN);
    let gone = app.get(&format!("/api/v1/enrollments/{id}")).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.body["code"], "enrollment_not_found");
}

#[tokio::test]
async fn limits_how_many_machines_wait() {
    let app = TestApp::start().await;
    for byte in 0..20u8 {
        let key = MachineKey::from_secret([byte; 32]);
        let response = app
            .post(
                "/api/v1/enrollments",
                request(&key, &format!("box-{byte}"), None),
            )
            .await;
        assert_eq!(response.status, StatusCode::CREATED);
    }

    let one_more = app
        .post(
            "/api/v1/enrollments",
            request(&MachineKey::from_secret([99; 32]), "one-more", None),
        )
        .await;

    assert_eq!(one_more.status, StatusCode::CONFLICT);
    assert_eq!(one_more.body["code"], "too_many_pending");
}

#[tokio::test]
async fn withdraws_a_code() {
    let app = TestApp::start().await;
    create_code(&app, None).await;
    let id = app.get("/api/v1/join-codes").await.body["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let deleted = app.delete(&format!("/api/v1/join-codes/{id}")).await;

    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/v1/join-codes").await.body["items"], json!([]));
    assert_eq!(
        app.delete(&format!("/api/v1/join-codes/{id}")).await.status,
        StatusCode::NOT_FOUND
    );
}
