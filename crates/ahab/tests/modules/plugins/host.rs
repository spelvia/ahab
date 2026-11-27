//! Plugins' daemon entries running on a machine: synced, started under
//! Node, restarted, stopped, with their states and logs.

use std::path::PathBuf;

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

/// Echo's state on the machines, as the API reports it.
async fn state(server: &TestServer) -> Option<Value> {
    let (_, states) = server.get("/plugin-states").await;
    states["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["plugin"] == "echo").cloned())
}

/// Waits until echo's state on the machine satisfies `wanted`.
async fn until(
    server: &TestServer,
    what: &str,
    wanted: impl Fn(Option<&Value>) -> bool,
) -> Option<Value> {
    for _ in 0..200 {
        let now = state(server).await;
        if wanted(now.as_ref()) {
            return now;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!(
        "timed out waiting until {what}; last: {:?}",
        state(server).await
    );
}

fn is(state: &str) -> impl Fn(Option<&Value>) -> bool + '_ {
    move |now| now.is_some_and(|now| now["state"] == state)
}

#[tokio::test]
async fn an_enabled_plugin_runs_on_the_machine_and_stops_when_disabled() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let copy = echo_copy();
    let (status, installed) = server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{installed}");
    assert!(state(&server).await.is_none(), "not while disabled");

    server.post("/plugins/echo/enable", json!({})).await;
    let running = until(&server, "echo runs", is("running")).await.unwrap();
    assert_eq!(running["machine"], json!(machine.id));
    assert_eq!(running["digest"], installed["plugin"]["digest"]);
    let commands: Vec<&str> = running["declared"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|command| command["name"].as_str())
        .collect();
    assert_eq!(
        commands,
        [
            "say",
            "slow",
            "where",
            "relay",
            "terminal",
            "greet",
            "fail",
            "notify",
            "delivered"
        ]
    );
    assert_eq!(
        running["declared"]["hooks"],
        json!(["terminals.create", "notifications.send"])
    );
    assert_eq!(running["declared"]["events"], json!(["echo.said"]));

    let (_, logs) = server
        .get(&format!("/plugins/echo/logs?machine={}", machine.id))
        .await;
    let lines = logs["lines"].as_array().unwrap();
    assert!(
        lines.iter().any(|line| line
            .as_str()
            .unwrap()
            .ends_with("[info] echo is running on atlas")),
        "{logs}"
    );

    // A new build replaces the running one.
    std::fs::write(
        copy.path().join("dist/web.js"),
        "export default () => {};\n",
    )
    .unwrap();
    let (_, updated) = server.post("/plugins/echo/update", json!({})).await;
    let digest = updated["plugin"]["digest"].clone();
    until(&server, "the new build runs", |now| {
        now.is_some_and(|now| now["state"] == "running" && now["digest"] == digest)
    })
    .await;

    server.post("/plugins/echo/disable", json!({})).await;
    until(&server, "echo stops", |now| now.is_none()).await;
}

#[tokio::test]
async fn a_crashing_plugin_is_restarted() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let copy = echo_copy();
    std::fs::write(
        copy.path().join("dist/daemon.js"),
        "console.error('nope'); process.exit(3);\n",
    )
    .unwrap();
    server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    server.post("/plugins/echo/enable", json!({})).await;
    let restarting = until(&server, "echo restarts", is("restarting"))
        .await
        .unwrap();
    assert!(
        restarting["message"]
            .as_str()
            .unwrap()
            .contains("starting again in"),
        "{restarting}"
    );
}

#[tokio::test]
async fn without_node_a_plugin_is_unavailable() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, Some(PathBuf::from("/nowhere/node"))).await;
    let copy = echo_copy();
    server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    server.post("/plugins/echo/enable", json!({})).await;
    let unavailable = until(&server, "echo is unavailable", is("unavailable"))
        .await
        .unwrap();
    assert!(
        unavailable["message"]
            .as_str()
            .unwrap()
            .contains("[plugins] node"),
        "{unavailable}"
    );
}
