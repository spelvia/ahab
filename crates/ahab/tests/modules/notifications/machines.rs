//! Notifications about machines and Ahab itself: a
//! machine with work on it going offline, an older daemon, a newer release.

use std::time::Duration;

use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;
use ahab::modules::notifications::{ReleaseCheck, Watched};

use crate::support::{FakeDaemon, TestServer};

fn watched(server: &TestServer) -> Watched {
    Watched {
        chats: server.state.chats.clone(),
        terminals: server.state.terminals.clone(),
        machines: server.state.machines.clone(),
        presence: server.state.presence.clone(),
    }
}

async fn inbox_until(server: &TestServer, what: &str, check: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        let inbox = server.get("/notifications").await.1;
        if check(&inbox) {
            return inbox;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out waiting until {what}");
}

fn of_kind<'a>(inbox: &'a Value, kind: &str) -> Vec<&'a Value> {
    inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect()
}

#[tokio::test]
async fn a_machine_with_running_work_that_goes_offline_is_told_until_it_is_back() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([31; 32]);
    let machine = server.local_machine(&key, "build-box").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    server.online(&machine).await;
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "build-box", "path": "/work/app" }),
        )
        .await;
    let project = project["id"].as_str().unwrap().to_owned();
    server
        .post(
            &format!("/projects/{project}/sessions"),
            json!({ "chat": { "agent": "claude-code", "message": "Go" } }),
        )
        .await;
    let id = machine.id;

    daemon.disconnect();
    for _ in 0..100 {
        if !server.state.presence.is_online(id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!server.state.presence.is_online(id));
    // What the server does a minute after it went: still offline, work on it.
    server
        .state
        .notifications
        .machine_offline(&watched(&server), id)
        .await
        .unwrap();
    let inbox = server.get("/notifications").await.1;
    let offline = of_kind(&inbox, "machines.offline");
    assert_eq!(offline.len(), 1);
    assert_eq!(offline[0]["title"], "build-box is offline");
    assert_eq!(offline[0]["target"]["type"], "machine");

    // Back: what it told is over.
    let _again = FakeDaemon::connect(&server, &key).await;
    inbox_until(&server, "the offline one is read", |inbox| {
        of_kind(inbox, "machines.offline")
            .first()
            .is_some_and(|n| n["readAt"].is_string())
    })
    .await;
}

#[tokio::test]
async fn a_machine_without_work_going_offline_is_not_told() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([32; 32]);
    let machine = server.local_machine(&key, "idle-box").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    server.online(&machine).await;
    daemon.disconnect();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let id = machine.id;
    server
        .state
        .notifications
        .machine_offline(&watched(&server), id)
        .await
        .unwrap();
    assert!(of_kind(&server.get("/notifications").await.1, "machines.offline").is_empty());
}

#[tokio::test]
async fn an_older_daemon_is_told_once() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([33; 32]);
    let machine = server.local_machine(&key, "old-box").await;
    let daemon = FakeDaemon::connect_with(&server, &key, Some(&["ping"])).await;
    server.online(&machine).await;
    let inbox = inbox_until(&server, "it is behind", |inbox| {
        !of_kind(inbox, "machines.behind").is_empty()
    })
    .await;
    let behind = of_kind(&inbox, "machines.behind");
    assert_eq!(behind[0]["title"], "old-box runs an older Ahab");

    // Read, then the same daemon again: not told twice.
    let id = behind[0]["id"].as_str().unwrap().to_owned();
    server
        .post(&format!("/notifications/{id}/read"), json!({}))
        .await;
    daemon.disconnect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _again = FakeDaemon::connect_with(&server, &key, Some(&["ping"])).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let inbox = server.get("/notifications").await.1;
    assert_eq!(of_kind(&inbox, "machines.behind").len(), 1);
    assert_eq!(inbox["unread"], 0);
}

/// A stand-in for GitHub's list of releases.
async fn releases(body: Value) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    let app = axum::Router::new().route(
        "/releases",
        axum::routing::get(move || async move { axum::Json(body) }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

#[tokio::test]
async fn a_newer_release_is_told_once_and_a_draft_never() {
    let server = TestServer::start().await;
    let url = releases(json!([
        { "tag_name": "v99.0.0", "draft": true, "html_url": "https://example/draft" },
        { "tag_name": "v9.1.0", "html_url": "https://example/releases/v9.1.0" },
    ]))
    .await;
    let check = ReleaseCheck {
        prereleases: false,
        url,
    };
    server
        .state
        .notifications
        .check_for_update(&check)
        .await
        .unwrap();
    server
        .state
        .notifications
        .check_for_update(&check)
        .await
        .unwrap();
    let inbox = server.get("/notifications").await.1;
    let update = of_kind(&inbox, "ahab.update-available");
    assert_eq!(update.len(), 1);
    assert_eq!(update[0]["title"], "Ahab 9.1.0 is available");
    assert_eq!(
        update[0]["target"],
        json!({ "type": "url", "url": "https://example/releases/v9.1.0" })
    );
}

#[tokio::test]
async fn no_newer_release_or_no_answer_tells_nothing() {
    let server = TestServer::start().await;
    let url = releases(json!([{ "tag_name": "v0.0.1", "html_url": "o" }])).await;
    let check = ReleaseCheck {
        prereleases: false,
        url,
    };
    server
        .state
        .notifications
        .check_for_update(&check)
        .await
        .unwrap();
    let unreachable = ReleaseCheck {
        prereleases: false,
        url: "http://127.0.0.1:9/releases".into(),
    };
    assert!(
        server
            .state
            .notifications
            .check_for_update(&unreachable)
            .await
            .is_err()
    );
    assert!(
        of_kind(
            &server.get("/notifications").await.1,
            "ahab.update-available"
        )
        .is_empty()
    );
}
