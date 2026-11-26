//! Daemons of other versions: the server accepts
//! every released hello and newer ones, and never asks a daemon for a
//! method it lacks.

use std::path::Path;
use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use ahab::modules::machines::MachineKey;
use ahab::shared::error::ServiceError;

use crate::support::{FakeDaemon, TestServer};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A hello a released daemon sent, from `tests/fixtures/protocol/`. Its key
/// and signature are replaced: the signature covers each connection's
/// challenge.
fn recorded(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/protocol")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn is_outdated(result: Result<Value, ServiceError>) -> bool {
    matches!(
        result,
        Err(ServiceError::Conflict {
            code: "machine_outdated",
            ..
        })
    )
}

#[tokio::test]
async fn a_released_hello_is_accepted() {
    for name in ["hello-v0.1.0.json"] {
        let server = TestServer::start().await;
        let key = MachineKey::from_secret([21; 32]);
        let machine = server.local_machine(&key, "atlas").await;

        let daemon = FakeDaemon::connect_hello(&server, &key, recorded(name)).await;
        assert!(server.state.presence.is_online(machine.id), "{name}");
        let answer = server
            .state
            .presence
            .request(machine.id, "ping", json!({}), TIMEOUT)
            .await;
        assert!(answer.is_ok(), "{name}: {answer:?}");
        assert_eq!(daemon.asked_for("ping").len(), 1);
    }
}

#[tokio::test]
async fn a_newer_hello_with_fields_this_server_does_not_know_is_accepted() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([22; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let mut hello = recorded("hello-v0.1.0.json");
    hello["facts"]["version"] = json!("9.0.0");
    hello["facts"]["kernel"] = json!("6.18.63");
    hello["capabilities"] = json!(["ping", "machine.metrics", "fs.list2"]);
    hello["features"] = json!({ "compression": "zstd" });

    let _daemon = FakeDaemon::connect_hello(&server, &key, hello).await;
    assert!(server.state.presence.is_online(machine.id));
    let stored = server.state.machines.get(machine.id).await.unwrap();
    assert_eq!(stored.version.as_deref(), Some("9.0.0"));
    let presence = &server.state.presence;
    assert!(presence.supports(machine.id, "fs.list2"));
    assert!(!presence.supports(machine.id, "chats.start"));
}

#[tokio::test]
async fn a_daemon_without_capabilities_handles_the_protocol_1_methods() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([23; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    let presence = &server.state.presence;

    assert!(presence.supports(machine.id, "machine.metrics"));
    let (status, _) = server
        .get(&format!("/machines/{}/metrics", machine.id))
        .await;
    assert_eq!(status, StatusCode::OK);

    // A method newer than protocol 1 is never sent to it.
    let answer = presence
        .request(machine.id, "fs.list2", json!({}), TIMEOUT)
        .await;
    assert!(is_outdated(answer));
    assert!(daemon.asked_for("fs.list2").is_empty());

    // Beyond the baseline, this server calls the editor's file methods
    // (and moving files by dragging), plugins' notifications, answering
    // every kind of request and language tooling's: such a daemon is
    // outdated for those alone.
    let (_, dto) = server.get(&format!("/machines/{}", machine.id)).await;
    assert_eq!(dto["outdated"], true, "{dto}");
    assert_eq!(
        dto["missingMethods"],
        json!([
            "fs.image",
            "fs.write",
            "fs.create",
            "fs.rename",
            "fs.delete",
            "fs.watch",
            "fs.move",
            "plugins.intercept",
            "plugins.notify",
            "chats.fork",
            "chats.commands",
            "chats.modes",
            "chats.previewCommands",
            "chats.startCommand",
            "chats.invoke",
            "chats.resumeCommand",
            "chats.taskOutput",
            "chats.stopWork",
            "chats.agentHistory",
            "chats.decide",
            "lsp.connect",
            "tools.status",
            "tools.servers",
            "tools.restart",
            "tools.log",
            "tools.install",
            "tools.check",
            "tools.checkRoots",
            "tools.pins"
        ])
    );
}

#[tokio::test]
async fn a_method_the_daemon_lacks_fails_without_reaching_it() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([24; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect_with(&server, &key, Some(&["ping", "agents.usage"])).await;

    let (status, problem) = server
        .get(&format!("/machines/{}/metrics", machine.id))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "machine_outdated");
    assert!(
        problem["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("older Ahab (0.1.0)")),
        "{problem}"
    );
    assert!(daemon.asked_for("machine.metrics").is_empty());

    // What it has still works.
    let (status, _) = server
        .get(&format!("/machines/{}/agents/usage", machine.id))
        .await;
    assert_eq!(status, StatusCode::OK);

    // The machine says it runs an older Ahab, and what it lacks.
    let (_, dto) = server.get(&format!("/machines/{}", machine.id)).await;
    assert_eq!(dto["outdated"], true, "{dto}");
    let missing = dto["missingMethods"].as_array().unwrap();
    assert!(missing.contains(&json!("machine.metrics")));
    assert!(!missing.contains(&json!("agents.usage")));
    let (_, list) = server.get("/machines").await;
    assert_eq!(list["items"][0]["outdated"], true);

    // Offline, nothing is known.
    daemon.disconnect();
    server
        .eventually("the machine is offline", |state| {
            !state.presence.is_online(machine.id)
        })
        .await;
    let (_, dto) = server.get(&format!("/machines/{}", machine.id)).await;
    assert_eq!(dto["outdated"], false);
    assert_eq!(dto["missingMethods"], json!([]));
}

#[tokio::test]
async fn an_unknown_method_answer_means_the_machine_is_outdated() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([25; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    // It lists a method it does not handle (its list drifted).
    let daemon = FakeDaemon::connect_with(&server, &key, Some(&["ping", "gone.away"])).await;

    let answer = server
        .state
        .presence
        .request(machine.id, "gone.away", json!({}), TIMEOUT)
        .await;
    assert!(is_outdated(answer));
    assert_eq!(daemon.asked_for("gone.away").len(), 1);
}
