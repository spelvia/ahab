//! A systemd user unit: `~/.config/systemd/user/ahab.service`.

use std::io;
use std::path::{Path, PathBuf};

use super::{MARKER, Runner, ServiceSpec, ServiceState};

pub struct Systemd {
    unit_dir: PathBuf,
}

impl Systemd {
    /// Units in `config_dir`/systemd/user (`$XDG_CONFIG_HOME`).
    pub fn new(config_dir: &Path) -> Self {
        Self {
            unit_dir: config_dir.join("systemd/user"),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.unit_dir.join("ahab.service")
    }

    pub fn state(&self, runner: &dyn Runner) -> ServiceState {
        let show = runner
            .run(
                "systemctl",
                &[
                    "--user",
                    "show",
                    "ahab",
                    "--property=LoadState,UnitFileState,ActiveState,ActiveEnterTimestamp",
                ],
            )
            .map(|ran| ran.stdout)
            .unwrap_or_default();
        let value = |key: &str| {
            show.lines()
                .find_map(|line| line.strip_prefix(&format!("{key}=")))
                .unwrap_or_default()
                .to_owned()
        };
        let running = value("ActiveState") == "active";
        ServiceState {
            installed: value("LoadState") == "loaded",
            enabled: value("UnitFileState") == "enabled",
            running,
            since: running
                .then(|| value("ActiveEnterTimestamp"))
                .filter(|since| !since.is_empty()),
        }
    }
}

/// The unit file. Values with spaces are quoted, as systemd reads them.
pub fn unit(spec: &ServiceSpec) -> String {
    let mut environment = String::new();
    for (name, value) in &spec.env {
        environment.push_str(&format!("Environment=\"{name}={value}\"\n"));
    }
    format!(
        "# {MARKER}. Changes are overwritten by the next `ahab up`; to manage the\n\
         # service yourself, remove this line.\n\
         [Unit]\n\
         Description={description}\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         ExecStart=\"{exe}\" up --foreground\n\
         {environment}\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        description = spec.description,
        exe = spec.exe.display(),
    )
}

pub fn reload(runner: &dyn Runner) -> io::Result<()> {
    checked(runner, &["--user", "daemon-reload"])
}

pub fn start(runner: &dyn Runner, restart: bool) -> io::Result<()> {
    checked(runner, &["--user", "enable", "ahab"])?;
    if restart {
        checked(runner, &["--user", "restart", "ahab"])
    } else {
        checked(runner, &["--user", "start", "ahab"])
    }
}

pub fn stop(runner: &dyn Runner) -> io::Result<()> {
    checked(runner, &["--user", "disable", "--now", "ahab"])
}

pub fn logs_command(follow: bool, lines: u32) -> (String, Vec<String>) {
    let mut args = vec![
        "--user".to_owned(),
        "--unit".to_owned(),
        "ahab".to_owned(),
        "--lines".to_owned(),
        lines.to_string(),
        "--no-pager".to_owned(),
    ];
    if follow {
        args.push("--follow".to_owned());
    }
    ("journalctl".to_owned(), args)
}

fn checked(runner: &dyn Runner, args: &[&str]) -> io::Result<()> {
    let ran = runner.run("systemctl", args)?;
    if ran.success {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "systemctl {} failed: {}",
            args.join(" "),
            ran.stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Ran;
    use super::super::tests::{FakeRunner, spec};
    use super::*;

    #[test]
    fn writes_a_unit_that_runs_ahab_up_in_the_foreground() {
        let unit = unit(&spec());
        assert!(unit.starts_with("# Written by `ahab up`"));
        assert!(unit.contains("ExecStart=\"/home/me/.local/bin/ahab\" up --foreground\n"));
        assert!(unit.contains("Environment=\"AHAB_CONFIG_DIR=/home/me/ahab-config\"\n"));
        assert!(unit.contains("Restart=on-failure\n"));
        assert!(unit.contains("WantedBy=default.target\n"));
    }

    #[test]
    fn enables_and_starts_or_restarts() {
        let runner = FakeRunner::default();
        start(&runner, false).unwrap();
        start(&runner, true).unwrap();
        stop(&runner).unwrap();
        assert_eq!(
            runner.calls(),
            [
                "systemctl --user enable ahab",
                "systemctl --user start ahab",
                "systemctl --user enable ahab",
                "systemctl --user restart ahab",
                "systemctl --user disable --now ahab",
            ]
        );
    }

    #[test]
    fn reports_a_failing_command() {
        let runner = FakeRunner {
            answers: vec![(
                "systemctl --user enable".into(),
                Ran {
                    success: false,
                    stderr: "Unit not found".into(),
                    ..Ran::default()
                },
            )],
            ..FakeRunner::default()
        };
        let error = start(&runner, false).unwrap_err();
        assert!(error.to_string().contains("Unit not found"));
    }

    #[test]
    fn reads_the_state() {
        let runner = FakeRunner {
            answers: vec![(
                "systemctl --user show".into(),
                Ran {
                    success: true,
                    stdout: "LoadState=loaded\nUnitFileState=enabled\nActiveState=active\nActiveEnterTimestamp=Fri 2026-11-13 19:45:00 UTC\n".into(),
                    ..Ran::default()
                },
            )],
            ..FakeRunner::default()
        };
        let state = Systemd::new(Path::new("/x")).state(&runner);
        assert!(state.installed && state.enabled && state.running);
        assert_eq!(state.since.as_deref(), Some("Fri 2026-11-13 19:45:00 UTC"));
    }
}
