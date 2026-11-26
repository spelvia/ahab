//! Plugins and notifications: Echo sends the kind it
//! declares, its `notifications.send` hook changes or drops notifications,
//! and its destination gets each one kept.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use ahab::modules::notifications::notification::model::{Level, NewNotification};

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

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

async fn echo(server: &TestServer, command: &str, input: Value) -> (StatusCode, Value) {
    server
        .post(
            &format!("/plugins/echo/commands/{command}"),
            json!({ "input": input }),
        )
        .await
}

async fn delivered(server: &TestServer, count: usize) -> Value {
    for _ in 0..200 {
        let (_, got) = echo(server, "delivered", json!({})).await;
        if got["output"]
            .as_array()
            .is_some_and(|got| got.len() >= count)
        {
            return got["output"].clone();
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("Echo's destination did not get {count}");
}

#[tokio::test]
async fn a_plugin_sends_what_it_declared_and_its_destination_gets_it() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;

    // Settings list the kind under the plugin.
    let (_, settings) = server.get("/notification-settings").await;
    let kind = settings["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["id"] == "echo.said")
        .cloned()
        .unwrap_or_else(|| panic!("{settings}"));
    assert_eq!(kind["sourceName"], "Echo");

    let (status, sent) = echo(&server, "notify", json!({ "text": "Hello" })).await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert!(sent["output"]["id"].is_string(), "{sent}");
    let (_, inbox) = server.get("/notifications").await;
    let kept = &inbox["items"][0];
    assert_eq!(kept["kind"], "echo.said");
    assert_eq!(kept["source"], "echo");
    assert_eq!(kept["title"], "Hello");
    assert_eq!(
        kept["target"],
        json!({ "type": "route", "path": "/settings/plugins" })
    );

    let got = delivered(&server, 1).await;
    assert_eq!(got[0]["title"], "Hello");

    // A kind it did not declare is refused.
    let (status, refused) = echo(&server, "notify", json!({ "text": "x", "kind": "other" })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["detail"].as_str().unwrap().contains("not declared"),
        "{refused}"
    );
}

#[tokio::test]
async fn the_hook_changes_or_drops_notifications_even_ahabs_own() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
    let _echo = running_echo(&server).await;

    let (_, shouted) = echo(&server, "notify", json!({ "text": "shout:quiet" })).await;
    assert!(shouted["output"]["id"].is_string(), "{shouted}");
    let (_, dropped) = echo(&server, "notify", json!({ "text": "drop me" })).await;
    assert_eq!(dropped["output"]["id"], Value::Null, "{dropped}");
    let (_, inbox) = server.get("/notifications").await;
    let titles: Vec<&str> = inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect();
    assert!(titles.contains(&"QUIET"), "{titles:?}");
    assert!(!titles.contains(&"drop me"), "{titles:?}");

    // Ahab's own go through the hook too.
    let kept = server
        .state
        .notifications
        .send(NewNotification {
            kind: "sessions.finished".into(),
            source: "ahab".into(),
            level: Level::Success,
            title: "drop me".into(),
            body: None,
            project_id: None,
            target: json!({ "type": "machine", "machineId": "m" }),
            group_key: None,
        })
        .await
        .unwrap();
    assert!(kept.is_none());
}

#[tokio::test]
async fn only_a_machine_the_plugin_runs_on_sends_for_it() {
    let server = TestServer::start().await;
    let (status, refused) = server
        .post(
            "/plugins/echo/notifications",
            json!({ "kind": "said", "title": "x" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
}
