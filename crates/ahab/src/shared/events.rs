//! Ahab's own events, for plugins and the web app: a machine
//! came online, a terminal exited, a session was archived. Published by
//! the features, heard by whoever subscribes; nothing is stored.

use serde_json::Value;
use tokio::sync::broadcast;

/// One event: its name (`terminal.exited`; a plugin's are
/// `<plugin>.<name>`) and what it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub name: String,
    pub data: Value,
}

/// The bus events go on.
pub struct Events {
    sender: broadcast::Sender<Event>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            sender: broadcast::channel(1024).0,
        }
    }
}

impl Events {
    pub fn publish(&self, name: impl Into<String>, data: Value) {
        let _ = self.sender.send(Event {
            name: name.into(),
            data,
        });
    }

    /// Every event from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}
