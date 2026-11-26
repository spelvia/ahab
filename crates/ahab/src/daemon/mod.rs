//! The daemon's plumbing: the connection to its server, and answering the
//! server's requests. Feature logic stays in the modules.

pub mod channels;
pub mod connection;
pub mod dispatch;
pub mod outbox;

pub use connection::{DaemonConfig, DaemonError, host_of, run};

use crate::modules::machines::presence::api::MachineFacts;

/// What this computer reports about itself.
pub fn local_facts() -> MachineFacts {
    MachineFacts {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        hostname: gethostname::gethostname().to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}
