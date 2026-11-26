//! Problems and checks: a check command's output through its adapter,
//! the server's snapshot and filters, project totals, settings.

use reqwest::StatusCode;
use serde_json::json;

use super::{folder, local_daemon, project};
use crate::support::TestServer;

/// A check that prints one problem in a line pattern, and one that fails.
fn checks() -> serde_json::Value {
    json!([
        {
            "id": "fake-check", "name": "Fake check", "kind": "check", "languages": ["rust"],
            "run": {
                "command": ["sh", "-c", "printf 'src/main.rs:1:4: error: fake problem [F001]\\nsrc/main.rs:2:1: warning: a smell [F002]\\n'"],
                "rootMarkers": ["Cargo.toml"],
                "on": ["request", "agentTurn"],
                "output": { "format": "pattern", "pattern": {
                    "regexp": "^(?P<file>[^:]+):(?P<line>\\d+):(?P<column>\\d+): (?P<severity>\\w+): (?P<message>.*) \\[(?P<code>\\w+)\\]$"
                }}
            }
        },
        {
            "id": "broken-check", "name": "Broken check", "kind": "check", "languages": ["rust"],
            "run": {
                "command": ["sh", "-c", "echo 'cannot read the project' >&2; exit 3"],
                "rootMarkers": ["Cargo.toml"],
                "output": { "format": "sarif" }
            }
        }
    ])
}

#[tokio::test]
async fn checks_find_problems_and_the_server_keeps_them() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (_dir, path) = folder();
    let (status, body) = server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({
                "records": checks(),
                "languages": { "rust": { "tools": ["fake-check", "broken-check"] } }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (project, id) = project(&server, &path).await;

    let (status, ran) = server
        .post(&format!("/projects/{id}/checks"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{ran}");
    let checks = ran["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 2, "{ran}");
    let fake = checks.iter().find(|c| c["tool"] == "fake-check").unwrap();
    assert_eq!(fake["ok"], true);
    assert_eq!(fake["counts"]["errors"], 1);
    let broken = checks.iter().find(|c| c["tool"] == "broken-check").unwrap();
    assert_eq!(broken["ok"], false);
    assert_eq!(broken["message"], "cannot read the project");
    assert_eq!(ran["counts"]["warnings"], 1);

    let (_, problems) = server.get(&format!("/projects/{id}/problems")).await;
    let items = problems["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{problems}");
    assert_eq!(items[0]["code"], "F001");
    assert_eq!(items[0]["severity"], "error");
    assert_eq!(items[0]["line"], 1);
    assert_eq!(items[0]["column"], 4);
    assert_eq!(items[0]["source"], "fake-check");
    let (_, errors) = server
        .get(&format!("/projects/{id}/problems?severity=error"))
        .await;
    assert_eq!(errors["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        errors["counts"]["warnings"], 1,
        "counts are of every problem"
    );
    let (_, found) = server
        .get(&format!("/projects/{id}/problems?text=smell&path=src"))
        .await;
    assert_eq!(found["items"][0]["code"], "F002");

    let (_, summary) = server
        .get(&format!("/projects/{project}/problems/summary"))
        .await;
    assert_eq!(summary["total"]["errors"], 1);

    // Only the check named; its problems replace its own.
    std::fs::remove_file(format!("{path}/Cargo.toml")).unwrap();
    let (_, ran) = server
        .post(
            &format!("/projects/{id}/checks"),
            json!({ "tools": ["fake-check"] }),
        )
        .await;
    assert_eq!(
        ran["checks"].as_array().unwrap().len(),
        0,
        "no root left: {ran}"
    );
}

#[tokio::test]
async fn tool_settings_are_checked_and_kept_per_project() {
    let server = TestServer::start().await;
    let id = &server.stored_project("app").await;
    let (status, body) = server
        .put(
            &format!("/projects/{id}/tool-settings"),
            json!({ "tools": { "cargo-clippy": { "on": ["sometimes"] } } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "validation_failed");
    let (status, _) = server
        .put(
            &format!("/projects/{id}/tool-settings"),
            json!({ "maxServers": 2 }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a machine's setting"
    );
    let choice = json!({
        "languages": { "typescript": { "tools": ["biome", "typescript-language-server"], "formatOnSave": true } },
        "environment": { "nix": true }
    });
    let (status, saved) = server
        .put(&format!("/projects/{id}/tool-settings"), choice.clone())
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (_, read) = server.get(&format!("/projects/{id}/tool-settings")).await;
    assert_eq!(read, choice);
    let (status, _) = server
        .get("/projects/01900000-0000-7000-8000-000000000000/tool-settings")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
