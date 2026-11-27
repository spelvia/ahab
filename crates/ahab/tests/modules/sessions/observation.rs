//! A chat's subagents and background tasks (Stubb's observation): the latest
//! picture from its machine reaches whoever watches, even one who connects
//! later, and is never kept.

use std::time::Duration;

use serde_json::json;

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

async fn read_until(response: &mut reqwest::Response, wanted: &str) -> String {
    let mut text = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !text.contains(wanted) {
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .unwrap_or_else(|_| panic!("never saw {wanted}: {text}"));
        text.push_str(&String::from_utf8_lossy(&chunk.unwrap().unwrap()));
    }
    text
}

#[tokio::test]
async fn the_latest_subagents_and_tasks_reach_a_late_watcher_and_go_when_the_chat_stops() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon =
        FakeDaemon::connect_with(&server, &key, Some(ahab::daemon::dispatch::METHODS)).await;
    server.online(&machine).await;
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": "/work/app" }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();
    let (_, session) = server
        .post(
            &format!("/projects/{project}/sessions"),
            json!({ "chat": { "agent": "claude-code", "message": "Fix it" } }),
        )
        .await;
    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );

    let picture = |tasks: serde_json::Value| {
        json!({ "type": "observation", "rootAgentId": "root",
            "agents": [
                { "id": "root", "parent": { "type": "root" }, "status": "running" },
                { "id": "sub", "parent": { "type": "agent", "id": "root" }, "name": "explorer", "status": "running" }
            ],
            "tasks": tasks })
    };
    daemon.send(0, &chat, None, picture(json!([])));
    daemon.send(0, &chat, None, picture(json!([{ "id": "cmd", "kind": { "type": "command", "command": "pnpm dev" }, "status": "running", "background": true }])));
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Connecting now: the history, then the latest picture only.
    let mut response = server
        .http()
        .get(format!("{}/api/v1/chats/{chat}/stream", server.url))
        .send()
        .await
        .unwrap();
    let text = read_until(&mut response, "pnpm dev").await;
    assert_eq!(text.matches("\"observation\"").count(), 1, "{text}");
    assert!(text.contains("explorer"));
    drop(response);

    // Never kept.
    let (_, events) = server.get(&format!("/chats/{chat}/events")).await;
    assert!(!events.to_string().contains("observation"));

    // A stop is scoped to this chat and its native task. Acceptance is not completion.
    let (status, receipt) = server
        .post(
            &format!("/chats/{chat}/stop-work"),
            json!({"type":"task","id":"cmd"}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(receipt["disposition"], "accepted");
    assert_eq!(
        daemon.asked_for("chats.stopWork")[0],
        json!({"chat":chat,"target":{"type":"task","id":"cmd"}})
    );
    let (status, modes) = server.get(&format!("/chats/{chat}/modes")).await;
    assert_eq!(status, 200);
    assert_eq!(modes["available"][0]["id"], "ask");

    // Stopped: forgotten.
    daemon.send(
        2,
        &chat,
        Some(2),
        json!({ "type": "closed", "reason": { "type": "agent" } }),
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut response = server
        .http()
        .get(format!("{}/api/v1/chats/{chat}/stream", server.url))
        .send()
        .await
        .unwrap();
    let text = read_until(&mut response, "\"closed\"").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!text.contains("observation"), "{text}");
}
