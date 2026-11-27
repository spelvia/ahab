//! Notifications through the API: agents that finish, need their person or
//! fail; the inbox; the settings; deleting old read ones.

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;
use ahab::modules::notifications::notification::model::{Level, NewNotification};

use crate::support::{FakeDaemon, TestApp, TestServer};

struct Agent {
    server: TestServer,
    daemon: FakeDaemon,
    chat: String,
}

impl Agent {
    /// A machine, a project and a running chat on it.
    async fn start() -> Self {
        let server = TestServer::start().await;
        let key = MachineKey::from_secret([7; 32]);
        let machine = server.local_machine(&key, "atlas").await;
        // A daemon as new as the server: it is not told to be behind.
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
        let (status, session) = server
            .post(
                &format!("/projects/{project}/sessions"),
                json!({ "chat": { "agent": "claude-code", "message": "Fix the flaky test" } }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{session}");
        let chat = session["chats"][0]["id"].as_str().unwrap().to_owned();
        Self {
            server,
            daemon,
            chat,
        }
    }

    /// The inbox once `check` holds.
    async fn inbox_until(&self, what: &str, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..300 {
            let inbox = self.server.get("/notifications").await.1;
            if check(&inbox) {
                return inbox;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("timed out waiting until {what}");
    }
}

fn kinds(inbox: &Value) -> Vec<String> {
    inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn an_agent_that_asks_then_stops_is_told_once_for_its_chat() {
    let agent = Agent::start().await;
    let id = &agent.chat;
    agent.daemon.send(
        1,
        id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    agent.daemon.send(
        2,
        id,
        Some(2),
        json!({ "type": "request", "request": {
        "id": "r1", "turnId": "t1", "title": "Write a.rs?", "native": {},
        "kind": { "type": "tool", "name": "Write" } } }),
    );
    let inbox = agent
        .inbox_until("it needs its person", |inbox| {
            inbox["unread"] == 1 && inbox["items"][0]["kind"] == "sessions.needs-you"
        })
        .await;
    let asked = &inbox["items"][0];
    assert_eq!(asked["kind"], "sessions.needs-you");
    assert_eq!(asked["level"], "warning");
    assert_eq!(asked["title"], "Claude Code needs you");
    assert_eq!(asked["body"], "Write a.rs?");
    assert_eq!(asked["target"]["type"], "session");
    assert_eq!(asked["target"]["chatId"], id.as_str());

    // It stops unexpectedly: the newest state stands for the chat.
    agent.daemon.send(
        3,
        id,
        Some(3),
        json!({ "type": "closed", "reason": { "type": "exited", "message": "killed" } }),
    );
    let inbox = agent
        .inbox_until("it failed", |inbox| kinds(inbox) == ["sessions.failed"])
        .await;
    assert_eq!(inbox["unread"], 1);
    assert_eq!(inbox["items"][0]["level"], "error");
    assert!(
        inbox["items"][0]["body"]
            .as_str()
            .unwrap()
            .contains("killed")
    );
}

#[tokio::test]
async fn an_agent_is_finished_only_once_it_stays_idle() {
    let agent = Agent::start().await;
    let id = &agent.chat;
    agent.daemon.send(
        1,
        id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    agent
        .daemon
        .send(2, id, Some(2), json!({ "type": "turnEnd", "turnId": "t1" }));
    // A follow-up turn at once: not finished yet.
    agent.daemon.send(
        3,
        id,
        Some(3),
        json!({ "type": "turnStart", "turnId": "t2" }),
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(agent.server.get("/notifications").await.1["unread"], 0);

    agent
        .daemon
        .send(4, id, Some(4), json!({ "type": "turnEnd", "turnId": "t2" }));
    let inbox = agent
        .inbox_until("it finished", |inbox| inbox["unread"] == 1)
        .await;
    assert_eq!(kinds(&inbox), ["sessions.finished"]);
    assert_eq!(inbox["items"][0]["title"], "Claude Code finished");
    assert_eq!(inbox["items"][0]["body"], "Fix the flaky test");
}

#[tokio::test]
async fn a_kind_turned_off_is_not_sent() {
    let agent = Agent::start().await;
    let (status, settings) = agent
        .server
        .put(
            "/notification-settings",
            json!({ "enabled": true, "keepDays": 30, "kinds": { "sessions.needs-you": false } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{settings}");
    let id = &agent.chat;
    agent.daemon.send(
        1,
        id,
        Some(1),
        json!({ "type": "turnStart", "turnId": "t1" }),
    );
    agent.daemon.send(
        2,
        id,
        Some(2),
        json!({ "type": "request", "request": { "id": "r1", "turnId": "t1" } }),
    );
    agent.daemon.send(
        3,
        id,
        Some(3),
        json!({ "type": "closed", "reason": { "type": "exited", "message": "x" } }),
    );
    // The failure still comes, the question did not.
    let inbox = agent
        .inbox_until("it failed", |inbox| inbox["unread"] == 1)
        .await;
    assert_eq!(kinds(&inbox), ["sessions.failed"]);
}

fn note(group: Option<&str>) -> NewNotification {
    NewNotification {
        kind: "sessions.finished".into(),
        source: "ahab".into(),
        level: Level::Success,
        title: "Claude Code finished".into(),
        body: None,
        project_id: None,
        target: json!({ "type": "session" }),
        group_key: group.map(str::to_owned),
    }
}

#[tokio::test]
async fn the_settings_are_on_by_default_and_checked() {
    let app = TestApp::start().await;
    let settings = app.get("/api/v1/notification-settings").await;
    assert_eq!(settings.body["enabled"], true);
    assert_eq!(settings.body["keepDays"], 30);
    assert_eq!(settings.body["outside"], true);
    assert_eq!(settings.body["kinds"].as_array().unwrap().len(), 6);
    assert!(
        settings.body["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|kind| kind["enabled"] == true)
    );

    let wrong = app
        .put(
            "/api/v1/notification-settings",
            json!({ "enabled": true, "keepDays": 0 }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    let forever = app
        .put(
            "/api/v1/notification-settings",
            json!({ "enabled": true, "keepDays": null }),
        )
        .await;
    assert_eq!(forever.status, StatusCode::OK);
    assert_eq!(forever.body["keepDays"], Value::Null);

    // Off: nothing is sent at all.
    app.put("/api/v1/notification-settings", json!({ "enabled": false }))
        .await;
    assert!(
        app.state
            .notifications
            .send(note(None))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn the_inbox_is_read_marked_and_cleared() {
    let app = TestApp::start().await;
    let first = app
        .state
        .notifications
        .send(note(Some("a")))
        .await
        .unwrap()
        .unwrap();
    app.state.notifications.send(note(Some("a"))).await.unwrap();
    app.state.notifications.send(note(None)).await.unwrap();
    let inbox = app.get("/api/v1/notifications").await;
    // The group's newest replaced its older unread one.
    assert_eq!(inbox.body["unread"], 2);
    assert_eq!(inbox.body["items"].as_array().unwrap().len(), 2);
    assert!(
        app.get(&format!("/api/v1/notifications/{}", first.id))
            .await
            .status
            != StatusCode::OK
    );

    let newest = inbox.body["items"][0]["id"].as_str().unwrap().to_owned();
    let read = app
        .post(&format!("/api/v1/notifications/{newest}/read"), json!({}))
        .await;
    assert_eq!(read.status, StatusCode::OK);
    assert!(read.body["readAt"].is_string());
    let unread = app.get("/api/v1/notifications?unread=true").await;
    assert_eq!(unread.body["items"].as_array().unwrap().len(), 1);

    let all = app.post("/api/v1/notifications/read-all", json!({})).await;
    assert_eq!(all.body["count"], 1);
    let cleared = app.delete("/api/v1/notifications").await;
    assert_eq!(cleared.body["count"], 2);
    assert_eq!(
        app.get("/api/v1/notifications").await.body["items"],
        json!([])
    );
}

#[tokio::test]
async fn read_notifications_go_after_the_kept_days_unless_kept() {
    let app = TestApp::start().await;
    let old = app
        .state
        .notifications
        .send(note(None))
        .await
        .unwrap()
        .unwrap();
    app.state.notifications.read(old.id).await.unwrap();
    app.state.notifications.send(note(None)).await.unwrap();

    app.clock.advance(time::Duration::days(31));
    // Kept forever: nothing goes.
    app.put(
        "/api/v1/notification-settings",
        json!({ "enabled": true, "keepDays": null }),
    )
    .await;
    assert_eq!(app.state.notifications.clean().await.unwrap(), 0);
    // 30 days: the read one goes, the unread one stays.
    app.put(
        "/api/v1/notification-settings",
        json!({ "enabled": true, "keepDays": 30 }),
    )
    .await;
    assert_eq!(app.state.notifications.clean().await.unwrap(), 1);
    assert_eq!(
        app.get("/api/v1/notifications").await.body["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn the_activity_counts_the_agents_working() {
    let agent = Agent::start().await;
    let (_, activity) = agent.server.get("/notification-activity").await;
    assert_eq!(activity["working"], 1, "a chat starting works");
    let id = agent.chat.as_str();
    agent.daemon.send(
        1,
        id,
        Some(1),
        json!({ "type": "closed", "reason": { "type": "exited", "message": "x" } }),
    );
    for _ in 0..200 {
        if agent.server.get("/notification-activity").await.1["working"] == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("a closed chat still counts");
}
