//! The `terminals.create` hook: a plugin changes how a terminal starts, or
//! refuses it.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

#[tokio::test]
async fn a_plugin_changes_or_refuses_how_a_terminal_starts() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
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
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();

    // Echo runs the command its context asks for instead of the shell.
    let (status, session) = server
        .post(
            &format!("/projects/{project}/terminals"),
            json!({ "context": { "echo": { "command": "printf hooked-ok; sleep 5" } } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["terminal"]["hookedBy"], json!(["echo"]));
    let terminal = session["terminal"]["id"].as_str().unwrap();
    let base = server.url.replacen("http", "ws", 1);
    let mut socket = server
        .ws(&format!("{base}/api/v1/terminals/{terminal}/attach"))
        .await
        .unwrap();
    let mut seen = String::new();
    let read = async {
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Binary(bytes) = message {
                seen.push_str(&String::from_utf8_lossy(&bytes));
                if seen.contains("hooked-ok") {
                    return;
                }
            }
        }
    };
    assert!(
        tokio::time::timeout(Duration::from_secs(10), read)
            .await
            .is_ok(),
        "the hooked command did not run: {seen:?}"
    );

    // A plain terminal is left alone.
    let (_, plain) = server
        .post(&format!("/projects/{project}/terminals"), json!({}))
        .await;
    assert_eq!(plain["terminal"]["hookedBy"], json!([]));

    // A refusal says who refused and why, and leaves nothing behind.
    let (_, before) = server.get("/projects/app/sessions").await;
    let (status, refused) = server
        .post(
            &format!("/projects/{project}/terminals"),
            json!({ "context": { "echo": { "command": "deny" } } }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["code"], "terminal_denied");
    assert_eq!(refused["detail"], "The plugin echo refused: Echo says no.");
    let (_, after) = server.get("/projects/app/sessions").await;
    assert_eq!(before, after);
}

#[tokio::test]
async fn a_hook_the_manifest_does_not_list_is_never_asked() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let copy = echo_copy();
    let path = copy.path().join("ahab-plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["hooks"] = json!([]);
    std::fs::write(&path, manifest.to_string()).unwrap();
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
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();

    // Echo would refuse this terminal, but it is not asked.
    let (status, session) = server
        .post(
            &format!("/projects/{project}/terminals"),
            json!({ "context": { "echo": { "command": "deny" } } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["terminal"]["hookedBy"], json!([]));
    let (_, logs) = server
        .get(&format!("/plugins/echo/logs?machine={}", machine.id))
        .await;
    assert!(
        logs.to_string()
            .contains("which its manifest does not list"),
        "{logs}"
    );
}
