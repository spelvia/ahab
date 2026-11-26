//! `ahab leave`: a machine stops being one, with a pretend
//! systemd user manager, in a home of its own. And `ahab status --json`
//! saying the computer's role, which the desktop app reads.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn script(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

struct Home {
    root: tempfile::TempDir,
}

impl Home {
    /// A home whose `systemctl` logs its calls and says the service runs.
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        script(
            &bin.join("systemctl"),
            &format!(
                "#!/bin/sh\necho \"$*\" >> '{}/systemctl.log'\n\
                 case \"$*\" in *show\\ ahab*) echo LoadState=loaded; echo UnitFileState=enabled; \
                 echo ActiveState=active;; esac\nexit 0\n",
                root.path().display()
            ),
        );
        Self { root }
    }

    fn config(&self) -> std::path::PathBuf {
        self.root.path().join("config/config.toml")
    }

    fn ahab(&self, args: &[&str]) -> (i32, Value, String) {
        let path = format!(
            "{}:{}",
            self.root.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = Command::new(env!("CARGO_BIN_EXE_ahab"))
            .args(args)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.path())
            .env("AHAB_CONFIG_DIR", self.root.path().join("config"))
            .env("AHAB_DATA_DIR", self.root.path().join("data"))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let json = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
        (output.status.code().unwrap_or(-1), json, stderr)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.root.path().join("systemctl.log")).unwrap_or_default()
    }
}

#[test]
fn a_machine_leaves_its_server_and_its_daemon_stops() {
    let home = Home::new();
    std::fs::create_dir_all(home.config().parent().unwrap()).unwrap();
    std::fs::write(
        home.config(),
        "server = \"https://atlas.lan\"\nserver_fingerprint = \"SHA256:abc\"\ndaemon = true\n",
    )
    .unwrap();

    let (code, status, _) = home.ahab(&["--json", "status"]);
    assert_eq!(status["role"], "machine", "{status} ({code})");
    assert_eq!(status["joined"], "https://atlas.lan");

    let (code, left, stderr) = home.ahab(&["--json", "leave", "--yes"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(left["left"], "https://atlas.lan");
    assert_eq!(left["stopped"], true);
    assert!(
        home.log().contains("--user disable --now ahab"),
        "{}",
        home.log()
    );
    let config = std::fs::read_to_string(home.config()).unwrap();
    assert!(
        !config.contains("atlas") && !config.contains("SHA256"),
        "{config}"
    );
    assert!(config.contains("daemon = true"), "{config}");

    let (_, status, _) = home.ahab(&["--json", "status"]);
    assert_eq!(status["role"], "server");
    assert_eq!(status["joined"], Value::Null);
}

#[test]
fn leaving_asks_first_and_needs_a_server() {
    let home = Home::new();
    let (code, _, stderr) = home.ahab(&["--json", "leave", "--yes"]);
    assert_eq!(code, 3, "{stderr}");
    assert!(stderr.contains("Not joined"), "{stderr}");

    std::fs::create_dir_all(home.config().parent().unwrap()).unwrap();
    std::fs::write(home.config(), "server = \"https://atlas.lan\"\n").unwrap();
    // Without a terminal and without --yes, asking fails and names the flag.
    let (code, _, stderr) = home.ahab(&["--json", "leave"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("--yes"), "{stderr}");
    assert!(
        home.log().is_empty() || !home.log().contains("disable"),
        "{}",
        home.log()
    );
}
