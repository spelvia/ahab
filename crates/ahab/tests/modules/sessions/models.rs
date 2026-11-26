//! Switching a chat's model (item 5 of the chat feedback): only its own
//! agent's, at its next message, by resuming the agent's session with it.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::json;

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

async fn chat() -> (TestServer, Arc<FakeDaemon>, String) {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = Arc::new(
        FakeDaemon::connect_with(&server, &key, Some(ahab::daemon::dispatch::METHODS)).await,
    );
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
            json!({ "chat": { "agent": "claude-code", "message": "Fix it", "model": "sonnet" } }),
        )
        .await;
    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    (server, daemon, chat)
}

#[tokio::test]
async fn a_live_chat_goes_on_with_another_model_from_its_next_message() {
    let (server, daemon, chat) = chat().await;
    // Its agent reports itself started (it has a session of its own).
    daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    daemon.send(
        2,
        &chat,
        Some(2),
        json!({ "type": "turnEnd", "turn": { "id": "t1", "status": "completed" } }),
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, model) = server
        .put(
            &format!("/chats/{chat}/model"),
            json!({ "model": "opus", "effort": "high" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{model}");
    assert_eq!(model["model"], "test-model");
    assert_eq!(model["next"]["model"], "opus");
    // Nothing stops until the next message.
    assert!(daemon.asked_for("chats.close").is_empty());

    // The agent stops when asked: the test plays it.
    let playing = Arc::clone(&daemon);
    let stopped_chat = chat.clone();
    tokio::spawn(async move {
        for _ in 0..200 {
            if !playing.asked_for("chats.close").is_empty() {
                playing.send(
                    3,
                    &stopped_chat,
                    Some(3),
                    json!({ "type": "closed", "reason": { "type": "agent" } }),
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    let (status, body) = server
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "text": "Now with Opus" }),
        )
        .await;
    assert!(status.is_success(), "{status} {body}");

    let resumed = daemon.asked_for("chats.resume");
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0]["model"], "opus");
    assert_eq!(resumed[0]["effort"], "high");
    assert_eq!(resumed[0]["agent"], "claude-code");
    assert_eq!(resumed[0]["message"], "Now with Opus");

    // The stop is kept as the switch, not as the agent stopping.
    let (_, events) = server.get(&format!("/chats/{chat}/events")).await;
    let kinds: Vec<&str> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["event"]["type"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"modelChanged"), "{kinds:?}");
    assert!(!kinds.contains(&"closed"), "{kinds:?}");
    let (_, model) = server.get(&format!("/chats/{chat}/model")).await;
    assert!(model["next"].is_null());
    let (_, notifications) = server.get("/notifications").await;
    assert_eq!(notifications["unread"], 0, "{notifications}");
}

#[tokio::test]
async fn a_stopped_chat_resumes_with_the_chosen_model() {
    let (server, daemon, chat) = chat().await;
    daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "closed", "reason": { "type": "agent" } }),
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, model) = server
        .put(&format!("/chats/{chat}/model"), json!({ "model": "haiku" }))
        .await;
    assert_eq!(model["model"], "haiku");
    assert!(model["next"].is_null());
    let (status, _) = server
        .call(
            Method::POST,
            &format!("/chats/{chat}/messages"),
            Some(json!({ "text": "go on" })),
        )
        .await;
    assert!(status.is_success());
    assert_eq!(daemon.asked_for("chats.resume")[0]["model"], "haiku");
}

#[tokio::test]
async fn fast_mode_starts_with_a_chat_and_switches_like_its_model() {
    let (server, daemon, chat) = chat().await;
    // Started without one: the provider's default.
    assert!(daemon.asked_for("chats.start")[0]["fast"].is_null());
    let (_, model) = server.get(&format!("/chats/{chat}/model")).await;
    assert!(model["fast"].is_null());

    daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "closed", "reason": { "type": "agent" } }),
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, model) = server
        .put(&format!("/chats/{chat}/model"), json!({ "fast": true }))
        .await;
    assert_eq!(model["fast"], true);
    // Choosing fast keeps the model it had.
    assert_eq!(model["model"], "test-model");
    server
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "text": "faster" }),
        )
        .await;
    let resumed = daemon.asked_for("chats.resume");
    assert_eq!(resumed[0]["fast"], true);
    assert_eq!(resumed[0]["model"], "test-model");
}

#[tokio::test]
async fn forks_keep_the_chosen_turn_and_resume_the_new_native_chat() {
    let (server, daemon, chat) = chat().await;
    let (_, source) = server.get(&format!("/chats/{chat}")).await;
    for (seq, event) in [
        (1, json!({"type":"turnStart","turnId":"first"})),
        (
            2,
            json!({"type":"turnEnd","turn":{"id":"first","status":"completed"}}),
        ),
        (3, json!({"type":"turnStart","turnId":"later"})),
    ] {
        daemon.send(seq, &chat, Some(i64::try_from(seq).unwrap()), event);
    }
    for _ in 0..100 {
        let (_, events) = server.get(&format!("/chats/{chat}/events")).await;
        if events["items"]
            .as_array()
            .is_some_and(|events| events.len() == 3)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    for new_session in [false, true] {
        let (status, fork) = server
            .post(
                &format!("/chats/{chat}/fork"),
                json!({"seq":2,"newSession":new_session}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{fork}");
        assert_eq!(fork["sessionId"] != source["sessionId"], new_session);
        let id = fork["id"].as_str().unwrap();
        let (_, events) = server.get(&format!("/chats/{id}/events")).await;
        assert_eq!(events["items"].as_array().unwrap().len(), 2);
        assert_eq!(
            daemon.asked_for("chats.fork").last().unwrap()["turn"],
            "first"
        );
        let (status, result) = server
            .post(&format!("/chats/{id}/messages"), json!({"text":"continue"}))
            .await;
        assert!(status.is_success(), "{result}");
        let request = daemon.asked_for("chats.resume").pop().unwrap();
        assert_ne!(request["nativeId"], "native-1");
        assert_eq!(request["after"], 2);
    }
    let (status, _) = server
        .post(
            &format!("/chats/{chat}/fork"),
            json!({"seq":3,"newSession":true}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
