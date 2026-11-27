//! Running Ahab in the background as a login service of the user: a systemd
//! user unit on Linux, a `LaunchAgent` on macOS. The
//! service runs `ahab up --foreground`.
//!
//! A service file `ahab up` wrote carries [`MARKER`]. One without it was
//! written by a system configuration (NixOS, Home Manager) or by hand: Ahab
//! is then *managed*, and `up` and `down` leave the service alone.

mod launchd;
mod systemd;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub use launchd::Launchd;
pub use systemd::Systemd;

/// In every service file `ahab up` writes.
pub const MARKER: &str = "Written by `ahab up`";

/// Runs the service manager's commands; tests replace it.
pub trait Runner: Send + Sync {
    /// Runs `program` with `args` and returns whether it succeeded and what it
    /// printed.
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Ran>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ran {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The real commands.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Ran> {
        let output = Command::new(program).args(args).output()?;
        Ok(Ran {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// What the service runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    /// The `ahab` binary.
    pub exe: PathBuf,
    /// Environment the service needs, such as `AHAB_CONFIG_DIR` when this
    /// `ahab up` used one.
    pub env: Vec<(String, String)>,
    pub description: String,
}

/// What the service manager says about the service.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceState {
    pub installed: bool,
    /// Starts at login.
    pub enabled: bool,
    pub running: bool,
    /// When it started, as the service manager says it.
    pub since: Option<String>,
}

/// A service manager Ahab can use.
pub enum Backend {
    Systemd(Systemd),
    Launchd(Launchd),
}

impl Backend {
    /// The service manager of this computer, when there is one Ahab can use:
    /// a reachable systemd user manager, or launchd on macOS. None inside
    /// most containers.
    pub fn detect(runner: &dyn Runner) -> Option<Self> {
        let home = directories::BaseDirs::new()?;
        if cfg!(target_os = "macos") {
            return Some(Self::Launchd(Launchd::new(
                home.home_dir(),
                rustix::process::getuid().as_raw(),
            )));
        }
        let reachable = runner
            .run("systemctl", &["--user", "show-environment"])
            .is_ok_and(|ran| ran.success);
        reachable.then(|| Self::Systemd(Systemd::new(home.config_dir())))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Systemd(_) => "systemd",
            Self::Launchd(_) => "launchd",
        }
    }

    /// The service file.
    pub fn path(&self) -> PathBuf {
        match self {
            Self::Systemd(systemd) => systemd.path(),
            Self::Launchd(launchd) => launchd.path(),
        }
    }

    /// The service file's content for `spec`.
    pub fn definition(&self, spec: &ServiceSpec) -> String {
        match self {
            Self::Systemd(_) => systemd::unit(spec),
            Self::Launchd(launchd) => launchd.plist(spec),
        }
    }

    /// Whether a service file exists that `ahab up` did not write.
    pub fn is_managed(&self) -> bool {
        is_managed(&self.path())
    }

    /// Writes the service file when it differs; whether it changed.
    pub fn install(&self, runner: &dyn Runner, spec: &ServiceSpec) -> io::Result<bool> {
        let path = self.path();
        let content = self.definition(spec);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(content.as_str()) {
            return Ok(false);
        }
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&path, content)?;
        match self {
            Self::Systemd(_) => systemd::reload(runner)?,
            Self::Launchd(_) => {}
        }
        Ok(true)
    }

    /// Starts it at login from now on, and now; restarts it when `restart`.
    pub fn start(&self, runner: &dyn Runner, restart: bool) -> io::Result<()> {
        match self {
            Self::Systemd(_) => systemd::start(runner, restart),
            Self::Launchd(launchd) => launchd.start(runner, restart),
        }
    }

    /// Stops it, and no longer starts it at login.
    pub fn stop(&self, runner: &dyn Runner) -> io::Result<()> {
        match self {
            Self::Systemd(_) => systemd::stop(runner),
            Self::Launchd(launchd) => launchd.stop(runner),
        }
    }

    /// Deletes the service file (after `stop`).
    pub fn remove(&self, runner: &dyn Runner) -> io::Result<bool> {
        let path = self.path();
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path)?;
        if let Self::Systemd(_) = self {
            systemd::reload(runner)?;
        }
        Ok(true)
    }

    pub fn state(&self, runner: &dyn Runner) -> ServiceState {
        match self {
            Self::Systemd(systemd) => systemd.state(runner),
            Self::Launchd(launchd) => launchd.state(runner),
        }
    }

    /// The command that shows the service's logs.
    pub fn logs_command(&self, follow: bool, lines: u32) -> (String, Vec<String>) {
        match self {
            Self::Systemd(_) => systemd::logs_command(follow, lines),
            Self::Launchd(launchd) => launchd.logs_command(follow, lines),
        }
    }

    /// How to restart a managed service, to tell people.
    pub fn managed_hint(&self) -> &'static str {
        match self {
            Self::Systemd(_) => "systemctl --user restart ahab",
            Self::Launchd(_) => "launchctl kickstart -k gui/$UID/dev.ahab.ahab",
        }
    }

    /// How to stop a managed service, to tell people.
    pub fn managed_stop_hint(&self) -> &'static str {
        match self {
            Self::Systemd(_) => "systemctl --user stop ahab",
            Self::Launchd(_) => "launchctl bootout gui/$UID/dev.ahab.ahab",
        }
    }
}

fn is_managed(path: &Path) -> bool {
    if std::env::var_os("AHAB_MANAGED").is_some_and(|value| value == "1") {
        return true;
    }
    match std::fs::read_to_string(path) {
        Ok(content) => !content.contains(MARKER),
        Err(_) => false,
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records the commands it is given and answers from a script.
    #[derive(Default)]
    pub struct FakeRunner {
        pub calls: Mutex<Vec<String>>,
        /// (command prefix, answer); the first match answers.
        pub answers: Vec<(String, Ran)>,
    }

    impl Runner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<Ran> {
            let call = std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            let answer = self
                .answers
                .iter()
                .find(|(prefix, _)| call.starts_with(prefix.as_str()))
                .map_or_else(
                    || Ran {
                        success: true,
                        ..Ran::default()
                    },
                    |(_, ran)| ran.clone(),
                );
            self.calls.lock().unwrap().push(call);
            Ok(answer)
        }
    }

    impl FakeRunner {
        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    pub fn spec() -> ServiceSpec {
        ServiceSpec {
            exe: PathBuf::from("/home/me/.local/bin/ahab"),
            env: vec![("AHAB_CONFIG_DIR".into(), "/home/me/ahab-config".into())],
            description: "Ahab: the server and this computer's daemon".into(),
        }
    }

    #[test]
    fn installs_a_service_file_once_and_knows_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Backend::Systemd(Systemd::new(dir.path()));
        let runner = FakeRunner::default();

        assert!(backend.install(&runner, &spec()).unwrap());
        assert!(!backend.install(&runner, &spec()).unwrap());
        assert!(!backend.is_managed());
        assert_eq!(runner.calls(), ["systemctl --user daemon-reload"]);

        std::fs::write(
            backend.path(),
            "[Service]\nExecStart=/nix/store/x/bin/ahab up --foreground\n",
        )
        .unwrap();
        assert!(backend.is_managed());

        assert!(backend.remove(&runner).unwrap());
        assert!(!backend.remove(&runner).unwrap());
    }
}
