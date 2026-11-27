//! Plugins.

use std::path::{Path, PathBuf};

use tokio::sync::watch;

use ahab::daemon::{self, DaemonConfig};
use ahab::modules::machines::presence::api::MachineFacts;
use ahab::modules::machines::{Machine, MachineKey};

use crate::support::TestServer;

mod channel;
mod command;
mod diagnostics;
mod event;
mod hook;
mod host;
mod notification;
mod plugin;
mod setting;

/// The server's own machine with its daemon, running plugins with `node`.
pub async fn local_daemon(
    server: &TestServer,
    node: Option<PathBuf>,
) -> (Machine, watch::Sender<bool>) {
    let key = MachineKey::from_secret([9; 32]);
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
        plugin_node: node,
        tools: ahab::modules::tools::registry::daemon::ToolOptions {
            login_shell: false,
            ..Default::default()
        },
    };
    tokio::spawn(daemon::run(config, stopped));
    server.online(&machine).await;
    (machine, stop)
}

/// A copy of the built echo plugin, to change.
pub fn echo_copy() -> tempfile::TempDir {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/echo");
    assert!(
        from.join("dist/daemon.js").is_file(),
        "the echo plugin is not built: run `make plugin-fixtures`"
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        from.join("ahab-plugin.json"),
        dir.path().join("ahab-plugin.json"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("dist")).unwrap();
    for file in ["web.js", "daemon.js", "package.json", "settings.json"] {
        std::fs::copy(
            from.join("dist").join(file),
            dir.path().join("dist").join(file),
        )
        .unwrap();
    }
    dir
}

/// A copy of the built notes plugin (the diagnostics API).
pub fn notes_copy() -> tempfile::TempDir {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/notes");
    assert!(
        from.join("dist/daemon.js").is_file(),
        "the notes plugin is not built: run `make plugin-fixtures`"
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        from.join("ahab-plugin.json"),
        dir.path().join("ahab-plugin.json"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("dist")).unwrap();
    for file in ["daemon.js", "package.json"] {
        std::fs::copy(
            from.join("dist").join(file),
            dir.path().join("dist").join(file),
        )
        .unwrap();
    }
    dir
}
