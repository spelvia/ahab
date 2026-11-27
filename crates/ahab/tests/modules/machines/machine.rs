//! `/api/v1/machines`, and the local machine the server creates.

use axum::http::StatusCode;
use serde_json::json;

use ahab::modules::machines::machine::store::MachineRepository;
use ahab::modules::machines::{Machine, MachineName, PublicKey};
use ahab::shared::time::Clock;

use crate::support::TestApp;

fn key(byte: u8) -> PublicKey {
    PublicKey::from_bytes(&[byte; 32]).unwrap()
}

/// A machine that joined, added behind the API (joining comes later).
async fn add_pending(app: &TestApp, name: &str, byte: u8) -> Machine {
    let machine = Machine::pending(
        MachineName::parse(name).unwrap(),
        key(byte),
        app.clock.now(),
    );
    app.machines().insert(&machine).await.unwrap();
    machine
}

#[tokio::test]
async fn lists_no_machines_at_first() {
    let app = TestApp::start().await;

    let response = app.get("/api/v1/machines").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body, json!({ "items": [] }));
}

#[tokio::test]
async fn creates_the_local_machine_from_the_hostname_once() {
    let app = TestApp::start().await;

    let first = app
        .state
        .machines
        .ensure_local(key(1), "Alexs-Laptop.local")
        .await
        .unwrap();
    let again = app
        .state
        .machines
        .ensure_local(key(1), "Alexs-Laptop.local")
        .await
        .unwrap();
    assert_eq!(first, again);

    let response = app.get(&format!("/api/v1/machines/{}", first.id)).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body,
        json!({
            "id": first.id,
            "name": "alexs-laptop",
            "state": "active",
            "isLocal": true,
            "online": false,
            "checkCode": key(1).check_code(),
            "fingerprint": key(1).fingerprint().to_string(),
            "os": null,
            "arch": null,
            "version": null,
            "createdAt": "2026-11-13T09:00:00Z",
            "approvedAt": "2026-11-13T09:00:00Z",
            "lastSeenAt": null,
            "outdated": false,
            "missingMethods": [],
        })
    );
}

#[tokio::test]
async fn keeps_the_local_machine_when_its_key_is_replaced() {
    let app = TestApp::start().await;
    let first = app
        .state
        .machines
        .ensure_local(key(1), "atlas")
        .await
        .unwrap();

    let replaced = app
        .state
        .machines
        .ensure_local(key(2), "atlas")
        .await
        .unwrap();

    assert_eq!(replaced.id, first.id);
    assert_eq!(replaced.public_key, key(2));
    let stored = app.state.machines.get(first.id).await.unwrap();
    assert_eq!(stored.public_key, key(2));
}

#[tokio::test]
async fn numbers_the_local_machine_when_its_name_is_taken() {
    let app = TestApp::start().await;
    add_pending(&app, "atlas", 9).await;

    let local = app
        .state
        .machines
        .ensure_local(key(1), "atlas")
        .await
        .unwrap();

    assert_eq!(local.name.as_str(), "atlas-2");
}

#[tokio::test]
async fn lists_the_local_machine_first_then_by_name() {
    let app = TestApp::start().await;
    add_pending(&app, "zeta", 3).await;
    add_pending(&app, "alpha", 4).await;
    app.state
        .machines
        .ensure_local(key(1), "atlas")
        .await
        .unwrap();

    let response = app.get("/api/v1/machines").await;

    let names: Vec<&str> = response.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|machine| machine["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["atlas", "alpha", "zeta"]);
}

#[tokio::test]
async fn renames_a_machine() {
    let app = TestApp::start().await;
    let machine = add_pending(&app, "laptop", 3).await;

    let response = app
        .patch(
            &format!("/api/v1/machines/{}", machine.id),
            json!({ "name": "work-laptop" }),
        )
        .await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["name"], "work-laptop");
    let stored = app.state.machines.get(machine.id).await.unwrap();
    assert_eq!(stored.name.as_str(), "work-laptop");
}

#[tokio::test]
async fn refuses_an_invalid_or_taken_name() {
    let app = TestApp::start().await;
    let machine = add_pending(&app, "laptop", 3).await;
    add_pending(&app, "build-box", 4).await;
    let path = format!("/api/v1/machines/{}", machine.id);

    let invalid = app.patch(&path, json!({ "name": "Build Box" })).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid.body["code"], "validation_failed");
    assert_eq!(invalid.body["errors"][0]["field"], "name");

    let taken = app.patch(&path, json!({ "name": "build-box" })).await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    assert_eq!(taken.body["code"], "machine_name_taken");

    let unchanged = app.patch(&path, json!({ "name": "laptop" })).await;
    assert_eq!(unchanged.status, StatusCode::OK);
}

#[tokio::test]
async fn removes_a_machine() {
    let app = TestApp::start().await;
    let machine = add_pending(&app, "laptop", 3).await;
    let path = format!("/api/v1/machines/{}", machine.id);

    let removed = app.delete(&path).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);

    let gone = app.get(&path).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.body["code"], "machine_not_found");
    assert_eq!(app.delete(&path).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn keeps_the_local_machine() {
    let app = TestApp::start().await;
    let local = app
        .state
        .machines
        .ensure_local(key(1), "atlas")
        .await
        .unwrap();

    let response = app.delete(&format!("/api/v1/machines/{}", local.id)).await;

    assert_eq!(response.status, StatusCode::CONFLICT);
    assert_eq!(response.body["code"], "machine_is_local");
    assert!(app.state.machines.get(local.id).await.is_ok());
}

#[tokio::test]
async fn answers_not_found_for_an_unknown_or_malformed_id() {
    let app = TestApp::start().await;

    let unknown = app
        .get("/api/v1/machines/0193a7b0-0000-7000-8000-000000000000")
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.body["code"], "machine_not_found");

    let malformed = app.get("/api/v1/machines/not-an-id").await;
    assert_eq!(malformed.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn allows_one_local_machine_only() {
    let app = TestApp::start().await;
    app.state
        .machines
        .ensure_local(key(1), "atlas")
        .await
        .unwrap();

    let second = Machine::local(
        MachineName::parse("other").unwrap(),
        key(2),
        app.clock.now(),
    );

    assert!(app.machines().insert(&second).await.is_err());
}
