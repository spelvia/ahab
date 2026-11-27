//! Sessions with several chats and terminals: adding them, the
//! session's summary state, renaming, moving within the project, and
//! removing stopped ones. Only organization: each runs on its own.

use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

struct Setup {
    server: TestServer,
    daemon: FakeDaemon,
    project: String,
}

async fn setup() -> Setup {
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
    Setup {
        server,
        daemon,
        project,
    }
}

impl Setup {
    async fn session(&self, message: &str) -> Value {
        let (status, session) = self
            .server
            .post(
                &format!("/projects/{}/sessions", self.project),
                json!({ "chat": { "agent": "claude-code", "message": message } }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{session}");
        session
    }

    async fn get(&self, session: &str) -> Value {
        self.server.get(&format!("/sessions/{session}")).await.1
    }
}

fn id(value: &Value) -> String {
    value["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_session_holds_chats_and_terminals_added_any_time() {
    let s = setup().await;
    let session = s.session("Fix the flaky test").await;
    let sid = id(&session);

    let (status, second) = s
        .server
        .post(
            &format!("/sessions/{sid}/chats"),
            json!({ "agent": "claude-code", "message": "Second opinion on the fix" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_eq!(second["sessionId"], sid.as_str());
    assert_eq!(second["title"], "Second opinion on the fix");

    let (status, terminal) = s
        .server
        .post(
            &format!("/sessions/{sid}/terminals"),
            json!({ "command": "pnpm test --watch", "title": "tests" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{terminal}");
    assert_eq!(terminal["label"], "tests");
    assert_eq!(terminal["sessionId"], sid.as_str());
    // Started on the machine as any terminal is, in the session.
    let asked = s.daemon.asked_for("terminals.create");
    assert_eq!(asked.last().unwrap()["session"], sid.as_str());

    let found = s.get(&sid).await;
    assert_eq!(found["chats"].as_array().unwrap().len(), 2);
    assert_eq!(found["terminals"].as_array().unwrap().len(), 1);
    assert_eq!(found["terminal"]["id"], terminal["id"]);
    // The session list shows the same.
    let (_, list) = s.server.get("/projects/app/sessions").await;
    let listed = &list["items"][0];
    assert_eq!(listed["chats"].as_array().unwrap().len(), 2);
    assert_eq!(listed["terminals"].as_array().unwrap().len(), 1);

    // Still one session: nothing new was listed for the added ones.
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn the_state_sums_up_chats_and_terminals() {
    let s = setup().await;
    let session = s.session("Fix it").await;
    let sid = id(&session);
    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    let (_, terminal) = s
        .server
        .post(&format!("/sessions/{sid}/terminals"), json!({}))
        .await;
    let tid = id(&terminal);

    s.daemon.send(
        1,
        &chat,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    eventually(&s, &sid, "working").await;

    // The chat ends; the terminal still runs: the session waits for you.
    s.daemon.send(
        2,
        &chat,
        Some(2),
        json!({ "type": "closed", "reason": { "type": "exited" } }),
    );
    eventually(&s, &sid, "idle").await;

    // The terminal exits too: nothing runs.
    s.daemon.emit(
        3,
        "terminal.event",
        json!({ "type": "exited", "terminal": tid, "code": 0 }),
    );
    eventually(&s, &sid, "ended").await;
}

async fn eventually(s: &Setup, session: &str, state: &str) {
    for _ in 0..200 {
        if s.get(session).await["state"] == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{session} never became {state}: {}", s.get(session).await);
}

#[tokio::test]
async fn chats_and_terminals_are_renamed_moved_and_removed_once_stopped() {
    let s = setup().await;
    let first = s.session("Fix it").await;
    let second = s.session("Something else").await;
    let (a, b) = (id(&first), id(&second));
    let chat = first["chats"][0]["id"].as_str().unwrap().to_owned();
    let (_, terminal) = s
        .server
        .post(&format!("/sessions/{a}/terminals"), json!({}))
        .await;
    let tid = id(&terminal);

    // Renamed.
    let (status, renamed) = s
        .server
        .call(
            Method::PATCH,
            &format!("/chats/{chat}"),
            Some(json!({ "title": "Claude, main fix" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["title"], "Claude, main fix");
    let (_, named) = s
        .server
        .call(
            Method::PATCH,
            &format!("/terminals/{tid}"),
            Some(json!({ "label": "dev server" })),
        )
        .await;
    assert_eq!(named["label"], "dev server");
    let (_, unnamed) = s
        .server
        .call(
            Method::PATCH,
            &format!("/terminals/{tid}"),
            Some(json!({ "label": null })),
        )
        .await;
    assert!(unnamed["label"].is_null());

    // Moved to the other session: it keeps running, listed there now.
    let (status, _) = s
        .server
        .call(
            Method::PATCH,
            &format!("/terminals/{tid}"),
            Some(json!({ "sessionId": b })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = s
        .server
        .call(
            Method::PATCH,
            &format!("/chats/{chat}"),
            Some(json!({ "sessionId": b })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let moved = s.get(&b).await;
    assert_eq!(moved["chats"].as_array().unwrap().len(), 2);
    assert_eq!(moved["terminals"][0]["id"], tid.as_str());
    assert_eq!(moved["terminals"][0]["state"], "running");
    // The first is left empty, and stays until you delete it.
    let empty = s.get(&a).await;
    assert!(empty["chats"].as_array().unwrap().is_empty());
    assert_eq!(empty["state"], "ended");
    assert!(s.daemon.asked_for("terminals.kill").is_empty());

    // A running terminal or a live chat is stopped before it is removed.
    let (status, refused) = s
        .server
        .call(Method::DELETE, &format!("/terminals/{tid}"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "terminal_running");
    // …unless asked to stop it too: a second terminal goes that way.
    let (_, other) = s
        .server
        .post(&format!("/sessions/{b}/terminals"), json!({}))
        .await;
    let (status, _) = s
        .server
        .call(
            Method::DELETE,
            &format!("/terminals/{}?stop=true", id(&other)),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        s.daemon.asked_for("terminals.remove").last().unwrap()["terminal"],
        id(&other).as_str()
    );
    let (status, refused) = s
        .server
        .call(Method::DELETE, &format!("/chats/{chat}"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "chat_live");

    s.server
        .post(&format!("/chats/{chat}/close"), json!({}))
        .await;
    let (status, _) = s
        .server
        .call(Method::DELETE, &format!("/chats/{chat}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    s.daemon.emit(
        9,
        "terminal.event",
        json!({ "type": "exited", "terminal": tid, "code": 0 }),
    );
    for _ in 0..200 {
        if s.server.get(&format!("/terminals/{tid}")).await.1["state"] == "exited" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, _) = s
        .server
        .call(Method::DELETE, &format!("/terminals/{tid}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let left = s.get(&b).await;
    assert_eq!(left["chats"].as_array().unwrap().len(), 1);
    assert!(left["terminals"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn moving_stays_in_the_project_and_archived_sessions_take_nothing_new() {
    let s = setup().await;
    let session = s.session("Fix it").await;
    let sid = id(&session);
    let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
    let (_, other) = s
        .server
        .post(
            "/projects",
            json!({ "name": "other", "machine": "atlas", "path": "/work/other" }),
        )
        .await;
    let (_, there) = s
        .server
        .post(
            &format!("/projects/{}/sessions", id(&other)),
            json!({ "chat": { "agent": "claude-code", "message": "elsewhere" } }),
        )
        .await;
    let (status, refused) = s
        .server
        .call(
            Method::PATCH,
            &format!("/chats/{chat}"),
            Some(json!({ "sessionId": id(&there) })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["code"], "chat_in_other_project");

    s.server
        .post(&format!("/sessions/{sid}/archive"), json!({}))
        .await;
    let (status, refused) = s
        .server
        .post(&format!("/sessions/{sid}/terminals"), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["code"], "session_archived");
    let (status, _) = s
        .server
        .post(
            &format!("/sessions/{sid}/chats"),
            json!({ "agent": "codex", "message": "more" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn archiving_stops_every_terminal_and_deleting_forgets_them() {
    let s = setup().await;
    let session = s.session("Fix it").await;
    let sid = id(&session);
    for _ in 0..2 {
        s.server
            .post(&format!("/sessions/{sid}/terminals"), json!({}))
            .await;
    }
    s.server
        .post(&format!("/sessions/{sid}/archive"), json!({}))
        .await;
    assert_eq!(s.daemon.asked_for("terminals.kill").len(), 2);
    let (status, _) = s
        .server
        .call(Method::DELETE, &format!("/sessions/{sid}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(s.daemon.asked_for("terminals.remove").len(), 2);
}
