//! The plugin protocol's framing (as the SDK's `framing.ts`), the messages
//! between server and daemon about plugins, and how packages travel.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// One message on a plugin's pipes: JSON-RPC, or bytes for a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: Option<Uuid>,
    pub body: Vec<u8>,
}

/// Headers longer than this are not a frame: the stream is broken.
const MAX_HEADER: usize = 1024;

/// Encodes a frame: `Content-Length`, `Ahab-Channel` for channel bytes, a
/// blank line, the body.
pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n", frame.body.len());
    if let Some(channel) = frame.channel {
        out.push_str(&format!("Ahab-Channel: {channel}\r\n"));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(&frame.body);
    bytes
}

/// Reads frames from chunks as they arrive.
#[derive(Debug, Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    /// Adds a chunk; answers the frames it completed, or why the stream is
    /// broken.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, String> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            let Some(end) = self.buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
                if self.buffer.len() > MAX_HEADER {
                    return Err("a frame's headers are too long".to_owned());
                }
                return Ok(frames);
            };
            let headers = String::from_utf8_lossy(&self.buffer[..end]).into_owned();
            let mut length = None;
            let mut channel = None;
            for line in headers.split("\r\n") {
                let Some((name, value)) = line.split_once(':') else {
                    continue;
                };
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse::<usize>().ok(),
                    "ahab-channel" => channel = value.trim().parse::<Uuid>().ok(),
                    _ => {}
                }
            }
            let Some(length) = length else {
                return Err("a frame has no valid Content-Length".to_owned());
            };
            let start = end + 4;
            if self.buffer.len() < start + length {
                return Ok(frames);
            }
            let body = self.buffer[start..start + length].to_vec();
            self.buffer.drain(..start + length);
            frames.push(Frame { channel, body });
        }
    }
}

/// A plugin a machine should run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wanted {
    pub id: String,
    /// Which build.
    pub digest: String,
    /// The daemon entry, relative to the package; `None` when this machine
    /// cannot run it (a Rust entry, until those come).
    pub entry: Option<String>,
    /// The settings in effect on this machine.
    #[serde(default)]
    pub settings: Value,
    /// The hooks its manifest lists: the only ones it is asked about.
    /// `None` from a server that does not say, when every hook the plugin
    /// declares is asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<String>>,
}

/// `plugins.sync`: everything the machine should run now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    /// The machine's name, for the plugins.
    pub machine: String,
    pub plugins: Vec<Wanted>,
}

/// A build a machine does not have yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub id: String,
    pub digest: String,
}

/// The answer to `plugins.sync`: the builds to send, and how every plugin
/// is doing (a server that just connected has missed the live reports).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAnswer {
    #[serde(default)]
    pub missing: Vec<Missing>,
    #[serde(default)]
    pub states: Vec<StateReport>,
}

/// `plugins.receive`: a build comes on `channel`, one file per frame (see
/// [`encode_file`]); the channel closes after the last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveRequest {
    pub id: String,
    pub digest: String,
    pub channel: Uuid,
}

/// `plugins.command`: run a plugin's command on this machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub plugin: String,
    /// Its name in the plugin (`containers.list`).
    pub name: String,
    #[serde(default)]
    pub input: Value,
    /// The project it runs for, when one: the plugin gets its folder.
    #[serde(default)]
    pub project: Option<Uuid>,
    /// The plugin's settings in effect for it.
    #[serde(default)]
    pub settings: Value,
}

/// `plugins.channel`: open a plugin's channel; the server's channel
/// `channel` carries it. Frames to the server start with a byte: [`DATA`]
/// then bytes, or [`END`] then the reason (UTF-8) when the plugin closed
/// it. Frames from the server are bytes for the plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRequest {
    pub plugin: String,
    pub name: String,
    #[serde(default)]
    pub params: Value,
    pub channel: Uuid,
}

/// A channel frame's first byte: bytes from the plugin follow.
pub const DATA: u8 = 0;
/// A channel frame's first byte: the plugin closed it; the reason follows.
pub const END: u8 = 1;

/// A plugin by ID, as in `plugins.logs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRef {
    pub plugin: String,
}

/// How a plugin's daemon entry is doing on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    /// Its build is on the way to the machine.
    Receiving,
    Starting,
    Running,
    /// It crashed and will be started again shortly.
    Restarting,
    /// It crashed too often; it stays off until it changes or is enabled
    /// again.
    Failed,
    /// Stopped (disabled, removed, or not for this machine).
    Stopped,
    /// It cannot run here (no Node, a Rust entry); also a state from a
    /// newer daemon this server does not know (last, for `serde(other)`).
    #[serde(other)]
    Unavailable,
}

/// `plugin.state`, a live event from the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateReport {
    pub plugin: String,
    pub digest: String,
    pub state: RunState,
    /// Why, for `restarting`, `failed` and `unavailable`.
    pub message: Option<String>,
    /// What the entry declared when it started: commands, channels, hooks,
    /// events (the `initialize` answer).
    #[serde(default)]
    pub declared: Option<Value>,
}

/// One file of a build, as a channel frame: the path's length (4 bytes, big
/// endian), the path with `/` between its parts, then the contents.
pub fn encode_file(path: &str, contents: &[u8]) -> Vec<u8> {
    let path = path.as_bytes();
    let length = u32::try_from(path.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(4 + path.len() + contents.len());
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(path);
    out.extend_from_slice(contents);
    out
}

/// A file from [`encode_file`]; `None` when the frame is not one.
pub fn decode_file(frame: &[u8]) -> Option<(String, Vec<u8>)> {
    let length = u32::from_be_bytes(frame.get(..4)?.try_into().ok()?) as usize;
    let path = std::str::from_utf8(frame.get(4..4 + length)?).ok()?;
    Some((path.to_owned(), frame[4 + length..].to_vec()))
}

/// Whether a plugin's diagnostics collection may be called `name`:
/// lower-case letters, digits and dashes, at most 64.
pub fn valid_collection(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The source a plugin's collection's problems show as: `<plugin>.<collection>`
/// (`ahab-gradle.gradle`), so no plugin's can replace a tool's or another's.
pub fn collection_source(plugin: &str, collection: &str) -> String {
    format!("{plugin}.{collection}")
}

/// A path a plugin gave, relative to the project's `folder` (a real
/// path): relative ones as they are, absolute ones inside the folder made
/// relative; `None` for any that could leave it.
pub fn confined(path: &str, folder: &std::path::Path) -> Option<String> {
    let relative = if path.starts_with('/') {
        std::path::Path::new(path)
            .strip_prefix(folder)
            .ok()?
            .to_str()?
            .to_owned()
    } else {
        path.to_owned()
    };
    let parts: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let bad = parts.is_empty()
        || parts
            .iter()
            .any(|part| *part == ".." || part.contains('\\'))
        || relative.contains('\0');
    (!bad).then(|| parts.join("/"))
}

/// A plugin's Diagnostic in LSP's form, as every other source gives it:
/// `severity` as a word (`error`, `warning`, `information`, `hint`) or
/// LSP's number, `code`, `source`, `message`, `range` (from 0), and
/// `fixes` (edits) kept in `data`. `None` when it is not one.
pub fn lsp_diagnostic(value: &Value) -> Option<Value> {
    let message = value.get("message")?.as_str()?;
    let range = value.get("range")?;
    let position = |at: &Value| -> Option<Value> {
        Some(serde_json::json!({
            "line": at.get("line")?.as_u64()?,
            "character": at.get("character").and_then(Value::as_u64).unwrap_or(0),
        }))
    };
    let start = position(range.get("start")?)?;
    let end = range
        .get("end")
        .and_then(position)
        .unwrap_or_else(|| start.clone());
    let severity = match value.get("severity") {
        Some(Value::Number(n)) => n.as_u64().filter(|n| (1..=4).contains(n)).unwrap_or(1),
        Some(Value::String(word)) => match word.as_str() {
            "warning" => 2,
            "information" | "info" => 3,
            "hint" => 4,
            _ => 1,
        },
        _ => 1,
    };
    let mut out = serde_json::json!({
        "range": { "start": start, "end": end },
        "severity": severity,
        "message": message,
    });
    if let Some(code) = value.get("code").filter(|c| c.is_string() || c.is_number()) {
        out["code"] = code.clone();
    }
    if let Some(fixes) = value.get("fixes").filter(|f| f.is_array()) {
        out["data"] = serde_json::json!({ "fixes": fixes });
    }
    Some(out)
}

#[cfg(test)]
mod diagnostics {
    use super::*;

    #[test]
    fn paths_stay_inside_the_project() {
        let folder = std::path::Path::new("/w/app");
        assert_eq!(
            confined("src/Main.java", folder).as_deref(),
            Some("src/Main.java")
        );
        assert_eq!(
            confined("./src//A.java", folder).as_deref(),
            Some("src/A.java")
        );
        assert_eq!(
            confined("/w/app/build.gradle", folder).as_deref(),
            Some("build.gradle")
        );
        assert_eq!(confined("/w/other/x", folder), None);
        assert_eq!(confined("../secrets", folder), None);
        assert_eq!(confined("src/../../x", folder), None);
        assert_eq!(confined("", folder), None);
        assert!(
            valid_collection("gradle") && !valid_collection("Gradle") && !valid_collection("a.b")
        );
        assert_eq!(
            collection_source("ahab-gradle", "gradle"),
            "ahab-gradle.gradle"
        );
    }

    #[test]
    fn plugin_diagnostics_take_lsp_form() {
        let given = serde_json::json!({
            "range": { "start": { "line": 3, "character": 2 }, "end": { "line": 3, "character": 9 } },
            "severity": "warning", "message": "deprecated", "code": "deprecation",
            "fixes": [{ "title": "Use the new one", "edits": [] }]
        });
        let lsp = lsp_diagnostic(&given).unwrap();
        assert_eq!(lsp["severity"], 2);
        assert_eq!(lsp["range"]["end"]["character"], 9);
        assert_eq!(lsp["code"], "deprecation");
        assert_eq!(lsp["data"]["fixes"][0]["title"], "Use the new one");
        let short = lsp_diagnostic(
            &serde_json::json!({ "range": { "start": { "line": 0 } }, "message": "x" }),
        )
        .unwrap();
        assert_eq!(
            short["range"]["end"],
            serde_json::json!({ "line": 0, "character": 0 })
        );
        assert_eq!(short["severity"], 1);
        assert!(lsp_diagnostic(&serde_json::json!({ "message": "no range" })).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_from_a_newer_daemon_reads_as_unavailable() {
        let state: RunState = serde_json::from_str(r#""hibernating""#).unwrap();
        assert_eq!(state, RunState::Unavailable);
        let answer: SyncAnswer = serde_json::from_str("{}").unwrap();
        assert_eq!(answer, SyncAnswer::default());
    }

    #[test]
    fn frames_are_read_however_they_are_split() {
        let channel = Uuid::now_v7();
        let one = encode(&Frame {
            channel: None,
            body: br#"{"a":1}"#.to_vec(),
        });
        let two = encode(&Frame {
            channel: Some(channel),
            body: vec![0, 1, 2],
        });
        let all = [one, two].concat();
        let mut decoder = Decoder::default();
        let mut frames = Vec::new();
        for chunk in all.chunks(3) {
            frames.extend(decoder.push(chunk).unwrap());
        }
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].body, br#"{"a":1}"#);
        assert_eq!(frames[1].channel, Some(channel));
        assert!(Decoder::default().push(b"Nope: 1\r\n\r\n").is_err());
    }

    #[test]
    fn files_travel_with_their_paths() {
        let frame = encode_file("dist/daemon.js", b"export {};");
        assert_eq!(
            decode_file(&frame),
            Some(("dist/daemon.js".to_owned(), b"export {};".to_vec()))
        );
        assert_eq!(decode_file(&[0, 0]), None);
    }
}
