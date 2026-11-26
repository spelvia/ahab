//! Sessions through the API, with a fake daemon standing in for the agent:
//! starting, storing events once, acknowledging them, requests,
//! closing, the live stream, and sessions that stopped with their machine.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;
use ahab::modules::sessions::chat::store::ChatRepository;

use crate::support::{FakeDaemon, TestServer};

struct Setup {
    server: TestServer,
    daemon: FakeDaemon,
    key: MachineKey,
    project: String,
    http: reqwest::Client,
}

impl Setup {
    async fn new() -> Self {
        Self::with_capabilities(None).await
    }

    /// With a daemon whose hello lists `capabilities` (none: an old one).
    async fn with_capabilities(capabilities: Option<&[&str]>) -> Self {
        let server = TestServer::start().await;
        let key = MachineKey::from_secret([9; 32]);
        let machine = server.local_machine(&key, "atlas").await;
        let daemon = FakeDaemon::connect_with(&server, &key, capabilities).await;
        server.online(&machine).await;
        let http = server.http();
        let (_, project) = server
            .post(
                "/projects",
                json!({ "name": "app", "machine": "atlas", "path": "/work/app" }),
            )
            .await;
        let project = project["id"].as_str().unwrap().to_owned();
        Self {
            server,
            daemon,
            key,
            project,
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.server.url)
    }

    async fn get(&self, path: &str) -> Value {
        self.server.get(path).await.1
    }

    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.server.post(path, body).await
    }

    /// Starts a session with its first chat; the chat's ID.
    async fn start(&self) -> String {
        let (status, session) = self
            .post(
                &format!("/projects/{}/sessions", self.project),
                json!({ "chat": { "agent": "claude-code", "message": "Fix the flaky test\nmore detail",
                                  "model": "opus", "effort": "high" } }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{session}");
        assert_eq!(session["title"], "Fix the flaky test");
        session["chats"][0]["id"].as_str().unwrap().to_owned()
    }

    /// Waits until the session matches `check`.
    async fn until(&self, id: &str, what: &str, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..100 {
            let session = self.get(&format!("/chats/{id}")).await;
            if check(&session) {
                return session;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("timed out waiting until {what}");
    }
}

#[tokio::test]
async fn a_message_id_can_be_retried_without_sending_to_the_agent_twice() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let path = format!("/chats/{id}/messages");
    let key = uuid::Uuid::now_v7();
    let body = json!({ "text": "Just once", "submissionId": key });
    let (first, overlapping) = tokio::join!(
        setup.post(&path, body.clone()),
        setup.post(&path, body.clone())
    );
    assert!(first.0 == StatusCode::ACCEPTED || overlapping.0 == StatusCode::ACCEPTED);
    assert_eq!(
        setup.post(&path, body.clone()).await.0,
        StatusCode::ACCEPTED
    );
    assert_eq!(setup.daemon.asked_for("chats.send").len(), 1);
    let (status, problem) = setup
        .post(
            &path,
            json!({ "text": "Another message", "submissionId": key }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "submission_changed");
    let Setup {
        mut server, daemon, ..
    } = setup;
    daemon.disconnect();
    server.restart().await;
    assert_eq!(server.post(&path, body).await.0, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn the_agents_echo_of_a_message_carries_its_submission_id() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let key = uuid::Uuid::now_v7();
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/messages"),
            json!({ "text": "Echo me", "submissionId": key }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    daemon.send(2, &id, Some(2), json!({ "type": "itemStart", "turnId": "t1",
        "item": { "type": "userMessage", "id": "u1", "content": [{ "type": "text", "text": "Echo me" }] } }));
    daemon.send(3, &id, Some(3), json!({ "type": "itemEnd", "turnId": "t1",
        "item": { "type": "userMessage", "id": "u1", "content": [{ "type": "text", "text": "Echo me" }] } }));
    daemon.send(4, &id, Some(4), json!({ "type": "itemEnd", "turnId": "t1",
        "item": { "type": "userMessage", "id": "u2", "content": [{ "type": "text", "text": "Not ours" }] } }));
    let mut items = Vec::new();
    for _ in 0..100 {
        items = setup.get(&format!("/chats/{id}/events")).await["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if items.len() >= 4 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let tagged: Vec<Value> = items
        .iter()
        .map(|item| item["event"]["item"]["submissionId"].clone())
        .collect();
    assert_eq!(
        tagged,
        [
            Value::Null,
            json!(key.to_string()),
            json!(key.to_string()),
            Value::Null
        ]
    );
}

#[tokio::test]
async fn an_agents_title_names_the_chat_and_its_session_unless_someone_did() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    setup.daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "title", "title": "Stabilize the login test" }),
    );
    let chat = setup
        .until(&id, "the agent's title", |chat| {
            chat["title"] == "Stabilize the login test"
        })
        .await;
    let session = chat["sessionId"].as_str().unwrap().to_owned();
    let mut shown = Value::Null;
    for _ in 0..100 {
        shown = setup.get(&format!("/sessions/{session}")).await;
        if shown["title"] == "Stabilize the login test" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(shown["title"], "Stabilize the login test");

    // Once someone names the chat, the agent's titles leave it, and the
    // session no longer follows it.
    let (status, _) = setup
        .server
        .call(
            reqwest::Method::PATCH,
            &format!("/chats/{id}"),
            Some(json!({ "title": "Mine" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    setup.daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "title", "title": "Something else" }),
    );
    setup.daemon.send(
        3,
        &id,
        Some(3),
        json!({ "type": "turnAccounting", "turnId": "t1", "tokens": 1, "sessionCostUsd": 0.1 }),
    );
    for _ in 0..100 {
        if setup.get(&format!("/chats/{id}/usage")).await["tokens"] == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(setup.get(&format!("/chats/{id}")).await["title"], "Mine");
    assert_eq!(
        setup.get(&format!("/sessions/{session}")).await["title"],
        "Stabilize the login test"
    );
}

#[tokio::test]
async fn lists_every_turn_of_a_chat_for_the_rail() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    for (seq, text) in [(1_i64, "Fix the flaky test"), (5, "Now the other one")] {
        setup.daemon.send(
            seq.unsigned_abs(),
            &id,
            Some(seq),
            json!({ "type": "itemEnd", "turnId": format!("t{seq}"),
                    "item": { "type": "userMessage", "id": format!("u{seq}"),
                              "content": [{ "type": "text", "text": text }] } }),
        );
    }
    let mut turns = Value::Null;
    for _ in 0..100 {
        turns = setup.get(&format!("/chats/{id}/turns")).await;
        if turns["items"]
            .as_array()
            .is_some_and(|items| items.len() == 2)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(
        turns,
        json!({ "items": [
            { "seq": 1, "text": "Fix the flaky test" },
            { "seq": 5, "text": "Now the other one" },
        ] })
    );
}

#[tokio::test]
async fn a_chats_usage_adds_up_what_its_agent_reported() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    assert_eq!(
        setup.get(&format!("/chats/{id}/usage")).await,
        json!({ "tokens": null, "costUsd": null })
    );
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnAccounting", "turnId": "t1", "tokens": 1200, "sessionCostUsd": 0.5 }),
    );
    daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "turnAccounting", "turnId": "t2", "tokens": 800, "sessionCostUsd": 0.75 }),
    );
    let mut usage = Value::Null;
    for _ in 0..100 {
        usage = setup.get(&format!("/chats/{id}/usage")).await;
        if usage["tokens"] == 2000 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(usage, json!({ "tokens": 2000, "costUsd": 0.75 }));
}

#[tokio::test]
async fn a_lost_daemon_reply_leaves_delivery_unconfirmed_and_retry_does_not_redispatch() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let path = format!("/chats/{id}/messages");
    let body = json!({ "text": "Maybe delivered", "submissionId": uuid::Uuid::now_v7() });
    setup.daemon.withhold("chats.send");
    let http = setup.http.clone();
    let url = setup.url(&path);
    let message = body.clone();
    let send = tokio::spawn(async move { http.post(url).json(&message).send().await.unwrap() });
    asked(&setup.daemon, "chats.send", 1).await;
    let Setup {
        server,
        daemon,
        key,
        ..
    } = setup;
    daemon.disconnect();
    let answer = tokio::time::timeout(Duration::from_secs(5), send)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answer.status(), StatusCode::CONFLICT);
    assert_eq!(
        answer.json::<Value>().await.unwrap()["code"],
        "message_unconfirmed"
    );
    let daemon = FakeDaemon::connect(&server, &key).await;
    let (status, problem) = server.post(&path, body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "message_unconfirmed");
    assert!(daemon.asked_for("chats.send").is_empty());
}

/// Changes the text of waiting message `item` of chat `id`.
async fn edit(setup: &Setup, id: &str, item: uuid::Uuid, text: &str) -> (StatusCode, Value) {
    setup
        .server
        .call(
            reqwest::Method::PATCH,
            &format!("/chats/{id}/queue/{item}"),
            Some(json!({ "text": text })),
        )
        .await
}

#[tokio::test]
async fn waiting_messages_can_be_reordered_and_edited() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let path = format!("/chats/{id}/messages");
    let ids: Vec<uuid::Uuid> = (0..3).map(|_| uuid::Uuid::now_v7()).collect();
    for (text, submission) in ["A", "B", "C"].iter().zip(&ids) {
        let body = json!({ "text": text, "delivery": "queue", "submissionId": submission });
        assert_eq!(setup.post(&path, body).await.0, StatusCode::ACCEPTED);
    }
    let chat: uuid::Uuid = id.parse().unwrap();
    let texts = |snapshot: &Value| -> Vec<String> {
        snapshot["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["text"].as_str().unwrap().to_owned())
            .collect()
    };

    let (status, _) = setup
        .server
        .put(
            &format!("/chats/{id}/queue"),
            json!({ "order": [ids[2], ids[0], ids[1]] }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let snapshot = setup.server.state.chats.queue_event(chat).await.unwrap();
    assert_eq!(texts(&snapshot), ["C", "A", "B"]);

    assert_eq!(
        edit(&setup, &id, ids[1], "B, but better").await.0,
        StatusCode::NO_CONTENT
    );
    let snapshot = setup.server.state.chats.queue_event(chat).await.unwrap();
    assert_eq!(texts(&snapshot), ["C", "A", "B, but better"]);
    assert_eq!(
        edit(&setup, &id, ids[1], "  ").await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        edit(&setup, &id, uuid::Uuid::now_v7(), "X").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn queued_messages_survive_restart_but_an_inflight_message_is_not_replayed() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let path = format!("/chats/{id}/messages");
    let waiting = uuid::Uuid::now_v7();
    let in_flight = uuid::Uuid::now_v7();
    let accepted =
        json!({ "text": "Wait for the next turn", "delivery": "queue", "submissionId": waiting });
    for body in [
        accepted.clone(),
        json!({ "text": "Already being delivered", "delivery": "queue", "submissionId": in_flight }),
    ] {
        assert_eq!(setup.post(&path, body).await.0, StatusCode::ACCEPTED);
    }
    assert!(
        setup
            .server
            .chats()
            .claim_submission(id.parse().unwrap(), in_flight, None)
            .await
            .unwrap()
    );
    let Setup {
        mut server,
        daemon,
        key,
        ..
    } = setup;
    daemon.disconnect();
    server.restart().await;
    assert_eq!(server.post(&path, accepted).await.0, StatusCode::ACCEPTED);
    let snapshot = server
        .state
        .chats
        .queue_event(id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(snapshot["items"][0]["id"], waiting.to_string());
    assert_eq!(snapshot["items"][0]["unconfirmed"], false);
    assert_eq!(snapshot["items"][1]["id"], in_flight.to_string());
    assert_eq!(snapshot["items"][1]["unconfirmed"], true);
    assert_eq!(snapshot["items"].as_array().unwrap().len(), 2);

    let daemon = FakeDaemon::connect(&server, &key).await;
    for _ in 0..100 {
        if server.get(&format!("/chats/{id}")).await.1["state"] == "closed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        server
            .post(&format!("/chats/{id}/queue/{in_flight}/now"), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, body) = server
        .post(&format!("/chats/{id}/queue/{waiting}/now"), json!({}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(
        daemon.asked_for("chats.resume")[0]["message"],
        "Wait for the next turn"
    );
    assert!(daemon.asked_for("chats.send").is_empty());
}

#[tokio::test]
async fn a_session_starts_stores_its_events_once_and_answers_requests() {
    let setup = Setup::new().await;
    let id = setup.start().await;

    let session = setup.get(&format!("/chats/{id}")).await;
    assert_eq!(session["title"], "Fix the flaky test");
    assert_eq!(session["state"], "live");
    let started = setup.daemon.asked_for("chats.start");
    assert_eq!(started[0]["agent"], "claude-code");
    assert_eq!(started[0]["message"], "Fix the flaky test\nmore detail");
    assert_eq!(started[0]["model"], "opus");
    assert_eq!(started[0]["effort"], "high");
    // The fake agent answers with its own model name, which wins.
    assert_eq!(session["model"], "test-model");
    assert_eq!(session["effort"], "high");

    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    daemon.send(0, &id, None, json!({ "type": "itemDelta", "turnId": "t1", "itemId": "i1", "delta": { "type": "text", "text": "Look" } }));
    daemon.send(2, &id, Some(2), json!({ "type": "itemEnd", "turnId": "t1", "item": { "type": "agentMessage", "id": "i1", "text": "Looking." } }));
    daemon.send(3, &id, Some(3), json!({ "type": "request", "request": {
        "id": "r1", "turnId": "t1", "native": {},
        "kind": { "type": "tool", "name": "Write", "displayName": null, "description": null,
                  "input": { "file_path": "a.rs" }, "toolCallId": null, "canAllowForSession": true } } }));
    // Sent again after a reconnect: stored once.
    daemon.send(4, &id, Some(2), json!({ "type": "itemEnd", "turnId": "t1", "item": { "type": "agentMessage", "id": "i1", "text": "Looking." } }));

    let waiting = setup
        .until(&id, "the request is open", |s| {
            s["openRequests"].as_array().is_some_and(|r| r.len() == 1)
        })
        .await;
    assert_eq!(waiting["status"], "working");
    let events = setup.get(&format!("/chats/{id}/events")).await;
    let seqs: Vec<i64> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["seq"].as_i64().unwrap())
        .collect();
    assert_eq!(seqs, [1, 2, 3]);
    assert_eq!(
        setup.get(&format!("/chats/{id}/events?after=2")).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Every kept event is acknowledged, live-only ones are not. An ack
    // covers everything up to its number, so acks that arrive together can
    // come as one (3 and 4 as just 4): they only ever go up.
    for _ in 0..50 {
        if daemon.acks.lock().unwrap().last() == Some(&4) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let acks = daemon.acks.lock().unwrap().clone();
    assert_eq!(acks.last(), Some(&4), "{acks:?}");
    assert!(acks.windows(2).all(|pair| pair[0] < pair[1]), "{acks:?}");
    assert!(!acks.contains(&0), "{acks:?}");

    let counts = setup.get("/sessions/waiting").await;
    assert_eq!(
        counts["projects"][&waiting["projectId"].as_str().unwrap()],
        1
    );

    let (status, refused) = setup
        .post(
            &format!("/chats/{id}/requests/nope"),
            json!({ "decision": "allow" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "request_not_open");
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/requests/r1"),
            json!({ "decision": "allowForSession" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let answered = daemon.asked_for("chats.respond");
    assert_eq!(answered[0]["request"], "r1");
    assert_eq!(answered[0]["decision"], "allowForSession");

    daemon.send(5, &id, Some(4), json!({ "type": "requestResolved", "id": "r1", "outcome": { "type": "answered", "decision": { "type": "allowForSession" } } }));
    daemon.send(
        6,
        &id,
        Some(5),
        json!({ "type": "turnEnd", "turn": { "id": "t1", "status": "completed", "items": [] } }),
    );
    let idle = setup
        .until(&id, "the turn ends", |s| s["status"] == "idle")
        .await;
    assert_eq!(idle["openRequests"], json!([]));

    let (status, _) = setup
        .post(
            &format!("/chats/{id}/messages"),
            json!({ "text": "Thanks" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(daemon.asked_for("chats.send")[0]["text"], "Thanks");

    let (status, closed) = setup.post(&format!("/chats/{id}/close"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(closed["state"], "closed");
    assert_eq!(daemon.asked_for("chats.close").len(), 1);
    let list = setup.get("/projects/app/sessions").await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["state"], "ended");
    assert_eq!(list["items"][0]["chats"][0]["id"], id.as_str());

    // Writing to an ended chat continues it: the agent loads its own
    // session, and new events are numbered after the kept ones.
    let kept = setup.get(&format!("/chats/{id}/events?after=0")).await;
    let last = kept["items"].as_array().unwrap().last().unwrap()["seq"].clone();
    let (status, body) = setup
        .post(&format!("/chats/{id}/messages"), json!({ "text": "again" }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let resumed = &daemon.asked_for("chats.resume")[0];
    assert_eq!(resumed["nativeId"], "native-1");
    assert_eq!(resumed["message"], "again");
    assert_eq!(resumed["after"], last);
    let chat = setup.get(&format!("/chats/{id}")).await;
    assert_eq!(chat["state"], "live");
    assert!(chat["closeReason"].is_null());
}

#[tokio::test]
async fn the_stream_replays_kept_events_then_goes_live() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    setup.daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    setup
        .until(&id, "the turn starts", |s| s["status"] == "working")
        .await;

    let mut response = setup
        .http
        .get(setup.url(&format!("/chats/{id}/stream")))
        .send()
        .await
        .unwrap();
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let mut text = String::new();
    let mut sent = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !text.contains("\"Hel\"") {
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .expect("stream stalled");
        text.push_str(&String::from_utf8_lossy(&chunk.unwrap().unwrap()));
        if text.contains("turnStart") && !sent {
            sent = true;
            setup.daemon.send(0, &id, None, json!({ "type": "itemDelta", "turnId": "t1", "itemId": "i1", "delta": { "type": "text", "text": "Hel" } }));
        }
    }
    assert!(text.contains("\"seq\":1"));
    assert!(text.contains("\"seq\":null"));
}

#[tokio::test]
async fn idle_chat_streams_can_repeatedly_disconnect_and_resume_from_their_cursor() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let path = setup.url(&format!("/chats/{id}/stream?after=1"));
    for _ in 0..64 {
        let mut stream = setup.http.get(&path).send().await.unwrap();
        assert!(stream.chunk().await.unwrap().is_some());
        drop(stream);
    }
    for seq in [1_u64, 2] {
        setup.daemon.send(seq, &id, Some(i64::try_from(seq).unwrap()), json!({
            "type":"itemEnd", "item": {"id":seq.to_string(), "type":"agentMessage", "text":"kept"}
        }));
    }
    let mut stream = setup.http.get(path).send().await.unwrap();
    let mut data = String::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !data.contains("\"seq\":2") {
            data.push_str(&String::from_utf8_lossy(
                &stream.chunk().await.unwrap().unwrap(),
            ));
        }
    })
    .await
    .unwrap();
    assert!(!data.contains("\"seq\":1"), "{data}");
}

#[tokio::test]
async fn a_later_event_cannot_acknowledge_an_earlier_unfinished_event_from_another_route() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    // Keep the first event unfinished on the separate tools route.
    setup.daemon.emit(1, "tools.testUnfinished", json!({}));
    setup.daemon.send(
        2,
        &id,
        Some(1),
        json!({ "type":"turnStart", "turnId":"two" }),
    );
    setup
        .until(&id, "the later event is stored", |chat| {
            chat["status"] == "working"
        })
        .await;
    assert!(setup.daemon.acks.lock().unwrap().is_empty());

    // On replay the first event can be completed and the duplicate is harmless.
    setup.daemon.disconnect();
    let daemon = FakeDaemon::connect(&setup.server, &setup.key).await;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type":"turnStart", "turnId":"two" }),
    );
    daemon.send(
        2,
        &id,
        Some(1),
        json!({ "type":"turnStart", "turnId":"two" }),
    );
    for _ in 0..100 {
        if daemon.acks.lock().unwrap().contains(&2) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the replay was not acknowledged");
}

#[tokio::test]
async fn history_pages_join_without_losing_events_at_turn_boundaries() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    for turn in 0..40_u64 {
        let first = turn * 3 + 1;
        let turn_id = format!("turn-{turn}");
        for (number, event) in [
            (first, json!({ "type": "turnStart", "turnId": turn_id })),
            (
                first + 1,
                json!({ "type": "itemEnd", "item": { "type": "agentMessage", "id": turn_id, "text": "answer" } }),
            ),
            (
                first + 2,
                json!({ "type": "turnEnd", "turn": { "id": turn_id, "status": "completed" } }),
            ),
        ] {
            setup
                .daemon
                .send(number, &id, Some(i64::try_from(number).unwrap()), event);
        }
    }
    tokio::time::timeout(Duration::from_secs(15), async {
        while !setup.daemon.acks.lock().unwrap().contains(&120) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the daemon's last event was stored");
    let latest = setup.get(&format!("/chats/{id}/events?before=0")).await;
    assert_eq!(
        latest["items"].as_array().unwrap().last().unwrap()["seq"],
        120
    );
    assert_eq!(latest["more"], true, "{latest}");
    let first = latest["historyBefore"].as_u64().unwrap();
    assert_eq!(first % 3, 1);
    let earlier = setup
        .get(&format!("/chats/{id}/events?before={first}"))
        .await;
    assert_eq!(earlier["historyBefore"], 0);
    let numbers: Vec<_> = earlier["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(latest["items"].as_array().unwrap())
        .map(|event| event["seq"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, (1..=120).collect::<Vec<_>>());
    let legacy = setup.get(&format!("/chats/{id}/events?after=119")).await;
    assert!(legacy.get("historyBefore").is_none());
    assert_eq!(legacy["items"][0]["seq"], 120);
}

#[tokio::test]
async fn opening_a_chat_with_one_long_turn_only_reads_the_latest_window() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    for seq in 1..=110 {
        let event = if seq == 1 {
            json!({"type": "turnStart", "turnId": "long"})
        } else {
            json!({"type": "itemEnd", "item": {"id": seq.to_string(), "type": "agentMessage", "text": "answer"}})
        };
        setup
            .daemon
            .send(seq, &id, Some(i64::try_from(seq).unwrap()), event);
    }
    tokio::time::timeout(Duration::from_secs(15), async {
        while !setup.daemon.acks.lock().unwrap().contains(&110) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let latest = setup
        .get(&format!("/chats/{id}/events?before=0&bounded=true"))
        .await;
    let items = latest["items"].as_array().unwrap();
    assert_eq!(items.len(), 96);
    assert_eq!(items.last().unwrap()["seq"], 110);
    assert_eq!(latest["historyBefore"], 15);
    let older = setup
        .get(&format!("/chats/{id}/events?before=15&bounded=true"))
        .await;
    assert_eq!(older["historyBefore"], 0);
    assert_eq!(
        older["items"].as_array().unwrap().last().unwrap()["seq"],
        14
    );
}

#[tokio::test]
async fn reconnecting_chat_stream_reports_an_empty_queue() {
    async fn snapshot(setup: &Setup, id: &str) -> Value {
        let mut response = setup
            .http
            .get(setup.url(&format!("/chats/{id}/stream")))
            .send()
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut text = String::new();
        loop {
            if let Some(queue) = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .find_map(|line| {
                    let event: Value = serde_json::from_str(line).ok()?;
                    (event["event"]["type"] == "queue").then(|| event["event"]["items"].clone())
                })
            {
                return queue;
            }
            let chunk = tokio::time::timeout_at(deadline, response.chunk())
                .await
                .expect("stream stalled")
                .unwrap()
                .unwrap();
            text.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
    let setup = Setup::new().await;
    let id = setup.start().await;
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/messages"),
            json!({ "text": "Waiting", "delivery": "queue" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let waiting = snapshot(&setup, &id).await;
    let item = waiting[0]["id"].as_str().unwrap();
    let removed = setup
        .http
        .delete(setup.url(&format!("/chats/{id}/queue/{item}")))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    assert_eq!(snapshot(&setup, &id).await, json!([]));
}

#[tokio::test]
async fn a_shared_event_stream_announces_session_activity() {
    let setup = Setup::new().await;
    let mut response = setup
        .http
        .get(setup.url("/plugin-events?metadata=true&names=sessions.changed"))
        .send()
        .await
        .unwrap();
    let chat = setup.start().await;
    let chat = setup.get(&format!("/chats/{chat}")).await;
    let mut text = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !text.contains("\"name\":\"sessions.changed\"") {
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .expect("stream stalled")
            .unwrap()
            .unwrap();
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    let mut frames = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok());
    assert_eq!(
        frames.next(),
        Some(json!({ "name": "stream.ready", "data": { "sessions": true } }))
    );
    let changed = frames
        .find(|event| event["name"] == "sessions.changed")
        .unwrap();
    assert_eq!(changed["data"]["session"], chat["sessionId"]);
    assert_eq!(changed["data"]["project"], chat["projectId"]);
}

#[tokio::test]
async fn an_event_applied_from_an_older_copy_keeps_what_the_start_recorded() {
    use ahab::modules::sessions::chat::store::ChatRepository;

    let setup = Setup::new().await;
    let id = setup.start().await;
    let chats = setup.server.chats();
    let uuid = id.parse().unwrap();
    // An event's copy, read before the daemon's answer was recorded.
    let mut stale = chats.get(uuid).await.unwrap().unwrap();
    stale.native_id = None;
    stale.model = None;
    let mut started = stale.clone();
    started.native_id = Some("native-2".to_owned());
    started.model = Some("other-model".to_owned());
    chats.record_started(&started).await.unwrap();

    let now = time::OffsetDateTime::now_utc();
    assert!(stale.apply(&json!({ "type": "turnStart", "turnId": "t1" }), now));
    chats.update_activity(&stale).await.unwrap();
    assert!(stale.apply(&json!({ "type": "closed", "reason": null }), now));
    chats.update_activity(&stale).await.unwrap();

    let chat = setup.get(&format!("/chats/{id}")).await;
    assert_eq!(chat["state"], "closed");
    assert_eq!(chat["model"], "other-model");
    // So it can still be continued.
    assert_eq!(chat["resumable"], true);
}

#[tokio::test]
async fn sessions_end_when_their_machine_restarted() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    setup.daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    setup
        .until(&id, "the turn starts", |s| s["status"] == "working")
        .await;

    // The machine comes back without the session.
    let Setup {
        server,
        daemon,
        key,
        ..
    } = setup;
    daemon.disconnect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _again = FakeDaemon::connect(&server, &key).await;
    let http = server.http();
    for _ in 0..100 {
        let session: Value = http
            .get(format!("{}/api/v1/chats/{id}", server.url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if session["state"] == "closed" {
            assert!(
                session["closeReason"]
                    .as_str()
                    .unwrap()
                    .contains("restarted")
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("the session stayed live");
}

impl Setup {
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self.http.request(method, self.url(path));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn sessions_are_renamed_pinned_filed_archived_and_deleted() {
    use reqwest::Method;
    let setup = Setup::new().await;
    let chat = setup.start().await;
    let chat = setup
        .until(&chat, "it runs", |c| c["state"] == "live")
        .await;
    let sessions = setup.get("/projects/app/sessions").await;
    let session = sessions["items"][0]["id"].as_str().unwrap().to_owned();
    let path = format!("/sessions/{session}");

    // Renaming and pinning.
    let (status, body) = setup
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "title": "Flaky test", "pinned": true })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["title"], "Flaky test");
    assert!(body["pinnedAt"].is_string());
    assert!(body["folderId"].is_null());

    // Folders belong to the project; a session is filed in one.
    let (status, folder) = setup
        .post(
            "/projects/app/session-folders",
            json!({ "name": " Tests " }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{folder}");
    assert_eq!(folder["name"], "Tests");
    let folder_id = folder["id"].as_str().unwrap();
    let (_, body) = setup
        .send(Method::PATCH, &path, Some(json!({ "folderId": folder_id })))
        .await;
    assert_eq!(body["folderId"], folder_id);
    assert_eq!(body["title"], "Flaky test", "absent fields stay");
    let (status, _) = setup
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "folderId": uuid::Uuid::now_v7() })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A session can start in a folder; not in another project's or none.
    let start = |folder: Value| json!({ "chat": { "agent": "claude-code", "message": "In a folder" }, "folderId": folder });
    let (status, filed) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            start(json!(folder_id)),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{filed}");
    assert_eq!(filed["folderId"], folder_id);
    let (status, _) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            start(json!(uuid::Uuid::now_v7())),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let listed = setup.get("/projects/app/sessions").await;
    assert_eq!(
        listed["items"].as_array().unwrap().len(),
        2,
        "the refused one isn't kept"
    );
    let (status, _) = setup
        .post("/projects/app/session-folders", json!({ "name": "  " }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let folders = setup.get("/projects/app/session-folders").await;
    assert_eq!(folders["items"].as_array().unwrap().len(), 1);

    // Deleting the folder keeps the session, out of any folder.
    let (status, _) = setup
        .send(
            Method::DELETE,
            &format!("/session-folders/{folder_id}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(setup.get(&path).await["folderId"].is_null());

    // Only archived sessions are deleted; archiving ends their chats.
    let (status, body) = setup.send(Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "session_not_archived");
    let (status, body) = setup.post(&format!("{path}/archive"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["archivedAt"].is_string());
    assert_eq!(body["state"], "ended");
    let (status, body) = setup
        .post(
            &format!("/chats/{}/messages", chat["id"].as_str().unwrap()),
            json!({ "text": "more" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["code"], "session_archived",
        "an archived session is not continued"
    );

    // Unarchiving brings it back; archived again, it can be deleted.
    let (_, body) = setup.post(&format!("{path}/unarchive"), json!({})).await;
    assert!(body["archivedAt"].is_null());
    setup.post(&format!("{path}/archive"), json!({})).await;
    let (status, _) = setup.send(Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = setup.send(Method::GET, &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = setup
        .send(
            Method::GET,
            &format!("/chats/{}", chat["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn attachments_are_kept_on_the_machine_and_sent_with_messages() {
    let setup = Setup::new().await;
    let response = setup
        .http
        .post(setup.url(&format!(
            "/projects/{}/attachments?name=../shot.png",
            setup.project
        )))
        .body(b"png".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let attachment: Value = response.json().await.unwrap();
    let put = setup.daemon.asked_for("attachments.put");
    assert_eq!(put[0]["name"], "shot.png", "the name is made safe first");
    assert_eq!(put[0]["data"], "cG5n");

    // Read back, as an image.
    let id = attachment["id"].as_str().unwrap();
    let file = setup
        .http
        .get(setup.url(&format!(
            "/projects/{}/attachments/{id}/shot.png",
            setup.project
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(file.status(), StatusCode::OK);
    assert_eq!(file.headers()["content-type"], "image/png");
    assert_eq!(file.bytes().await.unwrap().as_ref(), b"png");

    // Empty files are refused; attachments go with the first message.
    let empty = setup
        .http
        .post(setup.url(&format!(
            "/projects/{}/attachments?name=x.txt",
            setup.project
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let (status, session) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "claude-code", "message": "", "attachments": [id] } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["title"], "Attached files");
    let started = setup.daemon.asked_for("chats.start");
    assert_eq!(started[0]["attachments"], json!([id]));

    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    setup
        .until(&chat, "it runs", |c| c["state"] == "live")
        .await;
    let (status, _) = setup
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "attachments": [id] }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        setup.daemon.asked_for("chats.send")[0]["attachments"],
        json!([id])
    );
    let (status, _) = setup
        .post(&format!("/chats/{chat}/messages"), json!({ "text": " " }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn chats_run_in_a_permission_mode_that_can_change() {
    use reqwest::Method;
    let setup = Setup::new().await;
    // Each agent has its own modes, and a default.
    let (status, body) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "claude-code", "message": "go", "mode": "fullAccess" } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, session) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "claude-code", "message": "go", "mode": "plan" } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["chats"][0]["mode"], "plan");
    assert_eq!(setup.daemon.asked_for("chats.start")[0]["mode"], "plan");
    let id = session["chats"][0]["id"].as_str().unwrap().to_owned();
    setup.until(&id, "it runs", |c| c["state"] == "live").await;

    // A live chat changes at once, through its machine.
    let (status, chat) = setup
        .send(
            Method::PUT,
            &format!("/chats/{id}/mode"),
            Some(json!({ "mode": "acceptEdits" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{chat}");
    assert_eq!(chat["mode"], "acceptEdits");
    let asked = setup.daemon.asked_for("chats.mode");
    assert_eq!(asked[0]["mode"], "acceptEdits");
    assert_eq!(asked[0]["project"], setup.project.as_str());

    // An ended chat keeps it, and resumes in it.
    setup.post(&format!("/chats/{id}/close"), json!({})).await;
    let (status, chat) = setup
        .send(
            Method::PUT,
            &format!("/chats/{id}/mode"),
            Some(json!({ "mode": "bypass" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(chat["mode"], "bypass");
    assert_eq!(setup.daemon.asked_for("chats.mode").len(), 1);
    setup
        .post(&format!("/chats/{id}/messages"), json!({ "text": "again" }))
        .await;
    assert_eq!(setup.daemon.asked_for("chats.resume")[0]["mode"], "bypass");
    let (status, _) = setup
        .send(
            Method::PUT,
            &format!("/chats/{id}/mode"),
            Some(json!({ "mode": "nope" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn an_added_agent_resumes_in_its_own_mode_when_the_saved_one_is_gone() {
    use reqwest::Method;
    let setup = Setup::with_capabilities(Some(ahab::daemon::dispatch::METHODS)).await;
    let machine = setup.get("/machines").await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, added) = setup
        .post(
            &format!("/machines/{machine}/agents"),
            json!({ "agent": "acp:gemini" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{added}");
    setup
        .post(&format!("/machines/{machine}/agents/refresh"), json!({}))
        .await;
    let (status, session) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "acp:gemini", "message": "go" } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["chats"][0]["id"].as_str().unwrap().to_owned();
    setup.until(&id, "it runs", |c| c["state"] == "live").await;
    let (status, chat) = setup
        .send(
            Method::PUT,
            &format!("/chats/{id}/mode"),
            Some(json!({ "mode": "retired" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{chat}");
    setup.post(&format!("/chats/{id}/close"), json!({})).await;

    // The agent no longer offers it: the chat continues, in the agent's mode.
    let (status, body) = setup
        .post(&format!("/chats/{id}/messages"), json!({ "text": "again" }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(setup.daemon.asked_for("chats.resume")[0]["mode"], "retired");
    let chat = setup.get(&format!("/chats/{id}")).await;
    assert_eq!(chat["state"], "live", "{chat}");
    assert_eq!(chat["mode"], "ask");

    // The daemon's note of it is kept with the chat's events.
    let notice = json!({ "type": "modeUnavailable", "mode": "retired", "current": "ask" });
    setup.daemon.send(1, &id, Some(1), notice.clone());
    let mut kept = Value::Null;
    for _ in 0..100 {
        kept = setup.get(&format!("/chats/{id}/events")).await;
        if !kept["items"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(kept["items"][0]["event"], notice, "{kept}");
}

#[tokio::test]
async fn each_agent_keeps_its_last_choices() {
    use reqwest::Method;
    let setup = Setup::new().await;
    assert_eq!(setup.get("/agent-defaults").await["items"], json!([]));
    let (status, saved) = setup
        .send(
            Method::PUT,
            "/agent-defaults/claude-code",
            Some(json!({ "model": "opus", "effort": "medium", "mode": "bypass" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    // Saving again replaces them; empty means the agent's own.
    setup
        .send(
            Method::PUT,
            "/agent-defaults/claude-code",
            Some(json!({ "model": "opus", "effort": "", "mode": "bypass" })),
        )
        .await;
    let list = setup.get("/agent-defaults").await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["model"], "opus");
    assert!(list["items"][0]["effort"].is_null());
    assert_eq!(list["items"][0]["mode"], "bypass");

    let (status, _) = setup
        .send(
            Method::PUT,
            "/agent-defaults/codex",
            Some(json!({ "mode": "bypass" })),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Codex has no bypass"
    );
    let (status, _) = setup
        .send(Method::PUT, "/agent-defaults/gpt", Some(json!({})))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_session_whose_agent_cannot_start_is_not_kept() {
    let setup = Setup::new().await;
    let (status, body) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "claude-code", "message": "go", "mode": "nope" } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        setup.get("/projects/app/sessions").await["items"],
        json!([])
    );
    let (status, _) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "title": "x" }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a session starts with its conversation"
    );
}

#[tokio::test]
async fn context_references_reach_the_agent_after_the_message() {
    let setup = Setup::new().await;
    let lines = json!({
        "path": "src/join.rs", "startLine": 40, "endLine": 42,
        "text": "fn join() {\n    todo!()\n}\n", "version": "v-now",
    });
    // A session with only context takes its title from the first reference.
    let (status, session) = setup
        .post(
            &format!("/projects/{}/sessions", setup.project),
            json!({ "chat": { "agent": "claude-code", "message": "", "references": [lines] } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["title"], "src/join.rs, lines 40\u{2013}42");
    assert_eq!(
        setup.daemon.asked_for("chats.start")[0]["message"],
        "Context: src/join.rs, lines 40\u{2013}42\n```rust\nfn join() {\n    todo!()\n}\n```"
    );

    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    setup
        .until(&chat, "it runs", |c| c["state"] == "live")
        .await;
    // Lines whose file changed since they were selected say so; a whole
    // file is its path.
    let (status, _) = setup
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "text": "Why?", "references": [
                { "path": "src/join.rs", "startLine": 7, "endLine": 7, "text": "x", "version": "v-then" },
                { "kind": "file", "path": "README.md" },
                { "kind": "terminalOutput", "label": "cargo test", "text": "1 failed" },
            ] }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        setup.daemon.asked_for("chats.send")[0]["text"],
        "Why?\n\n\
         Context: src/join.rs, line 7 (changed on disk since it was selected)\n```rust\nx\n```\n\n\
         Context: README.md (the whole file)\n\n\
         Context: terminal output from cargo test\n```text\n1 failed\n```"
    );

    // A reference to a file needs its path; too many are refused.
    let (status, refused) = setup
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "references": [{ "kind": "lines", "text": "x" }] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let many: Vec<Value> = (0..21)
        .map(|_| json!({ "kind": "file", "path": "a" }))
        .collect();
    let (status, _) = setup
        .post(
            &format!("/chats/{chat}/messages"),
            json!({ "references": many }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Waits until the daemon was asked `method` `count` times.
async fn asked(daemon: &FakeDaemon, method: &str, count: usize) -> Vec<Value> {
    for _ in 0..100 {
        let asked = daemon.asked_for(method);
        if asked.len() >= count {
            return asked;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {count} {method}");
}

#[tokio::test]
async fn messages_sent_while_the_agent_works_wait_for_its_turn_to_end() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    // Started chats work at once: wait for the turn's start to be kept.
    for _ in 0..100 {
        let events = setup.get(&format!("/chats/{id}/events")).await;
        if !events["items"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Queued: nothing reaches the agent yet.
    for text in ["first", "second", "dropped"] {
        let (status, body) = setup
            .post(
                &format!("/chats/{id}/messages"),
                json!({ "text": text, "delivery": "queue" }),
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    }
    assert!(daemon.asked_for("chats.send").is_empty());
    assert!(daemon.asked_for("chats.interrupt").is_empty());

    // Taken back, by its ID from the stream's `queue` event.
    let mut response = setup
        .http
        .get(setup.url(&format!("/chats/{id}/stream")))
        .send()
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut text = String::new();
    let queue = loop {
        if let Some(line) = text
            .lines()
            .find(|line| line.contains("\"type\":\"queue\""))
        {
            let data: Value =
                serde_json::from_str(line.trim_start_matches("data:").trim()).unwrap();
            break data["event"]["items"].clone();
        }
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .expect("stream stalled");
        text.push_str(&String::from_utf8_lossy(&chunk.unwrap().unwrap()));
    };
    let texts: Vec<&str> = queue
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["first", "second", "dropped"]);
    let dropped = queue[2]["id"].as_str().unwrap();
    let response = setup
        .http
        .delete(setup.url(&format!("/chats/{id}/queue/{dropped}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let again = setup
        .http
        .delete(setup.url(&format!("/chats/{id}/queue/{dropped}")))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::NOT_FOUND);

    // Interrupting: the turn is stopped, and the message goes first.
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/messages"),
            json!({ "text": "stop, do this", "delivery": "interrupt" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(daemon.asked_for("chats.interrupt")[0]["turn"], "t1");
    assert!(daemon.asked_for("chats.send").is_empty());

    // Each turn's end sends the next one.
    daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "turnEnd", "turn": { "id": "t1", "status": "interrupted", "items": [] } }),
    );
    assert_eq!(
        asked(daemon, "chats.send", 1).await[0]["text"],
        "stop, do this"
    );
    daemon.send(
        3,
        &id,
        Some(3),
        json!({ "type": "turnStart", "turnId": "t2" }),
    );
    daemon.send(
        4,
        &id,
        Some(4),
        json!({ "type": "turnEnd", "turn": { "id": "t2", "status": "completed", "items": [] } }),
    );
    assert_eq!(asked(daemon, "chats.send", 2).await[1]["text"], "first");
    daemon.send(
        5,
        &id,
        Some(5),
        json!({ "type": "turnStart", "turnId": "t3" }),
    );
    daemon.send(
        6,
        &id,
        Some(6),
        json!({ "type": "turnEnd", "turn": { "id": "t3", "status": "completed", "items": [] } }),
    );
    assert_eq!(asked(daemon, "chats.send", 3).await[2]["text"], "second");

    // Idle: a message goes at once, whatever it asks.
    daemon.send(
        7,
        &id,
        Some(7),
        json!({ "type": "turnStart", "turnId": "t4" }),
    );
    daemon.send(
        8,
        &id,
        Some(8),
        json!({ "type": "turnEnd", "turn": { "id": "t4", "status": "completed", "items": [] } }),
    );
    setup
        .until(&id, "the turn ends", |s| s["status"] == "idle")
        .await;
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/messages"),
            json!({ "text": "now", "delivery": "queue" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(asked(daemon, "chats.send", 4).await[3]["text"], "now");
    assert_eq!(daemon.asked_for("chats.send").len(), 4);
}

#[tokio::test]
async fn a_queued_message_can_be_sent_now() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    // Started chats work at once: wait for the turn's start to be kept.
    for _ in 0..100 {
        let events = setup.get(&format!("/chats/{id}/events")).await;
        if !events["items"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    for text in ["later", "urgent"] {
        let (status, _) = setup
            .post(
                &format!("/chats/{id}/messages"),
                json!({ "text": text, "delivery": "queue" }),
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    let mut response = setup
        .http
        .get(setup.url(&format!("/chats/{id}/stream")))
        .send()
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut text = String::new();
    let urgent = loop {
        if let Some(line) = text
            .lines()
            .find(|line| line.contains("\"type\":\"queue\""))
        {
            let data: Value =
                serde_json::from_str(line.trim_start_matches("data:").trim()).unwrap();
            break data["event"]["items"][1]["id"].as_str().unwrap().to_owned();
        }
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .expect("stream stalled");
        text.push_str(&String::from_utf8_lossy(&chunk.unwrap().unwrap()));
    };

    let (status, _) = setup
        .post(&format!("/chats/{id}/queue/{urgent}/now"), json!({}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(daemon.asked_for("chats.interrupt")[0]["turn"], "t1");
    let (status, _) = setup
        .post(&format!("/chats/{id}/queue/{urgent}/now"), json!({}))
        .await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "still waiting: it can be asked again"
    );

    daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "turnEnd", "turn": { "id": "t1", "status": "interrupted", "items": [] } }),
    );
    assert_eq!(asked(daemon, "chats.send", 1).await[0]["text"], "urgent");
    let missing = uuid::Uuid::now_v7();
    let (status, _) = setup
        .post(&format!("/chats/{id}/queue/{missing}/now"), json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Questions, plans and forms take their own answers, checked against what
/// they ask before the agent hears them; what the agent refuses stays open.
#[tokio::test]
async fn questions_plans_and_forms_are_answered_with_their_decisions() {
    let setup = Setup::with_capabilities(Some(ahab::daemon::dispatch::METHODS)).await;
    let id = setup.start().await;
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "request", "request": {
        "id": "q1", "turnId": "t1", "native": {},
        "kind": { "type": "question", "toolCallId": "toolu_1", "questions": [{
            "id": "Which library?", "header": "Library", "question": "Which library?",
            "options": [{ "label": "chrono", "description": null, "preview": null },
                        { "label": "time", "description": null, "preview": null }],
            "multiSelect": false, "allowOther": true, "secret": false }] } } }),
    );
    daemon.send(3, &id, Some(3), json!({ "type": "request", "request": {
        "id": "p1", "turnId": "t1", "native": {},
        "kind": { "type": "planApproval", "plan": "# Plan", "path": null, "toolCallId": null,
            "options": [{ "id": "acceptEdits", "label": "Yes, and accept edits", "description": null, "kind": "allowOnce" }] } } }));
    daemon.send(4, &id, Some(4), json!({ "type": "request", "request": {
        "id": "refused", "turnId": "t1", "native": {},
        "kind": { "type": "permission", "name": "edit", "displayName": null, "description": null,
            "input": {}, "toolCallId": null,
            "options": [{ "id": "x", "label": "Allow", "description": null, "kind": "allowOnce" }] } } }));
    setup
        .until(&id, "three requests wait", |s| {
            s["openRequests"].as_array().is_some_and(|r| r.len() == 3)
        })
        .await;

    // Checked by the server: nothing reaches the agent.
    let (status, refused) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "allow" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "invalid_decision");
    let (status, refused) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "answer", "answers": [{ "questionId": "Which library?", "selected": ["moment"] }] }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "invalid_decision");
    assert!(daemon.asked_for("chats.decide").is_empty());

    let (status, body) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "answer", "answers": [
                { "questionId": "Which library?", "selected": ["time"], "other": "pinned" }] }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/requests/p1"),
            json!({ "decision": "select", "optionId": "acceptEdits" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let decided = daemon.asked_for("chats.decide");
    assert_eq!(
        decided[0]["decision"],
        json!({ "type": "answer", "answers": [
            { "questionId": "Which library?", "selected": ["time"], "other": "pinned" }] })
    );
    assert_eq!(decided[0]["request"], "q1");
    assert_eq!(
        decided[1]["decision"],
        json!({ "type": "select", "optionId": "acceptEdits" })
    );
    assert!(daemon.asked_for("chats.respond").is_empty());
    // The plan's option switched Claude Code's mode: the chat's follows.
    assert_eq!(
        setup.get(&format!("/chats/{id}")).await["mode"],
        "acceptEdits"
    );

    // Checked by the agent: its refusal is the same Problem.
    let (status, refused) = setup
        .post(
            &format!("/chats/{id}/requests/refused"),
            json!({ "decision": "select", "optionId": "x" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "invalid_decision");

    // Deny with a reason, and Deny and stop.
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/requests/p1"),
            json!({ "decision": "deny", "message": "Keep it smaller" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "cancel" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let decided = daemon.asked_for("chats.decide");
    assert_eq!(
        decided[3]["decision"],
        json!({ "type": "deny", "message": "Keep it smaller" })
    );
    assert_eq!(decided[4]["decision"], json!({ "type": "cancel" }));
}

/// A daemon from before `chats.decide` still answers permissions; what it
/// cannot answer says to update it.
#[tokio::test]
async fn an_old_daemon_answers_permissions_only() {
    let setup = Setup::new().await;
    let id = setup.start().await;
    let daemon = &setup.daemon;
    daemon.send(
        1,
        &id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    daemon.send(
        2,
        &id,
        Some(2),
        json!({ "type": "request", "request": {
        "id": "q1", "turnId": "t1", "native": {},
        "kind": { "type": "question", "toolCallId": null, "questions": [{
            "id": "q", "header": null, "question": "Name?", "options": [],
            "multiSelect": false, "allowOther": true, "secret": false }] } } }),
    );
    setup
        .until(&id, "the question waits", |s| {
            s["openRequests"].as_array().is_some_and(|r| r.len() == 1)
        })
        .await;
    let (status, refused) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "answer", "answers": [{ "questionId": "q", "other": "Ann" }] }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "machine_outdated");
    let (status, _) = setup
        .post(
            &format!("/chats/{id}/requests/q1"),
            json!({ "decision": "deny", "message": "Not now" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(daemon.asked_for("chats.respond")[0]["decision"], "deny");
}
