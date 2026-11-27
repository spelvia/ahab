//! Daemons connecting to the server: the handshake, its refusals,
//! online state, and requests to a connected daemon.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use ahab::daemon::{self, DaemonConfig, DaemonError};
use ahab::modules::machines::machine::store::MachineRepository;
use ahab::modules::machines::presence::api::{Handshake, MachineFacts, connect_message};
use ahab::modules::machines::{Machine, MachineKey, MachineName};
use ahab::shared::error::ServiceError;
use ahab::shared::time::Clock;

use crate::support::TestServer;

fn facts() -> MachineFacts {
    MachineFacts {
        os: "linux".into(),
        arch: "x86_64".into(),
        hostname: "atlas".into(),
        version: "0.1.0".into(),
    }
}

/// Starts a daemon with `key`; stops when the returned sender says so.
fn start_daemon(
    server: &TestServer,
    key: MachineKey,
) -> (
    watch::Sender<bool>,
    tokio::task::JoinHandle<Result<(), DaemonError>>,
) {
    start_pinned_daemon(server, key, None)
}

/// A daemon that pinned `fingerprint` as its server's key.
fn start_pinned_daemon(
    server: &TestServer,
    key: MachineKey,
    fingerprint: Option<String>,
) -> (
    watch::Sender<bool>,
    tokio::task::JoinHandle<Result<(), DaemonError>>,
) {
    let (stop, stopped) = watch::channel(false);
    let config = DaemonConfig {
        server: server.url.clone(),
        key,
        facts: facts(),
        server_fingerprint: fingerprint,
        data_dir: server.daemon_dir(),
        executable: None,
        plugin_node: None,
        tools: ahab::modules::tools::registry::daemon::ToolOptions {
            login_shell: false,
            ..Default::default()
        },
    };
    (stop, tokio::spawn(daemon::run(config, stopped)))
}

/// Opens the socket, reads the challenge, sends a hello signed by `key` (or
/// tampered with), and returns the server's next message.
async fn raw_hello(
    server: &TestServer,
    key: &MachineKey,
    protocol: u32,
    sign_host: &str,
) -> Message {
    let url = server.url.replace("http://", "ws://") + "/api/v1/daemon/connect";
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let Some(Ok(Message::Text(text))) = socket.next().await else {
        panic!("no challenge");
    };
    let Ok(Handshake::Challenge { nonce }) = serde_json::from_str(&text) else {
        panic!("not a challenge: {text}");
    };
    let hello = Handshake::Hello {
        protocol,
        public_key: STANDARD.encode(key.public_key().as_bytes()),
        signature: STANDARD.encode(key.sign(&connect_message(sign_host, &nonce))),
        facts: facts(),
        challenge: "test".into(),
        capabilities: None,
    };
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    socket.next().await.unwrap().unwrap()
}

fn host(server: &TestServer) -> String {
    server.url.trim_start_matches("http://").to_owned()
}

fn close_code(message: &Message) -> Option<u16> {
    match message {
        Message::Close(Some(frame)) => Some(u16::from(frame.code)),
        _ => None,
    }
}

#[tokio::test]
async fn the_local_daemon_connects_and_its_machine_is_online() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([1; 32]);
    let machine = server
        .state
        .machines
        .ensure_local(key.public_key(), "atlas")
        .await
        .unwrap();

    let (stop, daemon) = start_daemon(&server, key);
    server
        .eventually("the machine is online", |state| {
            state.presence.is_online(machine.id)
        })
        .await;

    let stored = server.state.machines.get(machine.id).await.unwrap();
    assert_eq!(stored.os.as_deref(), Some("linux"));
    assert_eq!(stored.arch.as_deref(), Some("x86_64"));
    assert_eq!(stored.version.as_deref(), Some("0.1.0"));
    assert_eq!(stored.last_seen_at, Some(server.clock.now()));

    let answer = server
        .http()
        .get(format!("{}/api/v1/machines", server.url))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(answer["items"][0]["online"], true);

    stop.send(true).unwrap();
    assert!(daemon.await.unwrap().is_ok());
    server
        .eventually("the machine is offline", |state| {
            !state.presence.is_online(machine.id)
        })
        .await;
}

#[tokio::test]
async fn answers_requests_from_the_server() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([1; 32]);
    let machine = server
        .state
        .machines
        .ensure_local(key.public_key(), "atlas")
        .await
        .unwrap();
    let (_stop, _daemon) = start_daemon(&server, key);
    server
        .eventually("the machine is online", |state| {
            state.presence.is_online(machine.id)
        })
        .await;

    let pong = server
        .state
        .presence
        .request(machine.id, "ping", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(pong["pong"], true);

    let unknown = server
        .state
        .presence
        .request(
            machine.id,
            "nothing.here",
            json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
    // A method the daemon does not list is never sent: it is too old for it.
    assert!(
        matches!(&unknown, ServiceError::Conflict { code: "machine_outdated", message } if message.contains("nothing.here")),
        "{unknown:?}"
    );
}

#[tokio::test]
async fn a_request_to_an_offline_machine_fails_at_once() {
    let server = TestServer::start().await;
    let machine = server
        .state
        .machines
        .ensure_local(MachineKey::from_secret([1; 32]).public_key(), "atlas")
        .await
        .unwrap();

    let error = server
        .state
        .presence
        .request(machine.id, "ping", json!({}), Duration::from_secs(5))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ServiceError::Unavailable {
            code: "machine_offline",
            ..
        }
    ));
}

#[tokio::test]
async fn an_unknown_machine_is_told_to_join_and_the_daemon_stops() {
    let server = TestServer::start().await;

    let (_stop, daemon) = start_daemon(&server, MachineKey::from_secret([9; 32]));

    let result = tokio::time::timeout(Duration::from_secs(5), daemon)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(DaemonError::UnknownMachine)));
}

#[tokio::test]
async fn refuses_a_bad_signature_a_pending_machine_and_an_old_protocol() {
    let server = TestServer::start().await;
    let active = MachineKey::from_secret([1; 32]);
    server
        .state
        .machines
        .ensure_local(active.public_key(), "atlas")
        .await
        .unwrap();
    let waiting = MachineKey::from_secret([2; 32]);
    let pending = Machine::pending(
        MachineName::parse("box").unwrap(),
        waiting.public_key(),
        server.clock.now(),
    );
    server.machines().insert(&pending).await.unwrap();

    // Signed for another host: the signature does not match.
    let wrong_host = raw_hello(&server, &active, 1, "evil.example").await;
    assert_eq!(close_code(&wrong_host), Some(4401));

    let waits = raw_hello(&server, &waiting, 1, &host(&server)).await;
    assert_eq!(close_code(&waits), Some(4403));

    let outdated = raw_hello(&server, &active, 0, &host(&server)).await;
    assert_eq!(close_code(&outdated), Some(4426));

    let welcome = raw_hello(&server, &active, 1, &host(&server)).await;
    let Message::Text(text) = welcome else {
        panic!("expected a welcome, got {welcome:?}");
    };
    assert!(matches!(
        serde_json::from_str::<Handshake>(&text),
        Ok(Handshake::Welcome { protocol: 1, .. })
    ));
}

#[tokio::test]
async fn a_newer_connection_replaces_an_older_one() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([1; 32]);
    server
        .state
        .machines
        .ensure_local(key.public_key(), "atlas")
        .await
        .unwrap();
    let url = server.url.replace("http://", "ws://") + "/api/v1/daemon/connect";

    // The first connection, kept open after its welcome.
    let (mut first, _) = tokio_tungstenite::connect_async(url.clone()).await.unwrap();
    let Some(Ok(Message::Text(text))) = first.next().await else {
        panic!("no challenge");
    };
    let Ok(Handshake::Challenge { nonce }) = serde_json::from_str(&text) else {
        panic!("not a challenge");
    };
    let hello = Handshake::Hello {
        protocol: 1,
        public_key: STANDARD.encode(key.public_key().as_bytes()),
        signature: STANDARD.encode(key.sign(&connect_message(&host(&server), &nonce))),
        facts: facts(),
        challenge: "test".into(),
        capabilities: None,
    };
    first
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(first.next().await, Some(Ok(Message::Text(_)))));

    // The second connection is welcomed; the first is closed.
    let second = raw_hello(&server, &key, 1, &host(&server)).await;
    assert!(matches!(second, Message::Text(_)));
    // The server's requests to a new connection (its projects, its
    // agents) may arrive first.
    let closed = loop {
        match first.next().await.unwrap().unwrap() {
            Message::Text(_) => {}
            other => break other,
        }
    };
    assert_eq!(close_code(&closed), Some(4409));
}

#[tokio::test]
async fn a_joined_daemon_connects_to_the_server_it_pinned() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([1; 32]);
    let machine = server
        .state
        .machines
        .ensure_local(key.public_key(), "atlas")
        .await
        .unwrap();
    let pinned = server.state.presence.server_fingerprint();

    let (_stop, _daemon) = start_pinned_daemon(&server, key, Some(pinned));

    server
        .eventually("the machine is online", |state| {
            state.presence.is_online(machine.id)
        })
        .await;
}

#[tokio::test]
async fn a_joined_daemon_refuses_a_server_with_another_key() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([1; 32]);
    server
        .state
        .machines
        .ensure_local(key.public_key(), "atlas")
        .await
        .unwrap();
    let other = MachineKey::from_secret([77; 32])
        .public_key()
        .fingerprint()
        .to_string();

    let (_stop, daemon) = start_pinned_daemon(&server, key, Some(other));

    let result = tokio::time::timeout(Duration::from_secs(5), daemon)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(DaemonError::ServerKeyChanged { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn removing_a_machine_closes_its_connection_and_its_daemon_stops() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([3; 32]);
    let machine = server
        .state
        .machines
        .join(key.public_key(), None, "box", true)
        .await
        .unwrap();
    let (_stop, daemon) = start_daemon(&server, key);
    server
        .eventually("the machine is online", |state| {
            state.presence.is_online(machine.id)
        })
        .await;

    let removed = server
        .http()
        .delete(format!("{}/api/v1/machines/{}", server.url, machine.id))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), 204);

    let result = tokio::time::timeout(Duration::from_secs(5), daemon)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(DaemonError::UnknownMachine)),
        "{result:?}"
    );
    assert!(!server.state.presence.is_online(machine.id));
}
