//! The daemon's end of channels: streams of bytes with the server,
//! as binary frames on its connection. A request opens one by naming its ID;
//! frames for it come in through `receive`, and what the daemon writes goes
//! out through the connection that is up at the time. A dropped connection
//! closes every channel: their readers reattach through the server.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::modules::machines::presence::api::channel;
use crate::shared::sync::lock;

/// How many frames from the server a channel holds for a slow reader.
const FRAMES: usize = 512;

type Open = Arc<Mutex<HashMap<Uuid, mpsc::Sender<Vec<u8>>>>>;

pub struct Channels {
    open: Open,
    outgoing: mpsc::Sender<Vec<u8>>,
    /// Frames for the server, taken by the connection that is up.
    pub frames: tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>,
}

impl Default for Channels {
    fn default() -> Self {
        let (outgoing, frames) = mpsc::channel(FRAMES);
        Self {
            open: Arc::default(),
            outgoing,
            frames: tokio::sync::Mutex::new(frames),
        }
    }
}

impl Channels {
    /// The daemon's end of the channel `id`, which the server opened.
    pub fn open(&self, id: Uuid) -> DaemonChannel {
        let (sender, incoming) = mpsc::channel(FRAMES);
        lock(&self.open).insert(id, sender);
        DaemonChannel {
            id,
            incoming,
            outgoing: self.outgoing.clone(),
            open: Arc::clone(&self.open),
        }
    }

    /// A channel's frame from the server.
    pub fn receive(&self, frame: &[u8]) {
        let Some((kind, id, payload)) = channel::decode(frame) else {
            return;
        };
        let mut open = lock(&self.open);
        if kind == channel::CLOSE {
            open.remove(&id);
        } else if let Some(sender) = open.get(&id)
            && sender.try_send(payload.to_vec()).is_err()
        {
            open.remove(&id);
        }
    }

    /// The connection ended: every channel closes.
    pub fn close_all(&self) {
        lock(&self.open).clear();
    }
}

/// One channel, at the daemon: what the server sends, and a way to answer.
/// Dropping it closes the channel.
pub struct DaemonChannel {
    pub id: Uuid,
    incoming: mpsc::Receiver<Vec<u8>>,
    outgoing: mpsc::Sender<Vec<u8>>,
    open: Open,
}

impl DaemonChannel {
    /// The next bytes from the server; `None` once the channel closed.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.incoming.recv().await
    }

    /// Sends bytes to the server; false once the channel is closed.
    pub async fn send(&self, payload: &[u8]) -> bool {
        if !lock(&self.open).contains_key(&self.id) {
            return false;
        }
        self.outgoing
            .send(channel::encode(channel::DATA, self.id, payload))
            .await
            .is_ok()
    }

    /// Splits it: the reading half, and a sender usable from another task.
    pub fn split(self) -> (ChannelReader, ChannelWriter) {
        let id = self.id;
        let writer = ChannelWriter {
            id,
            outgoing: self.outgoing.clone(),
            open: Arc::clone(&self.open),
        };
        // The reader keeps the channel open; the writer closes nothing.
        (ChannelReader { inner: self }, writer)
    }
}

impl Drop for DaemonChannel {
    fn drop(&mut self) {
        let was_open = lock(&self.open).remove(&self.id).is_some();
        if was_open {
            let _ = self
                .outgoing
                .try_send(channel::encode(channel::CLOSE, self.id, &[]));
        }
    }
}

/// The reading half of a channel; dropping it closes the channel.
pub struct ChannelReader {
    inner: DaemonChannel,
}

impl ChannelReader {
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.inner.recv().await
    }
}

/// The sending half of a channel.
#[derive(Clone)]
pub struct ChannelWriter {
    id: Uuid,
    outgoing: mpsc::Sender<Vec<u8>>,
    open: Open,
}

impl ChannelWriter {
    /// Sends bytes to the server; false once the channel is closed.
    pub async fn send(&self, payload: &[u8]) -> bool {
        if !lock(&self.open).contains_key(&self.id) {
            return false;
        }
        self.outgoing
            .send(channel::encode(channel::DATA, self.id, payload))
            .await
            .is_ok()
    }

    /// Closes the channel from this end.
    pub fn close(&self) {
        if lock(&self.open).remove(&self.id).is_some() {
            let _ = self
                .outgoing
                .try_send(channel::encode(channel::CLOSE, self.id, &[]));
        }
    }
}
