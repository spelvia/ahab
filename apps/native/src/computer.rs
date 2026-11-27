//! This computer: what Ahab does here, and the changes
//! the built-in screens make, always through the `ahab` command line: the
//! app never starts the server or the daemon itself, it installs and drives
//! the normal user service (`ahab up`, `ahab down`, `ahab setup`, `ahab
//! leave`), so the CLI and the app see the same Ahab, and closing the app
//! never stops an agent.
//!
//! One Ahab per computer: an `ahab` already installed (on `PATH`, or in
//! `~/.local/bin`) is used as it is; the bundled one is copied there only
//! when there is none.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

/// The environment `ahab` runs in: this process's, with overrides (tests
/// give a home, `PATH` and folders of their own).
#[derive(Debug, Clone, Default)]
pub struct Env {
    overrides: Vec<(String, String)>,
    /// Run `ahab` with only the overrides, nothing inherited (tests).
    isolated: bool,
}

impl Env {
    /// This process's environment.
    pub fn process() -> Self {
        Self::default()
    }

    /// Only these variables (tests: no way to reach the real service
    /// manager or folders).
    pub fn isolated(vars: Vec<(String, String)>) -> Self {
        Self {
            overrides: vars,
            isolated: true,
        }
    }

    pub fn var(&self, name: &str) -> Option<String> {
        if let Some((_, value)) = self.overrides.iter().find(|(key, _)| key == name) {
            return Some(value.clone());
        }
        if self.isolated {
            return None;
        }
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    fn home(&self) -> Option<PathBuf> {
        self.var("HOME").map(PathBuf::from)
    }

    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        if self.isolated {
            command.env_clear();
        }
        for (key, value) in &self.overrides {
            command.env(key, value);
        }
        command
    }

    /// Ahab's data folder, as `ahab` resolves it: `AHAB_DATA_DIR`, else
    /// `$XDG_DATA_HOME/ahab`, else `~/.local/share/ahab` (Linux).
    pub fn data_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = self.var("AHAB_DATA_DIR") {
            return Some(PathBuf::from(dir));
        }
        if cfg!(target_os = "macos") {
            return self
                .home()
                .map(|home| home.join("Library/Application Support/ahab"));
        }
        self.var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| self.home().map(|home| home.join(".local/share")))
            .map(|dir| dir.join("ahab"))
    }

    /// Where the bundled `ahab` is installed: `~/.local/bin`.
    pub fn install_dir(&self) -> Option<PathBuf> {
        self.home().map(|home| home.join(".local/bin"))
    }

    /// An `ahab` already installed: the first on `PATH` (outside the app's
    /// own `AppImage`), else `~/.local/bin/ahab`.
    pub fn installed_ahab(&self) -> Option<PathBuf> {
        let appdir = self.var("APPDIR").map(PathBuf::from);
        let on_path = self
            .var("PATH")
            .map(|path| std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|dir| dir.is_absolute())
            .filter(|dir| {
                !appdir
                    .as_ref()
                    .is_some_and(|appdir| dir.starts_with(appdir))
            })
            .map(|dir| dir.join("ahab"))
            .find(|path| is_executable(path));
        on_path.or_else(|| {
            self.install_dir()
                .map(|dir| dir.join("ahab"))
                .filter(|path| is_executable(path))
        })
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComputerError {
    #[error("Ahab is not installed on this computer, and this app has no `ahab` to install.")]
    NothingToRun,
    #[error("There is no home folder to install Ahab in.")]
    NoHome,
    #[error(
        "This computer has no service manager (a systemd user instance) for Ahab to run \
         in the background. Start it in a terminal with `ahab up --foreground`."
    )]
    NoServiceManager,
    #[error(
        "Ahab's service here is managed by your system configuration (such as Home Manager), \
         so the app leaves it alone. Start or stop it there."
    )]
    Managed,
    #[error("Cannot run {0}: {1}")]
    Run(String, String),
    #[error("{message}")]
    Failed { code: String, message: String },
    #[error("Cannot install Ahab: {0}")]
    Install(String),
    #[error("Ahab does not run here, so there is no local credential.")]
    NoCredential,
}

impl ComputerError {
    pub fn code(&self) -> &str {
        match self {
            Self::NothingToRun => "nothing_to_run",
            Self::NoHome => "no_home",
            Self::NoServiceManager => "no_service_manager",
            Self::Managed => "managed",
            Self::Run(..) => "cannot_run",
            Self::Failed { code, .. } => code,
            Self::Install(_) => "cannot_install",
            Self::NoCredential => "no_credential",
        }
    }
}

/// What this computer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Runs Ahab's server (and usually its daemon).
    Server,
    /// A machine of a server elsewhere: only its daemon runs here.
    Machine,
    /// Neither: Ahab is not set up here.
    None,
}

/// An `ahab` binary and its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binary {
    pub path: PathBuf,
    pub version: Option<String>,
}

/// What the built-in screens show about this computer (and, read only,
/// the bridge's `app.thisComputer()`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThisComputer {
    pub role: Role,
    /// The `ahab` installed here, if any.
    pub installed: Option<Binary>,
    /// The `ahab` this app bundles.
    pub bundled: Option<Binary>,
    /// systemd or launchd; none in most containers.
    pub service_manager: Option<String>,
    /// A service file not written by `ahab up` (Nix, by hand).
    pub managed: bool,
    pub service_installed: bool,
    /// The service runs.
    pub running: bool,
    /// The server this computer runs, from here.
    pub url: Option<String>,
    /// Its version when it answers.
    pub server_version: Option<String>,
    /// The server this computer is a machine of.
    pub joined: Option<String>,
    /// This computer as a machine: its name and state, as the server says.
    pub machine: Option<Value>,
    /// What runs, in words (`ahab status`).
    pub mode: Option<String>,
}

impl ThisComputer {
    /// An Ahab that runs, or is set up, here: the app offers to use it
    /// instead of asking what this computer should do.
    pub fn has_ahab(&self) -> bool {
        self.role != Role::None
    }
}

/// What `ahab … --json` printed: its last JSON value on standard output,
/// or the problem on standard error.
#[derive(Debug)]
struct Ran {
    success: bool,
    json: Value,
    problem: Value,
}

/// Drives `ahab` on this computer.
#[derive(Debug, Clone)]
pub struct Computer {
    pub env: Env,
    /// The `ahab` inside this app (the sidecar), if any.
    pub bundled: Option<PathBuf>,
}

impl Computer {
    pub fn new(env: Env, bundled: Option<PathBuf>) -> Self {
        Self { env, bundled }
    }

    /// The `ahab` to run: the installed one, else the bundled one.
    fn ahab(&self) -> Option<PathBuf> {
        self.env
            .installed_ahab()
            .or_else(|| self.bundled.clone().filter(|path| path.is_file()))
    }

    fn run(&self, exe: &Path, args: &[&str]) -> Result<Ran, ComputerError> {
        let output = self
            .env
            .command(exe)
            .arg("--json")
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| ComputerError::Run(exe.display().to_string(), error.to_string()))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(exe = %exe.display(), ?args, code = ?output.status.code(), %stderr, "ran ahab");
        Ok(Ran {
            success: output.status.success(),
            json: last_json(&stdout),
            problem: last_json(&stderr),
        })
    }

    fn version(&self, exe: &Path) -> Option<String> {
        let output = self.env.command(exe).arg("--version").output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.split_whitespace().last().map(str::to_owned)
    }

    /// What Ahab does on this computer (`ahab status`). Only reads.
    pub fn detect(&self) -> ThisComputer {
        let installed = self.env.installed_ahab().map(|path| Binary {
            version: self.version(&path),
            path,
        });
        let bundled = self
            .bundled
            .clone()
            .filter(|path| path.is_file())
            .map(|path| Binary {
                version: self.version(&path),
                path,
            });
        let status = self
            .ahab()
            .and_then(|exe| self.run(&exe, &["status"]).ok())
            .map_or(Value::Null, |ran| ran.json);
        interpret(&status, installed, bundled)
    }

    /// Installs the bundled `ahab` in `~/.local/bin`, unless an `ahab` is
    /// installed already; the one to use.
    pub fn install(&self) -> Result<PathBuf, ComputerError> {
        if let Some(installed) = self.env.installed_ahab() {
            return Ok(installed);
        }
        let bundled = self
            .bundled
            .as_ref()
            .filter(|path| path.is_file())
            .ok_or(ComputerError::NothingToRun)?;
        let dir = self.env.install_dir().ok_or(ComputerError::NoHome)?;
        std::fs::create_dir_all(&dir).map_err(|error| ComputerError::Install(error.to_string()))?;
        let target = dir.join("ahab");
        let temporary = dir.join("ahab.new");
        std::fs::copy(bundled, &temporary)
            .map_err(|error| ComputerError::Install(error.to_string()))?;
        #[cfg(unix)]
        std::fs::set_permissions(
            &temporary,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .map_err(|error| ComputerError::Install(error.to_string()))?;
        std::fs::rename(&temporary, &target)
            .map_err(|error| ComputerError::Install(error.to_string()))?;
        tracing::info!(path = %target.display(), "installed ahab");
        Ok(target)
    }

    /// Starts Ahab in the background as the user service (`ahab up`),
    /// installing the bundled `ahab` first when none is installed. Refuses
    /// where there is no service manager: the app never runs the server
    /// itself.
    pub fn start(&self) -> Result<ThisComputer, ComputerError> {
        if self.ahab().is_none() {
            return Err(ComputerError::NothingToRun);
        }
        // `ahab status` (installed or bundled) says whether there is one.
        let before = self.detect();
        if before.service_manager.is_none() {
            return Err(ComputerError::NoServiceManager);
        }
        if before.managed {
            return if before.running {
                Ok(before)
            } else {
                Err(ComputerError::Managed)
            };
        }
        let exe = self.install()?;
        let ran = self.run(&exe, &["up"])?;
        if !ran.success {
            return Err(failure(&ran, "Ahab did not start."));
        }
        Ok(self.detect())
    }

    /// Stops the user service (`ahab down`); data and settings stay.
    pub fn stop(&self) -> Result<ThisComputer, ComputerError> {
        let exe = self.ahab().ok_or(ComputerError::NothingToRun)?;
        let before = self.detect();
        if before.managed {
            return Err(ComputerError::Managed);
        }
        let ran = self.run(&exe, &["down"])?;
        if !ran.success {
            return Err(failure(&ran, "Ahab did not stop."));
        }
        Ok(self.detect())
    }

    /// Joins this computer to `server` as a machine (`ahab setup`), which
    /// starts its daemon-only service. `fingerprint` pins the server's key;
    /// without one, `trust` must say the person accepted the server
    /// shown to them.
    pub fn join(
        &self,
        server: &str,
        code: &str,
        fingerprint: Option<&str>,
        trust: bool,
    ) -> Result<ThisComputer, ComputerError> {
        let before = self.detect();
        if before.managed {
            return Err(ComputerError::Managed);
        }
        let exe = self.install()?;
        let mut args = vec!["setup", "--server", server, "--code", code];
        if let Some(fingerprint) = fingerprint {
            args.extend(["--server-fingerprint", fingerprint]);
        }
        if trust {
            args.push("--yes");
        }
        let ran = self.run(&exe, &args)?;
        if !ran.success {
            return Err(failure(&ran, "This computer did not join."));
        }
        Ok(self.detect())
    }

    /// Leaves the server this computer is a machine of (`ahab leave`): its
    /// daemon stops; its key stays.
    pub fn leave(&self) -> Result<ThisComputer, ComputerError> {
        let exe = self.ahab().ok_or(ComputerError::NothingToRun)?;
        let ran = self.run(&exe, &["leave", "--yes"])?;
        if !ran.success {
            return Err(failure(&ran, "This computer did not leave its server."));
        }
        Ok(self.detect())
    }

    /// The local credential, which pairs the app with the server
    /// this computer runs.
    pub fn local_credential(&self) -> Result<String, ComputerError> {
        let file = self
            .env
            .data_dir()
            .ok_or(ComputerError::NoHome)?
            .join("credential");
        std::fs::read_to_string(file)
            .map(|text| text.trim().to_owned())
            .ok()
            .filter(|text| !text.is_empty())
            .ok_or(ComputerError::NoCredential)
    }
}

fn failure(ran: &Ran, fallback: &str) -> ComputerError {
    let code = ran.problem["code"].as_str().unwrap_or("ahab_failed");
    let title = ran.problem["title"].as_str();
    let detail = ran.problem["detail"].as_str();
    let message = match (title, detail) {
        (Some(title), Some(detail)) => format!("{title}: {detail}"),
        (Some(text), None) | (None, Some(text)) => text.to_owned(),
        (None, None) => fallback.to_owned(),
    };
    ComputerError::Failed {
        code: code.to_owned(),
        message,
    }
}

/// The last JSON value in `text` (a command may print more than one line).
fn last_json(text: &str) -> Value {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .map_while(Result::ok)
        .last()
        .or_else(|| {
            text.lines()
                .rev()
                .find_map(|line| serde_json::from_str(line).ok())
        })
        .unwrap_or(Value::Null)
}

/// Reads `ahab status --json`. Newer `ahab`s say their `role` and `joined`
/// server; older ones only describe it in `mode`.
pub fn interpret(
    status: &Value,
    installed: Option<Binary>,
    bundled: Option<Binary>,
) -> ThisComputer {
    let service = &status["service"];
    let mode = status["mode"].as_str().map(str::to_owned);
    let joined = status["joined"].as_str().map(str::to_owned).or_else(|| {
        mode.as_deref()
            .and_then(|mode| mode.strip_prefix("this computer's daemon, joined to "))
            .map(str::to_owned)
    });
    let running = service["running"].as_bool().unwrap_or(false);
    let service_installed = service["installed"].as_bool().unwrap_or(false);
    let server_version = status["server"]["version"].as_str().map(str::to_owned);
    let says = status["role"].as_str();
    let has_server = service_installed || running || server_version.is_some();
    let role = if says == Some("machine") || (says.is_none() && joined.is_some()) {
        Role::Machine
    } else if matches!(says, Some("server") | None) && has_server {
        Role::Server
    } else {
        Role::None
    };
    let role = if role == Role::Machine && !service_installed && !running && joined.is_none() {
        Role::None
    } else {
        role
    };
    ThisComputer {
        role,
        installed,
        bundled,
        service_manager: service["manager"].as_str().map(str::to_owned),
        managed: service["managed"].as_bool().unwrap_or(false),
        service_installed,
        running,
        url: (role == Role::Server)
            .then(|| status["server"]["url"].as_str().map(str::to_owned))
            .flatten(),
        server_version,
        joined,
        machine: Some(status["machine"].clone()).filter(|machine| !machine.is_null()),
        mode,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// These tests write fake programs and run them. Writing one while
    /// another test forks lets the child inherit the open file, and running
    /// it then fails with "Text file busy" (ETXTBSY), so they take turns.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        TURN.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn script(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }

    fn env(home: &Path, path: &str) -> Env {
        Env::isolated(vec![
            ("HOME".into(), home.display().to_string()),
            ("PATH".into(), path.into()),
        ])
    }

    #[test]
    fn reads_a_computer_that_runs_ahab() {
        let _turn = one_at_a_time();
        let status = json!({
            "mode": "the server and this computer's daemon",
            "role": "server",
            "service": { "manager": "systemd", "file": "/x", "managed": false,
                         "installed": true, "enabled": true, "running": true },
            "server": { "url": "http://127.0.0.1:8185", "version": "0.1.0" },
            "machine": { "name": "atlas", "state": "online" },
        });
        let computer = interpret(&status, None, None);
        assert_eq!(computer.role, Role::Server);
        assert!(computer.running && computer.has_ahab());
        assert_eq!(computer.url.as_deref(), Some("http://127.0.0.1:8185"));
        assert_eq!(computer.server_version.as_deref(), Some("0.1.0"));
        assert_eq!(computer.service_manager.as_deref(), Some("systemd"));
    }

    /// What `ahab status --json` of a release before `role` says on a
    /// computer whose service Home Manager runs (captured on Atlas's
    /// development machine): the app uses it, and never starts or stops it.
    #[test]
    fn reads_an_older_ahab_run_by_home_manager() {
        let _turn = one_at_a_time();
        let status = json!({"mode":"the server and this computer's daemon","service":{"manager":"systemd","file":"/home/me/.config/systemd/user/ahab.service","managed":true,"installed":true,"enabled":true,"running":true,"since":"Wed 2026-11-18 09:29:12 UTC"},"server":{"url":"http://127.0.0.1:8185","version":"0.1.0"},"machine":{"note":"the server does not answer"}});
        let computer = interpret(&status, None, None);
        assert_eq!(computer.role, Role::Server);
        assert!(computer.managed && computer.running && computer.has_ahab());
        assert_eq!(computer.url.as_deref(), Some("http://127.0.0.1:8185"));
    }

    /// This computer's real Ahab, read only (`ahab status`): run with
    /// `--ignored` to see what the app would find here.
    #[test]
    #[ignore = "reads this computer's own Ahab"]
    fn detects_this_computers_ahab_read_only() {
        let computer = Computer::new(Env::process(), None).detect();
        println!("{computer:#?}");
        assert!(computer.installed.is_some(), "{computer:?}");
    }

    #[test]
    fn reads_a_machine_of_another_server_from_older_ahabs_too() {
        let _turn = one_at_a_time();
        let status = json!({
            "mode": "this computer's daemon, joined to https://atlas.lan",
            "service": { "manager": "systemd", "installed": true, "running": true, "managed": false },
            "server": { "url": "https://atlas.lan", "version": "0.1.0" },
        });
        let computer = interpret(&status, None, None);
        assert_eq!(computer.role, Role::Machine);
        assert_eq!(computer.joined.as_deref(), Some("https://atlas.lan"));
        assert_eq!(computer.url, None);
    }

    #[test]
    fn reads_a_computer_without_ahab() {
        let _turn = one_at_a_time();
        let status = json!({
            "mode": "the server and this computer's daemon",
            "role": "server",
            "service": { "manager": "systemd", "installed": false, "running": false, "managed": false },
            "server": { "url": "http://127.0.0.1:8180", "version": null },
        });
        let computer = interpret(&status, None, None);
        assert_eq!(computer.role, Role::None);
        assert!(!computer.has_ahab());
        assert_eq!(interpret(&Value::Null, None, None).role, Role::None);
    }

    #[test]
    fn a_running_server_without_a_service_still_counts() {
        let _turn = one_at_a_time();
        // A server started by hand (`ahab up --foreground`), or in a
        // container: the app uses it but cannot stop it.
        let status = json!({
            "role": "server", "service": null,
            "server": { "url": "http://127.0.0.1:8180", "version": "0.1.0" },
        });
        let computer = interpret(&status, None, None);
        assert_eq!(computer.role, Role::Server);
        assert_eq!(computer.service_manager, None);
    }

    #[test]
    fn finds_an_installed_ahab_on_path_or_in_local_bin() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let none = env(home.path(), &bin.display().to_string());
        assert_eq!(none.installed_ahab(), None);

        script(&home.path().join(".local/bin/ahab"), "#!/bin/sh\n");
        assert_eq!(
            none.installed_ahab(),
            Some(home.path().join(".local/bin/ahab"))
        );
        script(&bin.join("ahab"), "#!/bin/sh\n");
        assert_eq!(none.installed_ahab(), Some(bin.join("ahab")));

        // Not the copy inside the app's own AppImage.
        let appimage = Env::isolated(vec![
            ("HOME".into(), home.path().display().to_string()),
            ("PATH".into(), bin.display().to_string()),
            ("APPDIR".into(), bin.display().to_string()),
        ]);
        assert_eq!(
            appimage.installed_ahab(),
            Some(home.path().join(".local/bin/ahab"))
        );
    }

    #[test]
    fn resolves_the_data_folder_as_ahab_does() {
        let _turn = one_at_a_time();
        let env = Env::isolated(vec![("HOME".into(), "/home/me".into())]);
        assert_eq!(
            env.data_dir(),
            Some(PathBuf::from("/home/me/.local/share/ahab"))
        );
        let env = Env::isolated(vec![
            ("HOME".into(), "/home/me".into()),
            ("XDG_DATA_HOME".into(), "/data".into()),
        ]);
        assert_eq!(env.data_dir(), Some(PathBuf::from("/data/ahab")));
        let env = Env::isolated(vec![
            ("HOME".into(), "/home/me".into()),
            ("AHAB_DATA_DIR".into(), "/srv/ahab".into()),
        ]);
        assert_eq!(env.data_dir(), Some(PathBuf::from("/srv/ahab")));
    }

    /// A pretend `ahab` that logs its arguments and answers `status` from
    /// a file.
    fn fake_ahab(path: &Path, log: &Path, status: &Path) {
        script(
            path,
            &format!(
                "#!/bin/sh\necho \"$*\" >> '{log}'\n\
                 case \"$*\" in\n\
                 --version) echo 'ahab 0.1.0';;\n\
                 '--json status') while IFS= read -r line || [ -n \"$line\" ]; do printf '%s\\n' \"$line\"; done < '{status}';;\n\
                 '--json up') echo '{{\"running\":true}}';;\n\
                 '--json setup'*) echo '{{\"title\":\"Not joined\",\"code\":\"fingerprint_mismatch\"}}' >&2; exit 3;;\n\
                 esac\n",
                log = log.display(),
                status = status.display()
            ),
        );
    }

    #[test]
    fn installs_the_bundled_ahab_once_and_starts_it_with_up() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let log = home.path().join("log");
        let status = home.path().join("status.json");
        std::fs::write(
            &status,
            json!({ "service": { "manager": "systemd", "installed": false, "running": false } })
                .to_string(),
        )
        .unwrap();
        let bundled = home.path().join("app/ahab");
        fake_ahab(&bundled, &log, &status);
        let computer = Computer::new(env(home.path(), "/nonexistent"), Some(bundled));

        let before = computer.detect();
        assert_eq!(before.role, Role::None);
        assert_eq!(before.installed, None);
        assert_eq!(
            before
                .bundled
                .as_ref()
                .and_then(|binary| binary.version.as_deref()),
            Some("0.1.0")
        );

        computer.start().unwrap();
        let installed = home.path().join(".local/bin/ahab");
        assert!(is_executable(&installed));
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains("--json up"), "{calls}");

        // Again: nothing copied over the installed one.
        std::fs::write(&installed, "#!/bin/sh\necho mine\n").unwrap();
        assert_eq!(computer.install().unwrap(), installed);
        assert_eq!(
            std::fs::read_to_string(&installed).unwrap(),
            "#!/bin/sh\necho mine\n"
        );
    }

    #[test]
    fn never_runs_the_server_itself_without_a_service_manager() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let log = home.path().join("log");
        let status = home.path().join("status.json");
        std::fs::write(&status, json!({ "service": null }).to_string()).unwrap();
        let bundled = home.path().join("app/ahab");
        fake_ahab(&bundled, &log, &status);
        let computer = Computer::new(env(home.path(), "/nonexistent"), Some(bundled));
        let error = computer.start().unwrap_err();
        assert_eq!(error.code(), "no_service_manager");
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(!calls.contains("up"), "{calls}");
    }

    #[test]
    fn leaves_a_managed_service_alone() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let log = home.path().join("log");
        let status = home.path().join("status.json");
        std::fs::write(
            &status,
            json!({ "role": "server", "service": { "manager": "systemd", "managed": true,
                    "installed": true, "running": false } })
            .to_string(),
        )
        .unwrap();
        let installed = home.path().join(".local/bin/ahab");
        fake_ahab(&installed, &log, &status);
        let computer = Computer::new(env(home.path(), "/nonexistent"), None);
        assert_eq!(computer.start().unwrap_err().code(), "managed");
        assert_eq!(computer.stop().unwrap_err().code(), "managed");
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(!calls.contains("up") && !calls.contains("down"), "{calls}");
    }

    #[test]
    fn joins_with_the_pinned_fingerprint_and_reports_its_problem() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let log = home.path().join("log");
        let status = home.path().join("status.json");
        std::fs::write(&status, "{}").unwrap();
        let installed = home.path().join(".local/bin/ahab");
        fake_ahab(&installed, &log, &status);
        let computer = Computer::new(env(home.path(), "/nonexistent"), None);
        let error = computer
            .join("https://atlas.lan", "ABCD-EFGH", Some("SHA256:x"), false)
            .unwrap_err();
        assert_eq!(error.code(), "fingerprint_mismatch");
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(
            calls.contains(
                "--json setup --server https://atlas.lan --code ABCD-EFGH --server-fingerprint SHA256:x"
            ),
            "{calls}"
        );
        assert!(!calls.contains("--yes"), "{calls}");
    }

    #[test]
    fn reads_the_local_credential() {
        let _turn = one_at_a_time();
        let home = tempfile::tempdir().unwrap();
        let computer = Computer::new(env(home.path(), ""), None);
        assert_eq!(
            computer.local_credential().unwrap_err().code(),
            "no_credential"
        );
        let dir = home.path().join(".local/share/ahab");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("credential"), "ahab_local\n").unwrap();
        assert_eq!(computer.local_credential().unwrap(), "ahab_local");
    }
}
