//! Language tooling: language servers through a window's
//! channel, Problems and checks, installing tools, settings.

use std::path::{Path, PathBuf};

use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use ahab::daemon::{self, DaemonConfig};
use ahab::modules::machines::presence::api::MachineFacts;
use ahab::modules::machines::{Machine, MachineKey};
use ahab::modules::tools::registry::daemon::ToolOptions;

use crate::support::TestServer;

mod install;
mod lsp;
mod problems;
mod repositories;

/// The server's own machine with its daemon; GitHub releases come from
/// `github` when given.
pub async fn local_daemon(
    server: &TestServer,
    github: Option<String>,
) -> (Machine, watch::Sender<bool>) {
    let key = MachineKey::from_secret([21; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let (stop, stopped) = watch::channel(false);
    let config = DaemonConfig {
        server: server.url.clone(),
        key,
        facts: MachineFacts {
            os: "linux".into(),
            arch: "x86_64".into(),
            hostname: "atlas".into(),
            version: "0.1.0".into(),
        },
        server_fingerprint: None,
        data_dir: server.daemon_dir(),
        executable: Some(env!("CARGO_BIN_EXE_ahab").into()),
        plugin_node: None,
        tools: ToolOptions {
            login_shell: false,
            github: github.unwrap_or_else(|| "http://127.0.0.1:9".to_owned()),
        },
    };
    tokio::spawn(daemon::run(config, stopped));
    server.online(&machine).await;
    (machine, stop)
}

/// A Rust-looking project folder.
pub fn folder() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(
        app.join("src/main.rs"),
        "fn main() { outside(); }   \n// TODO: more\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn outside() {}\n").unwrap();
    let path = std::fs::canonicalize(&app).unwrap().display().to_string();
    (dir, path)
}

/// A project `app` with a project at `path` on `atlas`, once served: the
/// project's and the project's IDs.
pub async fn project(server: &TestServer, path: &str) -> (String, String) {
    let (status, project) = server
        .post(
            "/projects",
            json!({ "name": "app", "machine": "atlas", "path": path }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    let id = project["id"].as_str().unwrap().to_owned();
    for _ in 0..100 {
        if server.get(&format!("/projects/{id}/files")).await.0 == StatusCode::OK {
            return (project["id"].as_str().unwrap().to_owned(), id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the project was never served");
}

/// The fake language server, as a machine's own tool for Rust.
pub fn fake_lsp(outside: &Path) -> Value {
    let script: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tools/fake-lsp.mjs");
    json!({
        "id": "fake-lsp", "name": "Fake LSP", "kind": "languageServer", "languages": ["rust"],
        "run": {
            "command": ["node", script],
            "rootMarkers": ["Cargo.toml"],
            "settings": { "fake": { "greeting": "hello" } },
            "env": { "FAKE_OUTSIDE": outside }
        }
    })
}

/// A window's channel to a project's language servers.
pub struct Window {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next: i64,
}

impl Window {
    pub async fn open(server: &TestServer, project: &str) -> Self {
        let url = format!(
            "{}/api/v1/projects/{project}/lsp",
            server.url.replace("http", "ws")
        );
        let socket = server.ws(&url).await.unwrap();
        Self { socket, next: 1 }
    }

    pub async fn send(&mut self, frame: Value) {
        self.socket
            .send(Message::Text(frame.to_string().into()))
            .await
            .unwrap();
    }

    /// The next frame `wanted` accepts, within ten seconds.
    pub async fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        let read = async {
            loop {
                match self.socket.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        if wanted(&frame) {
                            return frame;
                        }
                    }
                    Some(Ok(_)) => {}
                    other => panic!("the channel ended: {other:?}"),
                }
            }
        };
        let Ok(frame) = tokio::time::timeout(std::time::Duration::from_secs(10), read).await else {
            panic!("no such frame within ten seconds");
        };
        frame
    }

    /// Asks the language servers; their answer.
    pub async fn request(&mut self, path: &str, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "type": "request", "id": id, "path": path, "method": method, "params": params }))
            .await;
        self.until(|frame| frame["type"] == "response" && frame["id"] == id)
            .await
    }
}
