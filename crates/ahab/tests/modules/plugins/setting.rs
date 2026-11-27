//! Plugins' settings: declared by the plugin, kept per scope, given to its
//! daemon entry and its commands.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

async fn greet(server: &TestServer, body: Value) -> Value {
    let (status, answer) = server.post("/plugins/echo/commands/greet", body).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    answer["output"].clone()
}

#[tokio::test]
async fn settings_apply_by_scope_and_reach_the_plugin() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let copy = echo_copy();
    let (_, installed) = server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    assert_eq!(
        installed["plugin"]["manifest"]["settings"]["properties"]["greeting"]["default"], "hello",
        "the schema file is read into the manifest"
    );
    server.post("/plugins/echo/enable", json!({})).await;
    for _ in 0..200 {
        let (_, states) = server.get("/plugin-states").await;
        if states["items"][0]["state"] == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        greet(&server, json!({})).await,
        json!({ "machine": "hello", "call": "hello" })
    );

    // The server's value reaches the running plugin.
    let (status, saved) = server
        .put(
            "/plugins/echo/settings?scope=server",
            json!({ "values": { "greeting": "hi" } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["values"], json!({ "greeting": "hi" }));
    for _ in 0..100 {
        if greet(&server, json!({})).await["machine"] == "hi" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        greet(&server, json!({})).await,
        json!({ "machine": "hi", "call": "hi" })
    );

    // A project's own value, for commands run for it.
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();
    server
        .put(
            &format!("/plugins/echo/settings?scope=project&id={project}"),
            json!({ "values": { "loud": true } }),
        )
        .await;
    assert_eq!(
        greet(&server, json!({ "project": project })).await,
        json!({ "machine": "hi", "call": "HI" })
    );
    let (_, effective) = server
        .get(&format!(
            "/plugins/echo/settings/effective?project={project}"
        ))
        .await;
    assert_eq!(
        effective["values"],
        json!({ "greeting": "hi", "loud": true })
    );
    let (_, own) = server
        .get(&format!(
            "/plugins/echo/settings/effective?machine={}",
            machine.id
        ))
        .await;
    assert_eq!(own["values"], json!({ "greeting": "hi", "loud": false }));

    let (status, _) = server
        .put(
            "/plugins/echo/settings?scope=machine",
            json!({ "values": {} }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "which machine?");
}
