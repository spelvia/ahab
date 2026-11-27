//! Searching sessions: titles, and the messages kept from the chats'
//! finished items (`itemEnd`), every word of the query, newest first.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

async fn project_with_chat(server: &TestServer, title: &str) -> (String, String, String) {
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": "/work/app" }),
        )
        .await;
    let project_id = project["id"].as_str().unwrap().to_owned();
    let project = project["id"].as_str().unwrap().to_owned();
    let (status, session) = server
        .post(
            &format!("/projects/{project}/sessions"),
            json!({ "chat": { "agent": "claude-code", "message": title } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    (
        project_id,
        session["id"].as_str().unwrap().to_owned(),
        session["chats"][0]["id"].as_str().unwrap().to_owned(),
    )
}

async fn search_until(server: &TestServer, project: &str, q: &str, count: usize) -> Value {
    for _ in 0..200 {
        let (status, found) = server
            .get(&format!("/projects/{project}/session-search?q={q}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{found}");
        if found["items"].as_array().unwrap().len() == count {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never found {count} for {q}");
}

#[tokio::test]
async fn finds_titles_and_finished_messages_with_every_word() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon =
        FakeDaemon::connect_with(&server, &key, Some(ahab::daemon::dispatch::METHODS)).await;
    server.online(&machine).await;
    let (project, session, chat) = project_with_chat(&server, "Fix the flaky test").await;

    daemon.send(1, &chat, Some(1), json!({ "type": "itemEnd", "item": {
        "type": "userMessage", "content": [{ "type": "text", "text": "Why does the Linux CI fail?" }] } }));
    // Streamed text is never kept, so never found.
    daemon.send(
        2,
        &chat,
        Some(2),
        json!({ "type": "itemDelta", "item": {
        "type": "agentMessage", "text": "The Postgres container" } }),
    );
    daemon.send(3, &chat, Some(3), json!({ "type": "itemEnd", "item": {
        "type": "agentMessage", "text": "The Postgres container starts too late on CI, so the 100% wait_for check times out." } }));
    daemon.send(
        4,
        &chat,
        Some(4),
        json!({ "type": "itemEnd", "item": {
        "type": "toolCall", "title": "postgres logs" } }),
    );

    let found = search_until(&server, &project, "postgres%20CI", 1).await;
    let hit = &found["items"][0];
    assert_eq!(hit["match"], "agentMessage");
    assert_eq!(hit["sessionId"], session.as_str());
    assert_eq!(hit["chatId"], chat.as_str());
    assert_eq!(hit["seq"], 3);
    assert_eq!(hit["sessionTitle"], "Fix the flaky test");
    assert!(
        hit["snippet"]
            .as_str()
            .unwrap()
            .contains("Postgres container")
    );

    // Any case; and the person's messages too, newest first.
    let found = search_until(&server, &project, "ci", 2).await;
    let matches: Vec<&str> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["match"].as_str().unwrap())
        .collect();
    assert_eq!(matches, ["agentMessage", "userMessage"]);

    // Titles come first.
    let found = search_until(&server, &project, "FLAKY", 1).await;
    assert_eq!(found["items"][0]["match"], "title");
    assert!(found["items"][0]["chatId"].is_null());

    // `%` and `_` are words like any other, not patterns.
    search_until(&server, &project, "100%25%20wait_for", 1).await;
    search_until(&server, &project, "wait%25for", 0).await;
    // A word that is nowhere, or none at all, finds nothing.
    search_until(&server, &project, "postgres%20mysql", 0).await;
    search_until(&server, &project, "%20%20", 0).await;

    let (status, _) = server.get("/projects/nope/session-search?q=ci").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_session_forgets_its_messages() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon =
        FakeDaemon::connect_with(&server, &key, Some(ahab::daemon::dispatch::METHODS)).await;
    server.online(&machine).await;
    let (project, session, chat) = project_with_chat(&server, "Tidy up").await;
    daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "itemEnd", "item": {
        "type": "agentMessage", "text": "Removed the unused helper." } }),
    );
    search_until(&server, &project, "helper", 1).await;

    let (status, body) = server
        .post(&format!("/sessions/{session}/archive"), json!({}))
        .await;
    assert!(status.is_success(), "{status} {body}");
    let (status, body) = server.delete(&format!("/sessions/{session}")).await;
    assert!(status.is_success(), "{status} {body}");
    search_until(&server, &project, "helper", 0).await;
}
