//! Ahab on this computer, end to end: the app installs
//! the bundled `ahab`, starts the user service through `ahab up` (under a
//! pretend systemd, `tests/fake-systemctl.sh`), pairs through the local
//! credential, detects an Ahab already there, joins another server as a
//! machine and leaves it. Each computer is a temporary home on ports
//! 8187–8189; nothing reaches the real service manager or this computer's
//! own Ahab.
//!
//! The `ahab` used is `AHAB_TEST_BIN`, else the sidecar `make app` copies
//! into `binaries/`.

#![expect(clippy::unwrap_used, reason = "a failed unwrap is a failed test")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ahab_desktop::computer::{Computer, Env, Role};
use ahab_desktop::secrets::{EncryptedFile, Keychain};
use ahab_desktop::server_api::ServerApi;
use ahab_desktop::state::AppState;

fn ahab_binary() -> PathBuf {
    let path = std::env::var_os("AHAB_TEST_BIN").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/ahab-x86_64-unknown-linux-gnu"),
        PathBuf::from,
    );
    assert!(
        path.is_file(),
        "no ahab binary at {}: run `make app-test`, or set AHAB_TEST_BIN",
        path.display()
    );
    path
}

/// `PATH` without any folder that has an `ahab` in it, after the fake
/// programs.
fn clean_path(bin: &Path) -> String {
    let system = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.to_owned()];
    dirs.extend(std::env::split_paths(&system).filter(|dir| !dir.join("ahab").exists()));
    std::env::join_paths(dirs).unwrap().into_string().unwrap()
}

/// A pretend computer: a home with a pretend systemd and the given port.
struct Home {
    root: tempfile::TempDir,
}

impl Home {
    fn new(port: u16) -> Self {
        let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let bin = root.path().join("fake-bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-systemctl.sh"),
            bin.join("systemctl"),
        )
        .unwrap();
        std::fs::write(bin.join("loginctl"), "#!/bin/sh\necho yes\n").unwrap();
        for name in ["systemctl", "loginctl"] {
            std::fs::set_permissions(
                bin.join(name),
                std::os::unix::fs::PermissionsExt::from_mode(0o755),
            )
            .unwrap();
        }
        let config = root.path().join(".config/ahab");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("config.toml"),
            format!("listen_addr = \"127.0.0.1:{port}\"\n"),
        )
        .unwrap();
        Self { root }
    }

    fn env(&self) -> Env {
        let home = self.root.path();
        Env::isolated(vec![
            ("HOME".into(), home.display().to_string()),
            ("PATH".into(), clean_path(&home.join("fake-bin"))),
            ("USER".into(), "tester".into()),
            (
                "XDG_CONFIG_HOME".into(),
                home.join(".config").display().to_string(),
            ),
            (
                "XDG_DATA_HOME".into(),
                home.join(".local/share").display().to_string(),
            ),
        ])
    }

    fn computer(&self) -> Computer {
        Computer::new(self.env(), Some(ahab_binary()))
    }

    /// An app on this computer, with its own data folder.
    fn app(&self, name: &str) -> Arc<AppState> {
        let dir = self.root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        AppState::new(
            &dir,
            Box::new(EncryptedFile::new(&dir)),
            None,
            self.computer(),
        )
    }

    fn local_credential(&self) -> String {
        std::fs::read_to_string(self.root.path().join(".local/share/ahab/credential"))
            .unwrap()
            .trim()
            .to_owned()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.path().join(".fake-systemd/calls")).unwrap_or_default()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if let Ok(pid) = std::fs::read_to_string(self.root.path().join(".fake-systemd/pid")) {
            let _ = std::process::Command::new("kill").arg(pid.trim()).status();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_ahab_here_detects_it_and_joins_another_server() {
    let here = Home::new(8187);
    let app = here.app("app");

    // Nothing yet: the first-launch question.
    let before = app.this_computer().await;
    assert_eq!(before.role, Role::None, "{before:?}");
    assert!(before.installed.is_none());
    assert_eq!(before.service_manager.as_deref(), Some("systemd"));

    // Run Ahab here: installed, started through `ahab up`, paired.
    let server = app.run_here().await.unwrap();
    assert!(server.local);
    assert_eq!(server.url, "http://127.0.0.1:8187");
    let installed = here.root.path().join(".local/bin/ahab");
    assert!(installed.is_file());
    let unit = std::fs::read_to_string(here.root.path().join(".config/systemd/user/ahab.service"))
        .unwrap();
    assert!(
        unit.contains(&format!(
            "ExecStart=\"{}\" up --foreground",
            installed.display()
        )),
        "{unit}"
    );
    assert!(
        here.calls().contains("--user enable ahab"),
        "{}",
        here.calls()
    );

    // The app is a client of kind `app`, its credential in the store only.
    let credential = app.servers.credential(&server.id).unwrap().unwrap();
    let client = ServerApi::new(&server.url)
        .current_client(&credential)
        .await
        .unwrap();
    assert_eq!(client["kind"], "app", "{client}");
    let list = std::fs::read_to_string(here.root.path().join("app/servers.json")).unwrap();
    assert!(!list.contains(&credential));

    let running = app.this_computer().await;
    assert_eq!(running.role, Role::Server);
    assert!(running.running && running.has_ahab());
    assert_eq!(running.url.as_deref(), Some("http://127.0.0.1:8187"));

    // A second app on the same computer finds the Ahab already running and
    // uses it, without starting a second one.
    let calls_before = here.calls().matches("start ahab").count();
    let other = here.app("other-app");
    assert!(other.this_computer().await.has_ahab());
    let used = other.use_existing().await.unwrap();
    assert_eq!(used.url, server.url);
    assert_eq!(here.calls().matches("start ahab").count(), calls_before);
    // Running it again keeps the pairing that works.
    assert_eq!(app.run_here().await.unwrap().id, server.id);

    // Another computer runs a server; the app pairs with it and joins this
    // computer to it as a machine, its fingerprint pinned.
    let there = Home::new(8188);
    there.computer().start().unwrap();
    let remote_url = "http://127.0.0.1:8188";
    let secret = ServerApi::new(remote_url)
        .create_link(&there.local_credential())
        .await
        .unwrap();
    let remote = app.pair(remote_url, Some(&secret), None).await.unwrap();
    assert!(!remote.local);
    let joined = app.join_server(&remote.id).await.unwrap();
    assert_eq!(joined.role, Role::Machine, "{joined:?}");
    assert_eq!(joined.joined.as_deref(), Some(remote_url));
    let machines = reqwest::Client::new()
        .get(format!("{remote_url}/api/v1/machines"))
        .bearer_auth(there.local_credential())
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(machines["items"].as_array().unwrap().len(), 2, "{machines}");

    // Leaving: the daemon stops; this computer is set up as a server again
    // (its data stayed), not running.
    let left = app.leave().await.unwrap();
    assert_eq!(left.role, Role::Server, "{left:?}");
    assert!(!left.running);
    let started = app.start_here().await.unwrap();
    assert!(
        started.running && started.server_version.is_some(),
        "{started:?}"
    );

    // Stopping the other one.
    let stopped = there.app("app").stop_here().await.unwrap();
    assert!(!stopped.running, "{stopped:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_by_request_and_removing_revokes_the_client() {
    let home = Home::new(8189);
    home.computer().start().unwrap();
    let url = "http://127.0.0.1:8189";
    let app = AppState::new(
        &home.root.path().join("app"),
        Box::new(Keychain::new(keyring_core::mock::Store::new().unwrap())),
        None,
        Computer::new(Env::isolated(vec![]), None),
    );

    let reached = app.reach("127.0.0.1:8189").await;
    // Without a scheme the app tries https, which this server does not speak.
    assert!(reached.is_err());
    let reached = app.reach(url).await.unwrap();
    assert!(reached.known.is_none());

    let asked = app.ask(url).await.unwrap();
    let approve = reqwest::Client::new()
        .post(format!(
            "{url}/api/v1/pairing-requests/{}/approve",
            asked.code
        ))
        .bearer_auth(home.local_credential())
        .header("origin", url)
        .send()
        .await
        .unwrap();
    assert!(
        approve.status().is_success(),
        "{}",
        approve.text().await.unwrap()
    );
    let server = loop {
        match app.wait(&asked.id).await.unwrap() {
            ahab_desktop::state::Waited::Paired { server } => break server,
            ahab_desktop::state::Waited::Waiting => {}
            other => panic!("{other:?}"),
        }
    };
    assert!(app.still_paired(&server.id).await.unwrap());
    let credential = app.servers.credential(&server.id).unwrap().unwrap();

    app.remove(&server.id).await.unwrap();
    assert!(app.servers.load().unwrap().servers.is_empty());
    let after = ServerApi::new(url)
        .current_client(&credential)
        .await
        .unwrap_err();
    assert_eq!(after.code(), "not_paired");
}
