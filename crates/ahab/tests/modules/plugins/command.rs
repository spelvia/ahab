//! Plugins' commands, run on the machine through the API.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

/// Echo installed, enabled and running on the server's own machine.
async fn running_echo(server: &TestServer) -> tempfile::TempDir {
    let copy = echo_copy();
    server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    server.post("/plugins/echo/enable", json!({})).await;
    for _ in 0..200 {
        let (_, states) = server.get("/plugin-states").await;
        if states["items"][0]["state"] == "running" {
            return copy;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("echo did not start");
}

async fn run(server: &TestServer, name: &str, body: Value) -> (StatusCode, Value) {
    server
        .post(&format!("/plugins/echo/commands/{name}"), body)
        .await
}

#[tokio::test]
async fn a_command_runs_on_the_machine_and_its_errors_are_told() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;

    // On the only machine it runs on, or on the one named.
    let (status, said) = run(&server, "say", json!({ "input": { "text": "hi" } })).await;
    assert_eq!(status, StatusCode::OK, "{said}");
    assert_eq!(said["output"], json!({ "text": "hi", "machine": "atlas" }));
    let (_, named) = run(
        &server,
        "say",
        json!({ "input": { "text": "there" }, "machine": machine.id }),
    )
    .await;
    assert_eq!(named["output"]["text"], "there");

    let (status, failed) = run(&server, "fail", json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(failed["code"], "command_failed");
    assert_eq!(failed["detail"], "Echo was asked to fail.");
    assert!(failed.get("pluginCode").is_none(), "{failed}");
    let (status, coded) = run(
        &server,
        "fail",
        json!({ "input": { "code": "echo_refused" } }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(coded["code"], "command_failed");
    assert_eq!(coded["pluginCode"], "echo_refused", "{coded}");
    let (status, unknown) = run(&server, "nope", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(unknown["code"], "command_unknown");
    let (status, refused) = run(&server, "say", json!({ "input": { "text": 3 } })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    server.post("/plugins/echo/disable", json!({})).await;
    let (status, disabled) = run(&server, "say", json!({ "input": { "text": "hi" } })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(disabled["code"], "plugin_disabled");
}

#[tokio::test]
async fn a_command_for_a_project_gets_its_folder() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].clone();
    let (status, answered) = run(&server, "where", json!({ "project": project })).await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(answered["output"]["folder"], json!(path));
}

#[tokio::test]
async fn closing_the_request_cancels_the_command_in_the_plugin() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;
    let gave_up = server
        .http()
        .post(format!("{}/api/v1/plugins/echo/commands/slow", server.url))
        .json(&json!({ "input": { "ms": 60_000 } }))
        .timeout(Duration::from_millis(500))
        .send()
        .await;
    assert!(gave_up.is_err(), "the client gave up waiting");
    for _ in 0..100 {
        let (_, logs) = server
            .get(&format!("/plugins/echo/logs?machine={}", machine.id))
            .await;
        if logs.to_string().contains("slow was cancelled") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the plugin was not told");
}

#[tokio::test]
async fn a_plugin_calls_commands_and_starts_its_own_terminals() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;

    // A plugin's call to a plugin's command goes through Ahab.
    let (status, relayed) = run(&server, "relay", json!({ "input": { "text": "round" } })).await;
    assert_eq!(status, StatusCode::OK, "{relayed}");
    assert_eq!(
        relayed["output"],
        json!({ "text": "round", "machine": "atlas" })
    );

    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].clone();
    let (status, opened) = run(&server, "terminal", json!({ "project": project })).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["output"]["command"], r"echo 'hi there' 'it'\''s'");
    assert_eq!(opened["output"]["listed"], 1);
    let (_, mine) = server.get("/terminals?plugin=echo").await;
    assert_eq!(mine["items"][0]["plugin"], "echo");
    assert_eq!(mine["items"][0]["title"], json!(null));
    let (_, theirs) = server.get("/terminals?plugin=docker").await;
    assert_eq!(theirs["items"], json!([]));
}
