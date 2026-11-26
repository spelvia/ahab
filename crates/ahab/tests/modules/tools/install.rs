//! Installing a tool from a GitHub release, checked against its record's
//! SHA-256: here a plugin's record, served by a local stand-in for GitHub.

use std::io::Write as _;

use reqwest::StatusCode;
use serde_json::json;
use sha2::Digest as _;

use super::local_daemon;
use crate::support::TestServer;

/// A gzipped script that says its version, and its SHA-256.
fn release() -> (Vec<u8>, String) {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(b"#!/bin/sh\necho fake-tool 1.2.3\n")
        .unwrap();
    let bytes = encoder.finish().unwrap();
    let digest = ahab::modules::tools::registry::daemon::hex(&sha2::Sha256::digest(&bytes));
    (bytes, digest)
}

/// Serves `bytes` at `/acme/fake-tool/releases/download/v1.2.3/<file>`.
async fn github(bytes: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/acme/fake-tool/releases/download/v1.2.3/{file}",
        axum::routing::get(move || {
            let bytes = bytes.clone();
            async move { bytes }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}")
}

fn plugin(sha256: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let platform = ahab::modules::tools::registry::model::platform();
    let manifest = json!({
        "id": "fake-tools", "name": "Fake tools", "version": "0.1.0", "ahab": ">=0.1.2 <0.2.0",
        "contributes": { "tools": [{
            "id": "fake-tool", "name": "Fake tool", "kind": "formatter", "languages": ["rust"],
            "license": "MIT",
            "source": { "type": "githubRelease", "repo": "acme/fake-tool", "version": "1.2.3",
                "tag": "v${version}",
                "assets": { platform: { "file": "fake-tool-${version}.gz", "sha256": sha256 } } },
            "bin": { "fake-tool": "bin/fake-tool" },
            "run": { "command": ["fake-tool"] }
        }]}
    });
    std::fs::write(dir.path().join("ahab-plugin.json"), manifest.to_string()).unwrap();
    dir
}

async fn job_done(server: &TestServer, machine: uuid::Uuid) -> serde_json::Value {
    for _ in 0..200 {
        let (_, tools) = server.get(&format!("/machines/{machine}/tools")).await;
        let job = tools["jobs"]
            .as_array()
            .and_then(|jobs| jobs.iter().find(|job| job["tool"] == "fake-tool").cloned());
        if let Some(job) = job
            && job["state"] != "running"
        {
            return tools;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the job never finished");
}

#[tokio::test]
async fn a_tool_installs_from_a_checked_release_and_is_removed() {
    let server = TestServer::start().await;
    let (bytes, digest) = release();
    let base = github(bytes).await;
    let (machine, _daemon) = local_daemon(&server, Some(base)).await;
    let dir = plugin(&digest);
    let (status, body) = server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": dir.path() }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    server.post("/plugins/fake-tools/enable", json!({})).await;

    let (_, tools) = server.get(&format!("/machines/{}/tools", machine.id)).await;
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "fake-tool")
        .unwrap()
        .clone();
    assert_eq!(tool["state"], "available", "{tool}");
    assert_eq!(tool["origin"], "fake-tools");

    let (status, job) = server
        .post(
            &format!("/machines/{}/tools/fake-tool/install", machine.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{job}");
    let tools = job_done(&server, machine.id).await;
    let job = tools["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["tool"] == "fake-tool")
        .unwrap();
    assert_eq!(job["state"], "done", "{job}");
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "fake-tool")
        .unwrap();
    assert_eq!(tool["state"], "installed", "{tool}");
    assert_eq!(tool["foundVersion"], "1.2.3");
    let installed = std::path::Path::new(tool["path"].as_str().unwrap());
    assert!(
        installed.ends_with("tools/fake-tool/1.2.3/bin/fake-tool"),
        "{installed:?}"
    );

    let (status, _) = server
        .post(
            &format!("/machines/{}/tools/fake-tool/remove", machine.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let tools = job_done(&server, machine.id).await;
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "fake-tool")
        .unwrap();
    assert_eq!(tool["state"], "available");
    assert!(!installed.exists());
}

#[tokio::test]
async fn a_download_that_does_not_match_its_checksum_is_refused() {
    let server = TestServer::start().await;
    let (bytes, _) = release();
    let base = github(bytes).await;
    let (machine, _daemon) = local_daemon(&server, Some(base)).await;
    let dir = plugin(&"0".repeat(64));
    server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": dir.path() }),
        )
        .await;
    server.post("/plugins/fake-tools/enable", json!({})).await;
    server
        .post(
            &format!("/machines/{}/tools/fake-tool/install", machine.id),
            json!({}),
        )
        .await;
    let tools = job_done(&server, machine.id).await;
    let job = tools["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["tool"] == "fake-tool")
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["step"], "verifying");
    assert!(
        job["message"].as_str().unwrap().contains("SHA-256"),
        "{job}"
    );
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "fake-tool")
        .unwrap();
    assert_eq!(tool["state"], "available", "nothing was left behind");
}
