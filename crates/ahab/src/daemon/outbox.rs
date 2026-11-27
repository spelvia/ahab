//! Events the daemon sends its server. Lasting events are numbered and kept until the server
//! acknowledges them, so a dropped connection loses none: they are sent again
//! after the next connect. They are also written to a file in the data
//! folder, so a restart of the daemon loses none either. Live-only events
//! (streamed text) are sent while connected and otherwise dropped.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::modules::machines::presence::api::Envelope;
use crate::shared::sync::lock;

/// The most kept events take, in bytes as written; past it the oldest go,
/// and a `buffer.overflow` event says how many.
pub const LIMIT: usize = 100 * 1024 * 1024;

/// Acknowledged events after which the file is written again without them.
const COMPACT_AFTER: usize = 5_000;
const LIVE_BYTES: usize = 1024 * 1024;
const LIVE_COUNT: usize = 512;
const BATCH_BYTES: usize = 1024 * 1024;
const COMPACT_BYTES: u64 = 2 * LIMIT as u64;

/// The file in the data folder holding the kept events, one JSON per line.
const FILE: &str = "outbox.jsonl";

#[derive(Default)]
struct Inner {
    /// The last number given out.
    last: u64,
    /// Lasting events not acknowledged yet, oldest first, with their size.
    kept: VecDeque<Kept>,
    /// What `kept` takes, in bytes.
    bytes: usize,
    /// Events dropped for room since the server last heard of it.
    dropped: u64,
    /// The highest number sent on this connection.
    sent: u64,
    connected: bool,
    /// Live-only events waiting to be sent on this connection.
    live: VecDeque<(Envelope, usize)>,
    live_bytes: usize,
    /// Where kept events are written, when the outbox has a file.
    file: Option<Store>,
}

struct Kept {
    seq: u64,
    size: usize,
    body: Body,
}

enum Body {
    Disk(u64),
    Memory(Box<str>),
}

/// The kept events' file: appended to as events come, written again
/// without the acknowledged ones now and then.
struct Store {
    path: PathBuf,
    append: Option<File>,
    length: u64,
    /// Events acknowledged since the file was last written again.
    acknowledged: usize,
}

impl Store {
    fn open_append(path: &Path) -> Option<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .map_err(|error| tracing::warn!(path = %path.display(), %error, "cannot keep events on disk"))
            .ok()
    }

    fn append(&mut self, line: &str) -> Option<u64> {
        let file = self.append.as_mut()?;
        let offset = self.length;
        if let Err(error) = file.write_all(line.as_bytes()) {
            tracing::warn!(path = %self.path.display(), %error, "cannot write a kept event");
            // A partial line must never become part of the next append.
            if file.set_len(offset).is_err() {
                self.length = file.metadata().map_or(offset, |info| info.len());
                let _ = file.write_all(b"\n").map(|()| self.length += 1);
            }
            return None;
        }
        self.length += line.len() as u64;
        Some(offset)
    }

    fn read(&mut self, event: &Kept, buffer: &mut Vec<u8>) -> std::io::Result<()> {
        match &event.body {
            Body::Disk(offset) => {
                let file = self
                    .append
                    .as_mut()
                    .ok_or_else(|| std::io::Error::other("the kept events file is unavailable"))?;
                buffer.resize(event.size, 0);
                file.seek(SeekFrom::Start(*offset))?;
                file.read_exact(buffer)
            }
            Body::Memory(text) => {
                buffer.clear();
                buffer.extend_from_slice(text.as_bytes());
                Ok(())
            }
        }
    }

    /// Rewrites only the indexed events, copying one record at a time.
    fn rewrite(&mut self, kept: &mut VecDeque<Kept>) {
        let temporary = self.path.with_extension("jsonl.tmp");
        let written = (|| -> std::io::Result<File> {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .append(true)
                .open(&temporary)?;
            file.set_len(0)?;
            let mut buffer = Vec::new();
            for event in kept.iter() {
                self.read(event, &mut buffer)?;
                file.write_all(&buffer)?;
                if !buffer.ends_with(b"\n") {
                    file.write_all(b"\n")?;
                }
            }
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(file)
        })();
        match written {
            Ok(file) => {
                self.length = 0;
                for event in kept {
                    event.body = Body::Disk(self.length);
                    self.length += event.size as u64;
                }
                self.acknowledged = 0;
                self.append = Some(file);
            }
            Err(error) => {
                tracing::warn!(path = %self.path.display(), %error, "cannot rewrite the kept events");
            }
        }
    }
}

#[derive(Default)]
pub struct Outbox {
    inner: Mutex<Inner>,
    /// Wakes the connection when there is something to send.
    pub ready: Notify,
}

impl Inner {
    fn discard_through(&mut self, seq: u64) -> usize {
        let count = self.kept.partition_point(|event| event.seq <= seq);
        for event in self.kept.drain(..count) {
            self.bytes -= event.size;
        }
        if self.kept.is_empty() {
            self.kept = VecDeque::new();
        }
        count
    }

    fn trim_kept(&mut self) {
        let mut dropped = 0;
        while self.bytes > LIMIT && self.kept.len() > 1 {
            if let Some(event) = self.kept.pop_front() {
                self.bytes -= event.size;
                dropped += 1;
            }
        }
        if dropped > 0 {
            self.dropped += dropped;
            tracing::warn!(dropped, "too many events kept; dropped the oldest");
        }
    }
}

/// An event as a line of the file.
fn line(envelope: &Envelope) -> String {
    let mut text = serde_json::to_string(envelope).unwrap_or_default();
    text.push('\n');
    text
}

impl Outbox {
    /// An outbox keeping its events in `data_dir`, with those a previous run
    /// left unacknowledged: they are sent first, and numbering goes on after
    /// them.
    pub fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE);
        let mut inner = Inner::default();
        if let Ok(file) = File::open(&path) {
            let mut reader = BufReader::new(file);
            let mut offset = 0;
            let mut text = Vec::new();
            loop {
                text.clear();
                let Ok(size @ 1..) = reader.read_until(b'\n', &mut text) else {
                    break;
                };
                let start = offset;
                offset += size as u64;
                // The legacy file contains both events and acknowledgements.
                let Ok(envelope) = serde_json::from_slice::<Envelope>(&text) else {
                    continue;
                };
                match envelope {
                    Envelope::Ack { seq } => {
                        inner.last = inner.last.max(seq);
                        inner.discard_through(seq);
                    }
                    Envelope::Event { seq, .. } if seq > 0 => {
                        inner.last = inner.last.max(seq);
                        let (body, size) = if text.ends_with(b"\n") {
                            (Body::Disk(start), size)
                        } else {
                            let Ok(text) = String::from_utf8(text.clone()) else {
                                continue;
                            };
                            (Body::Memory((text + "\n").into_boxed_str()), size + 1)
                        };
                        inner.bytes += size;
                        inner.kept.push_back(Kept { seq, size, body });
                    }
                    _ => {}
                }
            }
        }
        inner.trim_kept();
        let _ = fs::create_dir_all(data_dir);
        let append = Store::open_append(&path);
        let length = append
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map_or(0, |info| info.len());
        let mut store = Store {
            path,
            append,
            length,
            acknowledged: 0,
        };
        // Written again at once: drops any line cut short.
        store.rewrite(&mut inner.kept);
        inner.file = Some(store);
        Self {
            inner: Mutex::new(inner),
            ready: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// Queues a lasting event.
    pub fn emit(&self, kind: &str, data: Value) {
        {
            let mut inner = self.lock();
            inner.last += 1;
            let envelope = Envelope::Event {
                seq: inner.last,
                kind: kind.to_owned(),
                data,
            };
            let text = line(&envelope);
            let size = text.len();
            let body = inner
                .file
                .as_mut()
                .and_then(|store| store.append(&text))
                .map_or_else(|| Body::Memory(text.into_boxed_str()), Body::Disk);
            let seq = inner.last;
            inner.bytes += size;
            inner.kept.push_back(Kept { seq, size, body });
            inner.trim_kept();
            let Inner { file, kept, .. } = &mut *inner;
            if let Some(store) = file.as_mut()
                && store.length >= COMPACT_BYTES
            {
                store.rewrite(kept);
            }
        }
        self.ready.notify_one();
    }

    /// Queues a live-only event (number 0, never acknowledged), if connected.
    pub fn emit_live(&self, kind: &str, data: Value) {
        {
            let mut inner = self.lock();
            if !inner.connected {
                return;
            }
            let event = Envelope::Event {
                seq: 0,
                kind: kind.to_owned(),
                data,
            };
            // A new observation replaces the previous snapshot for this chat.
            if kind == "chat.event"
                && let Envelope::Event { data, .. } = &event
                && data["event"]["type"] == "observation"
                && let Some(index) = inner.live.iter().rposition(|(queued, _)| {
                    matches!(queued, Envelope::Event { kind, data: old, .. }
                        if kind == "chat.event" && old["chat"] == data["chat"]
                            && old["event"]["type"] == "observation")
                })
                && let Some((_, bytes)) = inner.live.remove(index)
            {
                inner.live_bytes -= bytes;
            }
            let size = line(&event).len();
            if size > LIVE_BYTES {
                return;
            }
            while inner.live_bytes + size > LIVE_BYTES || inner.live.len() >= LIVE_COUNT {
                if let Some((_, removed)) = inner.live.pop_front() {
                    inner.live_bytes -= removed;
                }
            }
            inner.live_bytes += size;
            inner.live.push_back((event, size));
        }
        self.ready.notify_one();
    }

    /// The server stored everything up to `seq`.
    pub fn ack(&self, seq: u64) {
        let mut inner = self.lock();
        let acknowledged = inner.discard_through(seq);
        let Inner { file, kept, .. } = &mut *inner;
        if let Some(store) = file.as_mut() {
            store.acknowledged += acknowledged;
            // Nothing left, or much acknowledged: the file shrinks; else the
            // acknowledgement is noted in it.
            if (kept.is_empty() && store.acknowledged > 0) || store.acknowledged >= COMPACT_AFTER {
                store.rewrite(kept);
            } else if acknowledged > 0 {
                let _ = store.append(&line(&Envelope::Ack { seq }));
            }
        }
    }

    /// A connection is ready: everything kept is sent again, after word of
    /// any events dropped for room.
    pub fn connected(&self) {
        let dropped = {
            let mut inner = self.lock();
            inner.connected = true;
            inner.sent = 0;
            inner.live.clear();
            inner.live_bytes = 0;
            std::mem::take(&mut inner.dropped)
        };
        if dropped > 0 {
            self.emit("buffer.overflow", json!({ "dropped": dropped }));
        }
        self.ready.notify_one();
    }

    pub fn disconnected(&self) {
        let mut inner = self.lock();
        inner.connected = false;
        inner.live.clear();
        inner.live_bytes = 0;
    }

    /// What to send now: lasting events not sent on this connection, then
    /// live ones.
    pub fn take(&self, limit: usize) -> Vec<Envelope> {
        let mut inner = self.lock();
        let Inner {
            sent,
            kept,
            file,
            live,
            live_bytes,
            ..
        } = &mut *inner;
        let mut out = Vec::new();
        let mut bytes = 0;
        let mut buffer = Vec::new();
        let first = kept.partition_point(|event| event.seq <= *sent);
        for event in kept.range(first..).take(limit) {
            if !out.is_empty() && bytes + event.size > BATCH_BYTES {
                break;
            }
            let decoded = match (&event.body, file.as_mut()) {
                (Body::Memory(text), _) => serde_json::from_str(text).ok(),
                (Body::Disk(_), Some(store)) => store
                    .read(event, &mut buffer)
                    .ok()
                    .and_then(|()| serde_json::from_slice(&buffer).ok()),
                _ => None,
            };
            let Some(envelope) = decoded else {
                tracing::error!(seq = event.seq, "cannot read a durable outbox event");
                // Do not send anything that could acknowledge past this event.
                return out;
            };
            bytes += event.size;
            *sent = event.seq;
            out.push(envelope);
        }
        while out.len() < limit {
            let Some((_, size)) = live.front() else { break };
            if !out.is_empty() && bytes + size > BATCH_BYTES {
                break;
            }
            if let Some((event, size)) = live.pop_front() {
                bytes += size;
                *live_bytes -= size;
                out.push(event);
            }
        }
        if kept.back().is_some_and(|event| event.seq > *sent) || !live.is_empty() {
            self.ready.notify_one();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn numbers(envelopes: &[Envelope]) -> Vec<u64> {
        envelopes
            .iter()
            .filter_map(|event| match event {
                Envelope::Event { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn keeps_events_until_acknowledged_and_sends_them_again_after_reconnecting() {
        let outbox = Outbox::default();
        outbox.emit("a", json!(1));
        outbox.emit_live("delta", json!(0));
        outbox.connected();
        outbox.emit("b", json!(2));
        outbox.emit_live("delta", json!(0));
        assert_eq!(numbers(&outbox.take(2)), [1, 2]);
        assert_eq!(numbers(&outbox.take(2)), [0]);
        assert!(outbox.take(2).is_empty());
        outbox.ack(1);
        outbox.disconnected();
        outbox.emit("c", json!(3));
        outbox.emit_live("delta", json!(0));
        outbox.connected();
        assert_eq!(numbers(&outbox.take(2)), [2, 3]);
        outbox.ack(3);
        outbox.connected();
        assert!(outbox.take(2).is_empty());
    }

    #[test]
    fn reads_durable_batches_from_disk_and_bounds_live_backlog() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Outbox::open(dir.path());
        outbox.connected();
        let data = json!("x".repeat(BATCH_BYTES / 2));
        outbox.emit("a", data.clone());
        outbox.emit("b", data.clone());
        assert!(
            outbox
                .lock()
                .kept
                .iter()
                .all(|event| matches!(event.body, Body::Disk(_)))
        );
        for _ in 0..8 {
            outbox.emit_live("delta", data.clone());
        }
        assert!(outbox.lock().live_bytes <= LIVE_BYTES);
        assert_eq!(numbers(&outbox.take(16)), [1]);
        assert_eq!(numbers(&outbox.take(16)), [2]);
        assert_eq!(numbers(&outbox.take(16)), [0]);
        outbox.disconnected();
        outbox.connected();
        assert_eq!(numbers(&outbox.take(16)), [1]);
    }

    #[test]
    fn unacknowledged_events_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let outbox = Outbox::open(dir.path());
            outbox.emit("a", json!(1));
            outbox.emit("b", json!(2));
            outbox.emit("c", json!(3));
            outbox.connected();
            outbox.take(3);
            outbox.ack(1);
        }
        // A line cut short by a crash is skipped.
        let path = dir.path().join(FILE);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"kind\":\"event\",\"se").unwrap();

        let outbox = Outbox::open(dir.path());
        outbox.connected();
        assert_eq!(numbers(&outbox.take(3)), [2, 3]);
        // Numbering goes on after what was kept.
        outbox.emit("d", json!(4));
        assert_eq!(numbers(&outbox.take(3)), [4]);
        // Everything acknowledged: the file is emptied.
        outbox.ack(4);
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }
}
