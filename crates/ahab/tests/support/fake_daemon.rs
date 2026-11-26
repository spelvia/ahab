//! A daemon that only speaks the protocol: it connects like a real one,
//! answers requests with fixed replies, records what it was asked, and sends
//! the events a test gives it. For server behaviour without real agents.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use ahab::modules::machines::MachineKey;
use ahab::modules::machines::presence::api::{Envelope, Handshake, WireError, connect_message};

use super::TestServer;

pub struct FakeDaemon {
    /// Every request method it received, in order, with its params.
    pub asked: Arc<Mutex<Vec<(String, Value)>>>,
    /// Every acknowledgement it received.
    pub acks: Arc<Mutex<Vec<u64>>>,
    /// The token its welcome gave it for its plugins' API calls.
    pub api_token: Option<String>,
    events: mpsc::UnboundedSender<Envelope>,
    withheld: Arc<Mutex<HashSet<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeDaemon {
    /// Connects with `key`, whose machine must exist and be active, as a
    /// daemon from before capabilities: its hello lists none.
    pub async fn connect(server: &TestServer, key: &MachineKey) -> Self {
        Self::connect_with(server, key, None).await
    }

    /// Connects with a hello that lists `capabilities` (none when `None`).
    pub async fn connect_with(
        server: &TestServer,
        key: &MachineKey,
        capabilities: Option<&[&str]>,
    ) -> Self {
        let mut hello = json!({
            "type": "hello",
            "protocol": 1,
            "facts": { "os": "linux", "arch": "x86_64", "hostname": "atlas", "version": "0.1.0" },
            "challenge": "test",
        });
        if let Some(capabilities) = capabilities {
            hello["capabilities"] = json!(capabilities);
        }
        Self::connect_hello(server, key, hello).await
    }

    /// Connects with `hello` as written (a recorded one, say), with its
    /// `publicKey` and `signature` filled in for `key` and this challenge.
    /// Methods starting with `gone.` answer `unknown_method`, like a daemon
    /// whose capabilities drifted from what it handles; `chats.decide` for
    /// request `refused` answers `invalid_decision`, like an agent would.
    pub async fn connect_hello(server: &TestServer, key: &MachineKey, mut hello: Value) -> Self {
        let url = server.url.replace("http://", "ws://") + "/api/v1/daemon/connect";
        let host = server.url.trim_start_matches("http://").to_owned();
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let Some(Ok(Message::Text(text))) = socket.next().await else {
            panic!("no challenge");
        };
        let Ok(Handshake::Challenge { nonce }) = serde_json::from_str(&text) else {
            panic!("not a challenge");
        };
        hello["publicKey"] = json!(STANDARD.encode(key.public_key().as_bytes()));
        hello["signature"] = json!(STANDARD.encode(key.sign(&connect_message(&host, &nonce))));
        socket
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        let Some(Ok(Message::Text(welcome))) = socket.next().await else {
            panic!("not welcomed");
        };
        let Ok(Handshake::Welcome { api_token, .. }) = serde_json::from_str(&welcome) else {
            panic!("not a welcome: {welcome}");
        };

        let asked = Arc::new(Mutex::new(Vec::new()));
        let acks = Arc::new(Mutex::new(Vec::new()));
        let withheld = Arc::new(Mutex::new(HashSet::new()));
        let withheld_in = Arc::clone(&withheld);
        // The chats it says it runs (`chats.live`): none, like a
        // daemon that just started.
        let live = Arc::new(Mutex::new(Vec::<String>::new()));
        let (events, mut outgoing) = mpsc::unbounded_channel::<Envelope>();
        let (asked_in, acks_in, live_in) =
            (Arc::clone(&asked), Arc::clone(&acks), Arc::clone(&live));
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(envelope) = outgoing.recv() => {
                        let text = serde_json::to_string(&envelope).unwrap();
                        if socket.send(Message::Text(text.into())).await.is_err() { return; }
                    }
                    incoming = socket.next() => match incoming {
                        Some(Ok(Message::Text(text))) => match serde_json::from_str::<Envelope>(&text) {
                            Ok(Envelope::Request { id, method, params, .. }) => {
                                asked_in.lock().unwrap().push((method.clone(), params.clone()));
                                if withheld_in.lock().unwrap().contains(&method) {
                                    continue;
                                }
                                let answer = if method == "chats.decide" && params["request"] == "refused" {
                                    // The agent finds the decision does not answer it.
                                    let error = WireError::new("invalid_decision", "the request offers no option \"x\"");
                                    Envelope::Response { id, result: None, error: Some(error) }
                                } else if method.starts_with("gone.") {
                                    let error = WireError::new("unknown_method", format!("This daemon has no method {method}."));
                                    Envelope::Response { id, result: None, error: Some(error) }
                                } else {
                                    let result = reply(&method, &params, &live_in.lock().unwrap());
                                    Envelope::Response { id, result: Some(result), error: None }
                                };
                                let text = serde_json::to_string(&answer).unwrap();
                                if socket.send(Message::Text(text.into())).await.is_err() { return; }
                            }
                            Ok(Envelope::Ack { seq }) => acks_in.lock().unwrap().push(seq),
                            _ => {}
                        },
                        Some(Ok(Message::Ping(data))) => { let _ = socket.send(Message::Pong(data)).await; }
                        Some(Ok(_)) => {}
                        _ => return,
                    }
                }
            }
        });
        Self {
            asked,
            acks,
            api_token,
            events,
            withheld,
            task,
        }
    }

    /// Sends a session event: `seq` is the daemon's number (0 for live-only
    /// events), `session_seq` the session's (none for streamed text).
    pub fn send(&self, seq: u64, session: &str, session_seq: Option<i64>, event: Value) {
        self.events
            .send(Envelope::Event {
                seq,
                kind: "chat.event".into(),
                data: json!({ "chat": session, "seq": session_seq, "event": event }),
            })
            .unwrap();
    }

    /// Sends any event, such as `terminal.event`.
    pub fn emit(&self, seq: u64, kind: &str, data: Value) {
        self.events
            .send(Envelope::Event {
                seq,
                kind: kind.into(),
                data,
            })
            .unwrap();
    }

    /// Whether it was asked `method`, and with what.
    pub fn asked_for(&self, method: &str) -> Vec<Value> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|(asked, _)| asked == method)
            .map(|(_, params)| params.clone())
            .collect()
    }

    pub fn disconnect(self) {
        self.task.abort();
    }

    pub fn withhold(&self, method: &str) {
        self.withheld.lock().unwrap().insert(method.to_owned());
    }
}

fn reply(method: &str, params: &Value, live: &[String]) -> Value {
    match method {
        "fs.checkFolder" => json!({
            "exists": true, "isDir": true, "readable": true, "writable": true,
            "realPath": params["path"],
        }),
        "projects.sync" => json!({ "missing": [] }),
        // Claude Code, and every added agent, ready.
        "agents.detect" => {
            let mut agents = vec![json!({
                "agent": "claude-code", "state": "ready", "version": "2.1.322",
                "path": "/bin/claude", "account": null, "detail": null,
            })];
            for added in params["added"].as_array().into_iter().flatten() {
                agents.push(json!({
                    "agent": added["agent"], "label": added["name"], "state": "ready",
                    "version": null, "path": null, "account": null, "detail": null,
                }));
            }
            json!({ "agents": agents })
        }
        "agents.usage" => json!({ "items": [{
            "agent": "claude-code", "label": "Claude Code", "status": "available",
            "plan": "Max 5x", "problem": null, "fetchedAt": "2026-11-16T16:00:00Z",
            "windows": [{
                "id": "session", "label": "Session (5 hours)", "shortLabel": "5h",
                "usedPercent": 42.0, "resetsAt": "2026-11-16T18:00:00Z",
            }],
        }] }),
        "machine.metrics" => json!({
            "sampledAt": "2026-11-16T16:00:00Z", "hostName": "atlas", "osVersion": null,
            "kernel": null, "uptimeSeconds": 3600,
            "cpu": { "usagePercent": 12.5, "cores": [10.0, 15.0], "physicalCores": 1, "load": [0.5, 0.4, 0.3] },
            "memory": { "total": 100, "used": 40, "available": 60, "swapTotal": 0, "swapUsed": 0 },
            "disks": [{ "mount": "/", "fileSystem": "ext4", "total": 100, "available": 50 }],
            "processCount": 1,
            "processes": [{ "pid": 1, "name": "ahab", "script": null, "cpuPercent": 1.0,
                "memoryBytes": 10, "startedAt": null, "ahab": true, "role": "ahab", "parent": null,
                "user": "me", "status": "Sleep", "threads": 4, "cpuTimeMs": 1000, "runSeconds": 60,
                "virtualBytes": 100, "readPerSecond": 0, "writtenPerSecond": 0,
                "executable": "/bin/ahab", "cwd": "/" }],
            "apps": [{ "name": "ahab", "processes": 1, "cpuPercent": 1.0, "memoryBytes": 10,
                "members": [1] }],
            "ahab": [{ "kind": "ahab", "id": null, "count": 1, "processes": 1,
                "cpuPercent": 1.0, "memoryBytes": 10 }],
            "history": [],
        }),
        "agents.updates" => json!({ "items": [{
            "agent": "codex", "label": "Codex", "installed": "0.1.0", "latest": "0.2.0",
            "channel": "latest", "available": true, "method": "npm", "canUpdate": true,
            "command": "codex update", "reason": null, "checkedAt": "2026-11-16T16:00:00Z",
        }] }),
        "agents.update" => json!({
            "agent": params["agent"], "before": "0.1.0", "after": "0.2.0",
            "success": true, "output": "updated",
        }),
        "chats.fork" => json!(uuid::Uuid::now_v7()),
        // An added agent drops its `retired` mode: resumed in it, it carries
        // on in `ask`.
        "chats.resume" if params["mode"] == "retired" => json!({
            "nativeId": "native-1", "model": "test-model", "unrestricted": false,
            "modeUnavailable": "retired", "mode": "ask",
        }),
        "chats.start" | "chats.resume" => {
            json!({ "nativeId": "native-1", "model": "test-model", "unrestricted": false })
        }
        "chats.stopWork" => json!({"disposition":"accepted","message":null}),
        "chats.modes" => json!({"current":"ask","available":[
            {"id":"ask","name":"Ask","description":null},
            {"id":"retired","name":"Retired","description":null},
        ]}),
        "chats.live" => json!({ "chats": live }),
        "attachments.put" => json!({
            "id": "01a1db75-0000-7000-8000-000000000001", "name": params["name"],
            "path": format!("/data/attachments/1/{}", params["name"].as_str().unwrap_or_default()),
            "contentType": "image/png", "size": 3,
        }),
        // "png", base64-encoded.
        "attachments.get" => json!({ "data": "cG5n" }),
        // Every file is at version `v-now`.
        "fs.read" => json!({
            "path": params["path"], "size": 1, "binary": false, "content": "x",
            "version": "v-now",
        }),
        _ => json!({}),
    }
}
