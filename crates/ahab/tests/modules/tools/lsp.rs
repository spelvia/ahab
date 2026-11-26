//! Language servers through a window's channel: diagnostics, hover,
//! definitions outside the project, formatting, the machine's list,
//! logs and restarting; and older machines.

use reqwest::StatusCode;
use serde_json::json;

use ahab::modules::machines::MachineKey;

use super::{Window, fake_lsp, folder, local_daemon, project};
use crate::support::{FakeDaemon, TestServer};

#[tokio::test]
async fn closing_the_last_document_releases_the_server_even_with_a_window_open() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (dir, path) = folder();
    for root in ["other", "third"] {
        std::fs::create_dir_all(format!("{path}/{root}/src")).unwrap();
        std::fs::write(
            format!("{path}/{root}/Cargo.toml"),
            format!("[package]\nname = \"{root}\"\n"),
        )
        .unwrap();
        std::fs::write(format!("{path}/{root}/src/main.rs"), "fn main() {}").unwrap();
    }
    let (status, saved) = server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({
                "maxServers": 1,
                "records": [fake_lsp(&dir.path().join("lib.rs"))],
                "languages": { "rust": { "tools": ["fake-lsp"] } }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (_, id) = project(&server, &path).await;
    let mut first = Window::open(&server, &id).await;
    let mut second = Window::open(&server, &id).await;
    let open = |path| {
        json!({
            "type": "open", "path": path, "language": "rust", "text": "// TODO", "version": 1
        })
    };
    let list = format!("/machines/{}/language-servers", machine.id);
    first.send(open("src/main.rs")).await;
    first.until(|f| f["type"] == "diagnostics").await;
    second.send(open("src/main.rs")).await;
    second.until(|f| f["type"] == "diagnostics").await;
    let original = server.get(&list).await.1["items"][0]["id"].clone();
    first
        .send(json!({"type": "close", "path": "src/main.rs"}))
        .await;

    // The remaining window keeps this server even when the soft count limit is reached.
    first.send(open("other/src/main.rs")).await;
    first
        .until(|f| f["type"] == "diagnostics" && f["path"] == "other/src/main.rs")
        .await;
    let both = server.get(&list).await.1;
    assert_eq!(both["items"].as_array().unwrap().len(), 2, "{both}");
    let hover = second
        .request(
            "src/main.rs",
            "textDocument/hover",
            json!({
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;
    assert!(hover.get("result").is_some(), "{hover}");
    // After the last consumer closes, the next new root can reclaim it.
    second
        .send(json!({"type": "close", "path": "src/main.rs"}))
        .await;
    second.send(open("third/src/main.rs")).await;
    second
        .until(|f| f["type"] == "diagnostics" && f["path"] == "third/src/main.rs")
        .await;
    let after = server.get(&list).await.1;
    assert_eq!(after["items"].as_array().unwrap().len(), 2, "{after}");
    assert!(
        after["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != original),
        "{after}"
    );
}

#[tokio::test]
async fn a_window_reads_diagnostics_hover_and_definitions_from_a_language_server() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (dir, path) = folder();
    let outside = dir.path().join("lib.rs");
    let (status, saved) = server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({
                "records": [fake_lsp(&outside)],
                "languages": { "rust": { "tools": ["fake-lsp"] } }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (_, id) = project(&server, &path).await;

    let mut window = Window::open(&server, &id).await;
    let text = std::fs::read_to_string(format!("{path}/src/main.rs")).unwrap();
    window
        .send(json!({ "type": "open", "path": "src/main.rs", "language": "rust", "version": 1, "text": text }))
        .await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_eq!(status["language"], "rust");
    assert_eq!(status["tools"][0]["id"], "fake-lsp");
    assert_eq!(status["root"]["marker"], "Cargo.toml", "{status}");
    let diagnostics = window
        .until(|f| f["type"] == "diagnostics" && f["path"] == "src/main.rs")
        .await;
    assert_eq!(diagnostics["source"], "fake-lsp");
    // The server sent them once it ran.
    assert_eq!(diagnostics["items"][0]["message"], "TODO found");

    // Hover carries the settings the server asked for.
    let hover = window
        .request(
            "src/main.rs",
            "textDocument/hover",
            json!({ "position": { "line": 0, "character": 4 } }),
        )
        .await;
    let markdown = hover["result"]["contents"]["value"].as_str().unwrap();
    assert!(
        markdown.contains("main") && markdown.contains("hello"),
        "{hover}"
    );

    // A definition outside the project is an external file, granted.
    let definition = window
        .request(
            "src/main.rs",
            "textDocument/definition",
            json!({ "position": { "line": 0, "character": 14 } }),
        )
        .await;
    let place = definition["result"][0]["uri"].as_str().unwrap().to_owned();
    let token = place.strip_prefix("external:").expect("an external file");
    assert_eq!(definition["externals"][token]["name"], "lib.rs");
    let external = window
        .request("", "ahab/externalFile", json!({ "ref": token }))
        .await;
    assert_eq!(external["result"]["text"], "pub fn outside() {}\n");
    let refused = window
        .request("", "ahab/externalFile", json!({ "ref": "never-granted" }))
        .await;
    assert_eq!(refused["error"]["code"], "not_granted");

    // Edits reach the server; formatting comes back as edits.
    window
        .send(json!({ "type": "change", "path": "src/main.rs", "version": 2, "changes": [
            { "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 0 } }, "text": "ERROR " }
        ]}))
        .await;
    let changed = window
        .until(|f| {
            f["type"] == "diagnostics"
                && f["items"].as_array().is_some_and(|items| items.len() == 2)
        })
        .await;
    assert_eq!(changed["items"][1]["severity"], 1);
    let formatted = window
        .request(
            "src/main.rs",
            "ahab/format",
            json!({ "options": { "tabSize": 4, "insertSpaces": true } }),
        )
        .await;
    assert_eq!(formatted["result"]["tool"], "fake-lsp", "{formatted}");
    assert_eq!(
        formatted["result"]["edits"][0]["range"]["start"]["character"],
        24
    );

    // The server keeps the latest problems for other clients.
    for _ in 0..100 {
        let (_, problems) = server.get(&format!("/projects/{id}/problems")).await;
        if problems["counts"]["errors"] == 1 {
            assert_eq!(problems["items"][0]["source"], "fake-lsp");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (_, problems) = server
        .get(&format!("/projects/{id}/problems?severity=error"))
        .await;
    assert_eq!(problems["items"].as_array().unwrap().len(), 1, "{problems}");

    // The machine lists it, with its log; it restarts.
    let (status, servers) = server
        .get(&format!("/machines/{}/language-servers", machine.id))
        .await;
    assert_eq!(status, StatusCode::OK, "{servers}");
    let entry = &servers["items"][0];
    assert_eq!(entry["tool"], "fake-lsp");
    assert_eq!(entry["state"], "running");
    let server_id = entry["id"].as_str().unwrap().to_owned();
    let (_, log) = server
        .get(&format!(
            "/machines/{}/language-servers/{server_id}/log",
            machine.id
        ))
        .await;
    assert!(
        log["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().contains("running")),
        "{log}"
    );
    let (status, _) = server
        .post(
            &format!(
                "/machines/{}/language-servers/{server_id}/restart",
                machine.id
            ),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let hover = window
        .request(
            "src/main.rs",
            "textDocument/hover",
            json!({ "position": { "line": 0, "character": 4 } }),
        )
        .await;
    assert!(
        hover["result"]["contents"]["value"].as_str().is_some(),
        "{hover}"
    );
}

#[tokio::test]
async fn a_language_with_no_tool_found_says_why() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let (_dir, path) = folder();
    server
        .put(
            &format!("/machines/{}/tool-settings", machine.id),
            json!({
                "records": [{ "id": "nowhere-ls", "name": "Nowhere", "kind": "languageServer",
                    "languages": ["rust"], "run": { "command": ["ahab-test-no-such-server"] } }],
                "languages": { "rust": { "tools": ["nowhere-ls"] } }
            }),
        )
        .await;
    let (_, id) = project(&server, &path).await;
    let mut window = Window::open(&server, &id).await;
    window
        .send(json!({ "type": "open", "path": "src/main.rs", "language": "rust", "version": 1, "text": "fn main() {}\n" }))
        .await;
    let status = window.until(|f| f["type"] == "status").await;
    assert_eq!(status["tools"][0]["state"], "missing");
    let answer = window
        .request(
            "src/main.rs",
            "textDocument/definition",
            json!({ "position": { "line": 0, "character": 3 } }),
        )
        .await;
    assert_eq!(answer["error"]["code"], "no_server");
}

#[tokio::test]
async fn an_older_machine_has_no_language_tools() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([12; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    server.online(&machine).await;
    let (status, body) = server.get(&format!("/machines/{}/tools", machine.id)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "machine_outdated");
    let (_, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": "/work/app" }),
        )
        .await;
    let id = project["id"].as_str().unwrap();
    let url = format!(
        "{}/api/v1/projects/{id}/lsp",
        server.url.replace("http", "ws")
    );
    match server.ws(&url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }
        other => panic!("expected machine_outdated, got {other:?}"),
    }
    assert!(daemon.asked_for("lsp.connect").is_empty());
    assert!(daemon.asked_for("tools.status").is_empty());
}
