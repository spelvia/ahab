//! Adding machines and servers over SSH, against a fake
//! `ssh` and `scp` that run the "remote" commands here, in a temporary home
//! with a pretend service manager. Nothing connects to another host.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::support::TestServer;

/// A pretend host: `ssh` and `scp` that run here, with `home` as the
/// remote user's home, and `bin` first on its `PATH`.
struct FakeHost {
    root: tempfile::TempDir,
}

/// What the local `ahab` did.
struct Ran {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Ran {
    /// The last JSON value it printed on standard output.
    fn json(&self) -> Value {
        serde_json::Deserializer::from_str(&self.stdout)
            .into_iter::<Value>()
            .map_while(Result::ok)
            .last()
            .unwrap_or_else(|| panic!("no JSON: {}\n{}", self.stdout, self.stderr))
    }

    /// The problem it printed on standard error.
    fn problem(&self) -> Value {
        self.stderr
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line).ok())
            .unwrap_or_else(|| panic!("no problem: {}", self.stderr))
    }
}

fn script(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

/// Systemd's user manager, pretended: "starting" the unit runs the
/// installed `ahab up --foreground` in the background.
const SYSTEMCTL: &str = r#"#!/bin/sh
s="$HOME/.fake-systemd"; mkdir -p "$s"
[ "$1" = --user ] && shift
running() { [ -f "$s/pid" ] && kill -0 "$(cat "$s/pid")" 2>/dev/null; }
start() { running || { nohup "$HOME/.local/bin/ahab" up --foreground >>"$s/log" 2>&1 </dev/null & echo $! > "$s/pid"; }; }
stop() { if running; then kill "$(cat "$s/pid")"; fi; rm -f "$s/pid"; }
case "$1" in
  start) start;;
  restart) stop; sleep 0.3; start;;
  disable) stop;;
  show) echo LoadState=loaded; echo UnitFileState=enabled
        if running; then echo ActiveState=active; else echo ActiveState=inactive; fi;;
esac
exit 0
"#;

impl FakeHost {
    fn new() -> Self {
        // Beside the build, so `scp` can link the large binary instead of
        // copying it.
        let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let at = root.path().display().to_string();
        for dir in ["home", "bin", "local"] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        script(
            &root.path().join("ssh"),
            &format!(
                r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    -o|-p) shift 2;;
    -T|-q) shift;;
    -G) echo "hostname 127.0.0.1"; exit 0;;
    --) shift; break;;
    *) break;;
  esac
done
dest="$1"; shift
echo "$dest $*" >> "{at}/ssh.log"
if [ -f "{at}/fail" ]; then cat "{at}/fail" >&2; exit 255; fi
cd "{at}/home" || exit 1
exec env -i HOME="{at}/home" USER="$(id -un)" PATH="{at}/bin:$PATH" sh -c "$*"
"#
            ),
        );
        script(
            &root.path().join("scp"),
            &format!(
                r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in -o|-P) shift 2;; -q) shift;; --) shift; break;; *) break;; esac
done
if [ -f "{at}/fail" ]; then cat "{at}/fail" >&2; exit 1; fi
ln -f "$1" "{at}/home/${{2#*:}}" 2>/dev/null || cp "$1" "{at}/home/${{2#*:}}"
"#
            ),
        );
        script(&root.path().join("bin/systemctl"), SYSTEMCTL);
        // Lingering is on already: nothing to change.
        script(
            &root.path().join("bin/loginctl"),
            &format!("#!/bin/sh\necho \"$*\" >> \"{at}/loginctl.log\"\necho yes\n"),
        );
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    /// Makes the next SSH connections fail, saying `text`.
    fn fail_with(&self, text: &str) {
        std::fs::write(self.root.path().join("fail"), text).unwrap();
    }

    /// Puts `program` (a script) first on the host's `PATH`.
    fn replace(&self, program: &str, text: &str) {
        script(&self.root.path().join("bin").join(program), text);
    }

    fn ssh_log(&self) -> String {
        std::fs::read_to_string(self.root.path().join("ssh.log")).unwrap_or_default()
    }

    /// Runs this computer's `ahab` with `args` and `token`, in a home of its
    /// own, against this host.
    async fn ahab(&self, args: &[&str], token: Option<&str>) -> Ran {
        let local = self.root.path().join("local");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ahab"));
        command
            .args(args)
            .current_dir(&local)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &local)
            .env("AHAB_CONFIG_DIR", local.join("config"))
            .env("AHAB_DATA_DIR", local.join("data"))
            .env("AHAB_TEST_SSH", self.root.path().join("ssh"))
            .env("AHAB_TEST_SCP", self.root.path().join("scp"));
        if let Some(token) = token {
            command.env("AHAB_TOKEN", token);
        }
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        Ran {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        // Stops what the pretend service manager started.
        if let Ok(pid) = std::fs::read_to_string(self.home().join(".fake-systemd/pid")) {
            let _ = Command::new("kill").arg(pid.trim()).status();
        }
    }
}

async fn add(host: &FakeHost, server: &TestServer, extra: &[&str]) -> Ran {
    let mut args = vec![
        "--server",
        &server.url,
        "--json",
        "machines",
        "add",
        "me@build-box",
        "--server-url",
        &server.url,
    ];
    args.extend(extra);
    host.ahab(&args, Some(&server.credential)).await
}

#[tokio::test]
async fn installs_joins_and_starts_a_machine_and_again_repairs_it() {
    let server = TestServer::start().await;
    let host = FakeHost::new();

    let ran = add(&host, &server, &[]).await;
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    let first = ran.json();
    assert_eq!(first["install"], "copied", "{first}");
    assert_eq!(first["started"], true);
    assert_eq!(first["online"], true);
    assert_eq!(first["linger"], "on");
    assert_eq!(first["machine"]["state"], "active");
    let id = first["machine"]["id"].as_str().unwrap().to_owned();
    assert!(host.home().join(".local/bin/ahab").exists());

    // The server's key is pinned there, and the code is gone.
    let config = std::fs::read_to_string(host.home().join(".config/ahab/config.toml")).unwrap();
    assert!(
        config.contains(&format!(
            "server_fingerprint = \"{}\"",
            server.state.presence.server_fingerprint()
        )),
        "{config}"
    );
    assert!(config.contains(&format!("server = \"{}\"", server.url)));
    let (_, codes) = server.get("/join-codes").await;
    assert_eq!(codes["items"], serde_json::json!([]));
    // The machine's key on the host is the one the server approved.
    let (_, machine) = server.get(&format!("/machines/{id}")).await;
    assert_eq!(machine["online"], true);

    // Again: nothing copied, nothing added; it still runs.
    let ran = add(&host, &server, &["--name", "builder"]).await;
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    let again = ran.json();
    assert_eq!(again["install"], "current", "{again}");
    assert_eq!(again["machine"]["id"], id.as_str());
    assert_eq!(again["machine"]["name"], "builder");
    assert_eq!(again["online"], true);
    let (_, machines) = server.get("/machines").await;
    assert_eq!(machines["items"].as_array().unwrap().len(), 1, "{machines}");
    let (_, codes) = server.get("/join-codes").await;
    assert_eq!(codes["items"], serde_json::json!([]));
}

#[tokio::test]
async fn a_host_of_another_platform_is_refused_before_anything_changes() {
    let server = TestServer::start().await;
    let host = FakeHost::new();
    host.replace(
        "uname",
        "#!/bin/sh\ncase \"$1\" in -m) echo arm64;; *) echo Darwin;; esac\n",
    );

    let ran = add(&host, &server, &[]).await;
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    let problem = ran.problem();
    assert_eq!(problem["code"], "platform_mismatch", "{problem}");
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("macos aarch64"), "{detail}");
    assert!(detail.contains("~/.local/bin/ahab"), "{detail}");
    assert!(!host.home().join(".local/bin/ahab").exists());
    let (_, codes) = server.get("/join-codes").await;
    assert_eq!(codes["items"], serde_json::json!([]));
    let (_, machines) = server.get("/machines").await;
    assert_eq!(machines["items"], serde_json::json!([]));
}

#[tokio::test]
async fn ssh_failures_say_what_to_do() {
    let server = TestServer::start().await;
    let host = FakeHost::new();

    host.fail_with("me@build-box: Permission denied (publickey).\n");
    let denied = add(&host, &server, &[]).await;
    assert_eq!(denied.code, 4, "{}", denied.stderr);
    let problem = denied.problem();
    assert_eq!(problem["code"], "ssh_needs_interaction");
    assert!(problem["detail"].as_str().unwrap().contains("ssh-add"));

    host.fail_with("Host key verification failed.\n");
    let unknown = add(&host, &server, &[]).await;
    assert_eq!(unknown.code, 4);
    assert!(
        unknown.problem()["detail"]
            .as_str()
            .unwrap()
            .contains("ssh me@build-box")
    );

    host.fail_with("ssh: connect to host build-box port 22: Connection refused\n");
    let refused = add(&host, &server, &[]).await;
    assert_eq!(refused.code, 5);
    assert_eq!(refused.problem()["code"], "ssh_unreachable");

    let (_, codes) = server.get("/join-codes").await;
    assert_eq!(codes["items"], serde_json::json!([]));
}

#[tokio::test]
async fn an_address_only_this_computer_reaches_is_not_given_to_the_host() {
    let server = TestServer::start().await;
    let host = FakeHost::new();
    let ran = host
        .ahab(
            &[
                "--server",
                &server.url,
                "--json",
                "machines",
                "add",
                "me@build-box",
            ],
            Some(&server.credential),
        )
        .await;
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert_eq!(ran.problem()["code"], "server_url_needed");
    assert_eq!(host.ssh_log(), "", "nothing ran over SSH");
}

#[tokio::test]
async fn a_host_without_a_service_manager_joins_and_says_how_to_start() {
    let server = TestServer::start().await;
    let host = FakeHost::new();
    host.replace("systemctl", "#!/bin/sh\nexit 1\n");

    let ran = add(&host, &server, &[]).await;
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    assert_eq!(ran.problem()["code"], "no_service_manager");
    assert_eq!(ran.json()["machine"]["state"], "active");
    assert_eq!(ran.json()["started"], false);
}

/// A free port on loopback, for the pretend host's server.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn installs_a_server_and_logs_this_command_line_in() {
    let host = FakeHost::new();
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let listen = format!("127.0.0.1:{port}");
    let login = [
        "--json",
        "login",
        "ssh://me@build-box",
        "--url",
        &url,
        "--listen-addr",
        &listen,
        "--no-browser",
    ];

    let ran = host.ahab(&login, None).await;
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    let first = ran.json();
    assert_eq!(first["install"], "copied", "{first}");
    assert_eq!(first["server"], url.as_str());
    assert_eq!(first["client"]["kind"], "cli");
    assert!(
        first["pairingLink"]
            .as_str()
            .is_some_and(|link| link.starts_with(&format!("{url}/pair#"))),
        "{first}"
    );
    let config = std::fs::read_to_string(host.home().join(".config/ahab/config.toml")).unwrap();
    assert!(
        config.contains(&format!("public_url = \"{url}\"")),
        "{config}"
    );

    // The command line here is logged in: it lists the server's clients.
    let clients = host
        .ahab(&["--server", &url, "--json", "clients"], None)
        .await;
    assert_eq!(clients.code, 0, "{}", clients.stderr);
    assert!(clients.json()["items"].as_array().unwrap().len() >= 2);

    // Again: nothing copied, the same login.
    let ran = host.ahab(&login, None).await;
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    let again = ran.json();
    assert_eq!(again["install"], "current");
    assert_eq!(again["client"]["id"], first["client"]["id"]);
}

#[tokio::test]
async fn a_machine_of_another_server_is_not_made_a_server() {
    let host = FakeHost::new();
    std::fs::create_dir_all(host.home().join(".local/bin")).unwrap();
    // An ahab there that says it is a machine of another server.
    host.replace(
        "../home/.local/bin/ahab",
        "#!/bin/sh\ncase \"$1\" in --version) echo 'ahab 0.0.9';; status) echo \
         '{\"mode\":\"this computer'\"'\"'s daemon, joined to https://elsewhere\"}';; esac\n",
    );
    let ran = host
        .ahab(
            &["--json", "login", "ssh://me@build-box", "--no-browser"],
            None,
        )
        .await;
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert_eq!(ran.problem()["code"], "joined_elsewhere");
}

#[tokio::test]
async fn setup_refuses_a_server_with_another_fingerprint_than_given() {
    let server = TestServer::start().await;
    let host = FakeHost::new();
    let (_, code) = server.post("/join-codes", serde_json::json!({})).await;
    assert_eq!(
        code["serverFingerprint"],
        server.state.presence.server_fingerprint().as_str()
    );
    let ran = host
        .ahab(
            &[
                "--json",
                "setup",
                "--server",
                &server.url,
                "--code",
                code["code"].as_str().unwrap(),
                "--server-fingerprint",
                "SHA256:somebody-else",
                "--no-start",
            ],
            None,
        )
        .await;
    assert_eq!(ran.code, 4, "{}", ran.stderr);
    assert!(
        ran.problem()["detail"]
            .as_str()
            .unwrap()
            .contains("SHA256:somebody-else")
    );
    assert!(!host.root.path().join("local/config/config.toml").exists());
}
