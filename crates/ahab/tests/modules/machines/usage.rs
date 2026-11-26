//! The plan limits of the agents signed in on a machine, asked of its
//! daemon and kept a while by the server.

use reqwest::StatusCode;

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

#[tokio::test]
async fn a_machine_reports_its_plan_limits_kept_a_while() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([7; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    let path = format!("/machines/{}/agents/usage", machine.id);

    let (status, usage) = server.get(&path).await;
    assert_eq!(status, StatusCode::OK, "{usage}");
    let claude = &usage["items"][0];
    assert_eq!(claude["agent"], "claude-code");
    assert_eq!(claude["plan"], "Max 5x");
    assert_eq!(claude["windows"][0]["shortLabel"], "5h");
    assert_eq!(claude["windows"][0]["usedPercent"], 42.0);

    // Asked again within minutes, the server answers from what it kept…
    assert_eq!(server.get(&path).await.0, StatusCode::OK);
    assert_eq!(daemon.asked_for("agents.usage").len(), 1);
    // …unless told to ask now.
    assert_eq!(
        server.get(&format!("{path}?refresh=true")).await.0,
        StatusCode::OK
    );
    assert_eq!(daemon.asked_for("agents.usage").len(), 2);

    // Offline, the machine still shows what it last said.
    daemon.disconnect();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let (status, usage) = server.get(&format!("{path}?refresh=true")).await;
    assert_eq!(status, StatusCode::OK, "{usage}");
    assert_eq!(usage["items"][0]["plan"], "Max 5x");
}

#[tokio::test]
async fn an_offline_machine_with_nothing_kept_says_so() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([8; 32]);
    let machine = server.local_machine(&key, "atlas").await;

    let (status, problem) = server
        .get(&format!("/machines/{}/agents/usage", machine.id))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(problem["code"], "machine_offline");
}

#[tokio::test]
async fn agents_are_checked_for_updates_hourly_and_updated_on_request() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([5; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    let path = format!("/machines/{}/agents/updates", machine.id);

    let (status, updates) = server.get(&path).await;
    assert_eq!(status, StatusCode::OK, "{updates}");
    assert_eq!(updates["items"][0]["latest"], "0.2.0");
    server.get(&path).await;
    assert_eq!(daemon.asked_for("agents.updates").len(), 1);

    let (status, updated) = server
        .post(
            &format!("/machines/{}/agents/codex/update", machine.id),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["after"], "0.2.0");
    assert_eq!(
        daemon.asked_for("agents.update"),
        [serde_json::json!({ "agent": "codex" })]
    );
    // An update forgets the check, and detects the agents again.
    server.get(&path).await;
    assert_eq!(daemon.asked_for("agents.updates").len(), 2);
    assert!(!daemon.asked_for("agents.detect").is_empty());

    let (status, _) = server
        .post(
            &format!("/machines/{}/agents/vim/update", machine.id),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agents_are_added_from_the_registry_or_custom_and_detected_with_them() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([4; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;

    let (status, catalog) = server.get("/agents/catalog").await;
    assert_eq!(status, StatusCode::OK, "{catalog}");
    let items = catalog["items"].as_array().unwrap();
    assert!(items.iter().any(|agent| agent["id"] == "claude-code"));
    let gemini = items
        .iter()
        .find(|agent| agent["id"] == "acp:gemini")
        .unwrap();
    assert_eq!(gemini["addable"], true);
    let downloads_only = items
        .iter()
        .find(|agent| agent["addable"] == false && agent["source"] == "registry")
        .unwrap();

    let path = format!("/machines/{}/agents", machine.id);
    let (status, added) = server
        .post(&path, serde_json::json!({ "agent": "acp:gemini" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["name"], gemini["name"]);
    let (status, custom) = server
        .post(
            &path,
            serde_json::json!({ "name": "My Agent", "program": "my-agent", "args": ["--acp"] }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{custom}");
    assert_eq!(custom["agent"], "custom:my-agent");
    assert_eq!(
        server
            .post(&path, serde_json::json!({ "agent": downloads_only["id"] }))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        server
            .post(&path, serde_json::json!({ "agent": "acp:no-such-agent" }))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // Detection is told about both: the registry entry, and the command.
    let (status, _) = server
        .post(
            &format!("/machines/{}/agents/refresh", machine.id),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let asked = daemon.asked_for("agents.detect");
    let last = asked.last().unwrap();
    let added = last["added"].as_array().unwrap();
    let registry = added
        .iter()
        .find(|agent| agent["agent"] == "acp:gemini")
        .unwrap();
    assert_eq!(registry["entry"]["id"], "gemini");
    let custom = added
        .iter()
        .find(|agent| agent["agent"] == "custom:my-agent")
        .unwrap();
    assert_eq!(custom["launch"]["program"], "my-agent");
    assert_eq!(custom["launch"]["args"], serde_json::json!(["--acp"]));

    let (status, _) = server
        .call(
            reqwest::Method::DELETE,
            &format!("/machines/{}/agents/custom:my-agent", machine.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = server
        .call(
            reqwest::Method::DELETE,
            &format!("/machines/{}/agents/custom:my-agent", machine.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agents_logos_come_from_keel_and_unknown_agents_have_none() {
    let server = TestServer::start().await;
    let response = server
        .http()
        .get(format!("{}/api/v1/agents/claude-code/logo", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "image/svg+xml");
    assert!(
        headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'none'")
    );
    assert!(response.text().await.unwrap().starts_with("<svg"));

    let missing = server
        .http()
        .get(format!("{}/api/v1/agents/custom:mine/logo", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn models_are_kept_by_ahab_and_read_again_on_request() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([3; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    let path = format!("/machines/{}/agents/claude-code/models", machine.id);

    let (status, first) = server.get(&path).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["refreshing"], false);
    assert!(first["fetchedAt"].is_string());
    server.get(&path).await;
    assert_eq!(daemon.asked_for("agents.catalog").len(), 1);

    let (status, _) = server.get(&format!("{path}?refresh=true")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(daemon.asked_for("agents.catalog").len(), 2);
}
