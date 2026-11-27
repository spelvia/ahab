//! The daemon connection's messages. The server and the daemon both
//! use these types, as JSON text frames on the WebSocket.
//!
//! # Compatibility rules
//!
//! A newer server works with every released daemon, and a newer daemon with
//! an older server. So, within one protocol version:
//!
//! - **Additive only.** New fields are optional (`#[serde(default)]`) and
//!   ignored by peers that do not know them; no type denies unknown fields;
//!   an enum a newer peer may extend has a fallback (`#[serde(other)]`).
//!   Fields, methods and their meaning are never removed or changed.
//! - **New behaviour is a new method name.** A method whose params or answer
//!   must change incompatibly gets a new name (`fs.list2`, `chats.startV2`);
//!   the old one keeps working for older servers.
//! - **Capabilities decide what the server may call.** The daemon's hello
//!   lists the methods it handles; the server never sends a request for one
//!   that is not listed and answers its own caller `machine_outdated`
//!   instead. A daemon that lists nothing is a protocol-1 daemon from before
//!   capabilities, which handles [`BASELINE_V1`].
//! - **[`PROTOCOL`] is raised only for a change older peers cannot handle
//!   at all**, and [`OLDEST_PROTOCOL`] only with a major release.

use crate::shared::error::DaemonError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The protocol version this build speaks; raised only for changes older
/// daemons cannot handle (adding fields or methods does not raise it).
pub const PROTOCOL: u32 = 1;
/// The oldest protocol version this server still accepts. Never raised
/// without a major release: raising it locks out every daemon that has not
/// updated.
pub const OLDEST_PROTOCOL: u32 = 1;

/// The methods every protocol-1 daemon handles, frozen as they were when
/// daemons began to list their capabilities. The server assumes them for a
/// daemon whose hello lists none; a daemon keeps handling them for as long
/// as it speaks protocol 1. Never edit this list: new methods are announced
/// in the hello's `capabilities`.
pub const BASELINE_V1: &[&str] = &[
    "ping",
    "agents.detect",
    "agents.updates",
    "agents.update",
    "agents.usage",
    "agents.catalog",
    "machine.metrics",
    "projects.sync",
    "fs.home",
    "fs.listDirectories",
    "fs.checkFolder",
    "fs.list",
    "fs.read",
    "files.index",
    "git.run",
    "git.watch",
    "search.run",
    "search.stream",
    "plugins.sync",
    "plugins.receive",
    "plugins.command",
    "plugins.channel",
    "plugins.deliver",
    "plugins.logs",
    "attachments.put",
    "attachments.get",
    "terminals.create",
    "terminals.list",
    "terminals.kill",
    "terminals.restart",
    "terminals.remove",
    "terminals.attach",
    "chats.start",
    "chats.resume",
    "chats.mode",
    "chats.send",
    "chats.interrupt",
    "chats.respond",
    "chats.close",
    "chats.live",
];

/// Why the server closed a daemon connection.
pub mod close {
    /// Unknown key or bad signature: join again.
    pub const UNKNOWN_KEY: u16 = 4401;
    /// The machine waits for approval.
    pub const PENDING: u16 = 4403;
    /// A newer connection for this machine replaced this one.
    pub const REPLACED: u16 = 4409;
    /// The daemon's protocol is too old: update Ahab.
    pub const OUTDATED: u16 = 4426;
    /// The daemon broke the protocol.
    pub const PROTOCOL_ERROR: u16 = 1002;
}

/// The handshake: challenge, hello, welcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Handshake {
    /// Server → daemon: 32 random bytes to sign.
    Challenge { nonce: String },
    /// Daemon → server: who it is, proven by signing the challenge, and a
    /// challenge of its own for the server to sign.
    #[serde(rename_all = "camelCase")]
    Hello {
        protocol: u32,
        public_key: String,
        signature: String,
        facts: MachineFacts,
        challenge: String,
        /// The methods this daemon handles. Absent from daemons older than
        /// capabilities, which handle [`BASELINE_V1`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capabilities: Option<Vec<String>>,
    },
    /// Server → daemon: accepted, at this protocol version, with the server's
    /// key and its signature of the daemon's challenge, so the daemon knows
    /// which server it reached.
    #[serde(rename_all = "camelCase")]
    Welcome {
        machine_id: Uuid,
        name: String,
        protocol: u32,
        server_key: String,
        server_signature: String,
        /// A token for the daemon's plugins to call the server's API with
        /// (`Authorization: Bearer`), valid while this connection lasts. Absent
        /// from servers older than client pairing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_token: Option<String>,
    },
}

/// What the daemon reports about its computer. Every field may be missing
/// from another version's hello.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MachineFacts {
    pub os: String,
    pub arch: String,
    pub hostname: String,
    pub version: String,
}

/// What the daemon signs: a fixed context, the host it connected to (as in
/// its `Host` header) and the challenge, so the signature is useless
/// anywhere else.
pub fn connect_message(host: &str, nonce: &str) -> Vec<u8> {
    format!("ahab/daemon-connect/v1\n{host}\n{nonce}").into_bytes()
}

/// What the server signs to prove its key: the daemon's challenge, bound to
/// the host the daemon connected to.
pub fn server_proof_message(host: &str, challenge: &str) -> Vec<u8> {
    format!("ahab/server-proof/v1\n{host}\n{challenge}").into_bytes()
}

/// Every message after the handshake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Envelope {
    /// Ask the other side to do something.
    #[serde(rename_all = "camelCase")]
    Request {
        id: String,
        method: String,
        #[serde(default)]
        params: Value,
        timeout_ms: u64,
    },
    /// The single answer to a request.
    Response {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<WireError>,
    },
    /// One item of a streamed answer.
    Stream { id: String, item: Value },
    /// A stream finished.
    End { id: String, status: StreamStatus },
    /// Stop a request or stream that is still running.
    Cancel { id: String },
    /// Something happened on the machine (daemon → server), numbered.
    Event {
        seq: u64,
        #[serde(rename = "type")]
        kind: String,
        data: Value,
    },
    /// Every event up to `seq` is stored (server → daemon).
    Ack { seq: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamStatus {
    Completed,
    Cancelled,
    Failed,
    Truncated,
    /// A status from a newer peer.
    #[serde(other)]
    Unknown,
}

/// An error in a response, with a stable code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

impl WireError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<DaemonError> for WireError {
    fn from(error: DaemonError) -> Self {
        Self::new(error.code, error.message)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn writes_the_handshake_and_envelopes_as_the_rfc_shows() {
        let hello = Handshake::Hello {
            protocol: 1,
            public_key: "k".into(),
            signature: "s".into(),
            facts: MachineFacts::default(),
            challenge: "c".into(),
            capabilities: None,
        };
        assert_eq!(
            serde_json::to_value(&hello).unwrap(),
            json!({ "type": "hello", "protocol": 1, "publicKey": "k", "signature": "s",
                    "facts": { "os": "", "arch": "", "hostname": "", "version": "" },
                    "challenge": "c" })
        );
        let request = Envelope::Request {
            id: "r-1".into(),
            method: "ping".into(),
            params: json!({}),
            timeout_ms: 1000,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "kind": "request", "id": "r-1", "method": "ping", "params": {}, "timeoutMs": 1000 })
        );
        let event: Envelope =
            serde_json::from_value(json!({ "kind": "event", "seq": 3, "type": "x", "data": null }))
                .unwrap();
        assert_eq!(
            event,
            Envelope::Event {
                seq: 3,
                kind: "x".into(),
                data: Value::Null
            }
        );
    }

    #[test]
    fn reads_a_hello_from_another_version() {
        // Older: no capabilities, facts missing a field.
        let old: Handshake = serde_json::from_value(json!({
            "type": "hello", "protocol": 1, "publicKey": "k", "signature": "s",
            "facts": { "os": "linux", "arch": "x86_64", "hostname": "h" },
            "challenge": "c" }))
        .unwrap();
        let Handshake::Hello {
            capabilities,
            facts,
            ..
        } = old
        else {
            panic!("not a hello");
        };
        assert_eq!(capabilities, None);
        assert_eq!(facts.version, "");
        // Newer: fields this build does not know.
        let new: Handshake = serde_json::from_value(json!({
            "type": "hello", "protocol": 1, "publicKey": "k", "signature": "s",
            "facts": { "os": "linux", "arch": "x86_64", "hostname": "h", "version": "9.0.0",
                       "kernel": "6.18" },
            "challenge": "c", "capabilities": ["ping", "fs.list2"], "features": { "x": 1 } }))
        .unwrap();
        assert!(
            matches!(new, Handshake::Hello { capabilities: Some(list), .. } if list.len() == 2)
        );
    }

    #[test]
    fn reads_an_unknown_stream_status() {
        let end: Envelope =
            serde_json::from_value(json!({ "kind": "end", "id": "r", "status": "paused" }))
                .unwrap();
        assert_eq!(
            end,
            Envelope::End {
                id: "r".into(),
                status: StreamStatus::Unknown
            }
        );
    }
}

/// Channels: streams of bytes between the server and a daemon,
/// carried as binary WebSocket frames beside the JSON envelopes. A channel
/// is opened by a request naming its ID; then either side sends data, and
/// either side closes it.
pub mod channel {
    use uuid::Uuid;

    /// Bytes for the channel's other end.
    pub const DATA: u8 = 0;
    /// The channel is closed; no more frames follow.
    pub const CLOSE: u8 = 1;

    /// A frame: its kind, the channel's ID, then the payload.
    pub fn encode(kind: u8, id: Uuid, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(17 + payload.len());
        frame.push(kind);
        frame.extend_from_slice(id.as_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    /// A frame's kind, channel and payload; `None` for a frame too short.
    pub fn decode(frame: &[u8]) -> Option<(u8, Uuid, &[u8])> {
        let (&kind, rest) = frame.split_first()?;
        let id = Uuid::from_slice(rest.get(..16)?).ok()?;
        Some((kind, id, &rest[16..]))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn frames_carry_their_channel_and_payload() {
            let id = Uuid::now_v7();
            let frame = encode(DATA, id, b"ls\r");
            assert_eq!(decode(&frame), Some((DATA, id, &b"ls\r"[..])));
            assert_eq!(decode(&encode(CLOSE, id, &[])), Some((CLOSE, id, &[][..])));
            assert_eq!(decode(&[0, 1, 2]), None);
        }
    }
}
