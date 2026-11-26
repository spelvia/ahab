//! Web Push: browsers subscribe, and new notifications
//! are pushed to them, encrypted, while "outside the app" is on.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use serde_json::json;

use ahab::modules::notifications::notification::model::{Level, NewNotification};
use ahab::modules::notifications::push::model::{NewSubscription, encode};

use crate::support::TestServer;

/// What a push service got: its headers and body length.
type Received = Arc<Mutex<Vec<(HeaderMap, usize)>>>;

/// A stand-in push service answering `status`.
async fn push_service(status: StatusCode) -> (String, Received) {
    let received: Received = Arc::default();
    let kept = Arc::clone(&received);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/send/browser-1", listener.local_addr().unwrap());
    let app = axum::Router::new().route(
        "/send/browser-1",
        axum::routing::post(move |headers: HeaderMap, body: axum::body::Bytes| {
            let kept = Arc::clone(&kept);
            async move {
                kept.lock().unwrap().push((headers, body.len()));
                status
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, received)
}

fn browser(endpoint: &str) -> NewSubscription {
    // A real P-256 point: the generator.
    let mut key = vec![4];
    key.extend(hex(
        "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
    ));
    key.extend(hex(
        "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5",
    ));
    NewSubscription {
        endpoint: endpoint.to_owned(),
        p256dh: key,
        auth: vec![7; 16],
        label: "Firefox on Linux".to_owned(),
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

async fn this_client(server: &TestServer) -> uuid::Uuid {
    let (_, current) = server.get("/clients/current").await;
    current["id"].as_str().unwrap().parse().unwrap()
}

fn finished() -> NewNotification {
    NewNotification {
        kind: "sessions.finished".into(),
        source: "ahab".into(),
        level: Level::Success,
        title: "Claude Code finished".into(),
        body: Some("Fix the flaky test".into()),
        project_id: None,
        target: json!({ "type": "machine", "machineId": "m1" }),
        group_key: None,
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting until {what}");
}

#[tokio::test]
async fn a_browser_subscribes_with_the_servers_key() {
    let server = TestServer::start().await;
    let (status, key) = server.get("/push/key").await;
    assert_eq!(status, StatusCode::OK);
    // 65 bytes, base64url without padding.
    assert_eq!(key["publicKey"].as_str().unwrap().len(), 87);
    assert_eq!(
        server.get("/push/key").await.1,
        key,
        "the same key each time"
    );

    let keys = json!({ "p256dh": encode(&browser("x").p256dh), "auth": encode(&[1; 16]) });
    let (status, created) = server
        .post(
            "/push-subscriptions",
            json!({ "endpoint": "https://push.example.org/send/abc", "keys": keys, "label": "Firefox on Linux" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["service"], "push.example.org");
    assert_eq!(created["thisClient"], true);

    let (_, list) = server.get("/push-subscriptions").await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let (status, wrong) = server
        .post(
            "/push-subscriptions",
            json!({ "endpoint": "http://push.example.org/send/abc", "keys": keys }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{wrong}");

    let id = created["id"].as_str().unwrap();
    let gone = server
        .http()
        .delete(format!("{}/api/v1/push-subscriptions/{id}", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), StatusCode::NO_CONTENT);
    assert_eq!(server.get("/push-subscriptions").await.1, json!([]));
}

#[tokio::test]
async fn a_new_notification_is_pushed_encrypted_and_signed() {
    let server = TestServer::start().await;
    let (endpoint, received) = push_service(StatusCode::CREATED).await;
    let client = this_client(&server).await;
    server
        .state
        .push
        .subscribe(client, browser(&endpoint))
        .await
        .unwrap();

    server.state.notifications.send(finished()).await.unwrap();
    until("it is pushed", || received.lock().unwrap().len() == 1).await;
    let (headers, length) = received.lock().unwrap()[0].clone();
    assert_eq!(headers["content-encoding"], "aes128gcm");
    assert_eq!(headers["ttl"], "86400");
    assert_eq!(headers["urgency"], "normal");
    let authorization = headers["authorization"].to_str().unwrap();
    let key = server.get("/push/key").await.1["publicKey"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(authorization.starts_with("vapid t="), "{authorization}");
    assert!(
        authorization.ends_with(&format!(", k={key}")),
        "{authorization}"
    );
    // Salt, record size, key length, the server's key, then the record.
    assert!(length > 16 + 4 + 1 + 65 + 16, "{length}");

    let (_, list) = server.get("/push-subscriptions").await;
    assert!(list[0]["lastUsedAt"].is_string(), "{list}");
}

#[tokio::test]
async fn nothing_is_pushed_while_outside_the_app_is_off() {
    let server = TestServer::start().await;
    let (endpoint, received) = push_service(StatusCode::CREATED).await;
    let client = this_client(&server).await;
    server
        .state
        .push
        .subscribe(client, browser(&endpoint))
        .await
        .unwrap();
    let (status, _) = server
        .put(
            "/notification-settings",
            json!({ "enabled": true, "keepDays": 30, "outside": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    server.state.notifications.send(finished()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(received.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_browser_that_unsubscribed_is_forgotten() {
    let server = TestServer::start().await;
    let (endpoint, received) = push_service(StatusCode::GONE).await;
    let client = this_client(&server).await;
    let subscription = server
        .state
        .push
        .subscribe(client, browser(&endpoint))
        .await
        .unwrap();
    let (status, test) = server
        .post(
            &format!("/push-subscriptions/{}/test", subscription.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(test["outcome"], "gone");
    assert_eq!(received.lock().unwrap().len(), 1);
    assert_eq!(server.get("/push-subscriptions").await.1, json!([]));
}

#[tokio::test]
async fn a_revoked_client_gets_no_more_pushes() {
    let server = TestServer::start().await;
    let (endpoint, received) = push_service(StatusCode::CREATED).await;
    let (_, token) = server
        .post("/clients/tokens", json!({ "name": "old phone" }))
        .await;
    let phone: uuid::Uuid = token["client"]["id"].as_str().unwrap().parse().unwrap();
    server
        .state
        .push
        .subscribe(phone, browser(&endpoint))
        .await
        .unwrap();
    let revoked = server
        .http()
        .delete(format!("{}/api/v1/clients/{phone}", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    assert_eq!(server.get("/push-subscriptions").await.1, json!([]));

    server.state.notifications.send(finished()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(received.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_kind_kept_inside_ahab_is_not_pushed() {
    let server = TestServer::start().await;
    let (endpoint, received) = push_service(StatusCode::CREATED).await;
    let client = this_client(&server).await;
    server
        .state
        .push
        .subscribe(client, browser(&endpoint))
        .await
        .unwrap();
    let (status, settings) = server
        .put(
            "/notification-settings",
            json!({ "enabled": true, "keepDays": 30, "outside": true,
                    "insideOnly": ["sessions.finished"] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let finished_kind = settings["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["id"] == "sessions.finished")
        .unwrap()
        .clone();
    assert_eq!(finished_kind["outside"], false);
    assert_eq!(finished_kind["enabled"], true);
    // Kept, and shown inside: only not pushed.
    assert!(
        server
            .state
            .notifications
            .send(finished())
            .await
            .unwrap()
            .is_some()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(received.lock().unwrap().is_empty());
}
