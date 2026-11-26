//! A `LaunchAgent` on macOS: `~/Library/LaunchAgents/dev.ahab.ahab.plist`,
//! logging to `~/Library/Logs/ahab/ahab.log`.

use std::io;
use std::path::{Path, PathBuf};

use super::{MARKER, Runner, ServiceSpec, ServiceState};

const LABEL: &str = "dev.ahab.ahab";

pub struct Launchd {
    home: PathBuf,
    uid: u32,
}

impl Launchd {
    pub fn new(home: &Path, uid: u32) -> Self {
        Self {
            home: home.to_owned(),
            uid,
        }
    }

    pub fn path(&self) -> PathBuf {
        self.home
            .join(format!("Library/LaunchAgents/{LABEL}.plist"))
    }

    fn log_file(&self) -> PathBuf {
        self.home.join("Library/Logs/ahab/ahab.log")
    }

    fn target(&self) -> String {
        format!("gui/{}/{LABEL}", self.uid)
    }

    pub fn plist(&self, spec: &ServiceSpec) -> String {
        let mut environment = String::new();
        if !spec.env.is_empty() {
            environment.push_str("  <key>EnvironmentVariables</key>\n  <dict>\n");
            for (name, value) in &spec.env {
                environment.push_str(&format!(
                    "    <key>{}</key><string>{}</string>\n",
                    escape(name),
                    escape(value)
                ));
            }
            environment.push_str("  </dict>\n");
        }
        let log = escape(&self.log_file().display().to_string());
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!-- {MARKER}. Changes are overwritten by the next `ahab up`. -->\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \x20 <key>Label</key><string>{LABEL}</string>\n\
             \x20 <key>ProgramArguments</key>\n\
             \x20 <array><string>{exe}</string><string>up</string><string>--foreground</string></array>\n\
             {environment}\
             \x20 <key>RunAtLoad</key><true/>\n\
             \x20 <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n\
             \x20 <key>StandardOutPath</key><string>{log}</string>\n\
             \x20 <key>StandardErrorPath</key><string>{log}</string>\n\
             </dict>\n\
             </plist>\n",
            exe = escape(&spec.exe.display().to_string()),
        )
    }

    pub fn start(&self, runner: &dyn Runner, restart: bool) -> io::Result<()> {
        if let Some(folder) = self.log_file().parent() {
            std::fs::create_dir_all(folder)?;
        }
        let path = self.path().display().to_string();
        let domain = format!("gui/{}", self.uid);
        // Loaded already: bootstrap fails, and a restart picks up the file.
        if !self.state(runner).installed {
            checked(runner, &["bootstrap", &domain, &path])?;
        } else if restart {
            let _ = runner.run("launchctl", &["bootout", &self.target()]);
            checked(runner, &["bootstrap", &domain, &path])?;
        }
        checked(runner, &["enable", &self.target()])?;
        checked(runner, &["kickstart", &self.target()])
    }

    pub fn stop(&self, runner: &dyn Runner) -> io::Result<()> {
        let _ = runner.run("launchctl", &["disable", &self.target()]);
        let ran = runner.run("launchctl", &["bootout", &self.target()])?;
        // Not loaded is stopped too.
        if ran.success || ran.stderr.contains("No such process") {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "launchctl bootout failed: {}",
                ran.stderr.trim()
            )))
        }
    }

    pub fn state(&self, runner: &dyn Runner) -> ServiceState {
        let print = runner.run("launchctl", &["print", &self.target()]);
        match print {
            Ok(ran) if ran.success => {
                let running = ran.stdout.contains("state = running");
                let disabled = runner
                    .run(
                        "launchctl",
                        &["print-disabled", &format!("gui/{}", self.uid)],
                    )
                    .is_ok_and(|ran| ran.stdout.contains(&format!("\"{LABEL}\" => disabled")));
                ServiceState {
                    installed: true,
                    enabled: !disabled,
                    running,
                    since: None,
                }
            }
            _ => ServiceState {
                installed: false,
                ..ServiceState::default()
            },
        }
    }

    pub fn logs_command(&self, follow: bool, lines: u32) -> (String, Vec<String>) {
        let mut args = vec!["-n".to_owned(), lines.to_string()];
        if follow {
            args.push("-F".to_owned());
        }
        args.push(self.log_file().display().to_string());
        ("tail".to_owned(), args)
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn checked(runner: &dyn Runner, args: &[&str]) -> io::Result<()> {
    let ran = runner.run("launchctl", args)?;
    if ran.success {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "launchctl {} failed: {}",
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

    fn launchd() -> Launchd {
        Launchd::new(Path::new("/Users/me"), 501)
    }

    #[test]
    fn writes_a_launch_agent_that_runs_ahab_up_in_the_foreground() {
        let plist = launchd().plist(&spec());
        assert!(plist.contains("<!-- Written by `ahab up`"));
        assert!(plist.contains("<key>Label</key><string>dev.ahab.ahab</string>"));
        assert!(plist.contains(
            "<array><string>/home/me/.local/bin/ahab</string><string>up</string><string>--foreground</string></array>"
        ));
        assert!(plist.contains("<key>AHAB_CONFIG_DIR</key><string>/home/me/ahab-config</string>"));
        assert!(plist.contains("<string>/Users/me/Library/Logs/ahab/ahab.log</string>"));
        assert_eq!(
            launchd().path(),
            Path::new("/Users/me/Library/LaunchAgents/dev.ahab.ahab.plist")
        );
    }

    #[test]
    fn loads_enables_and_starts_an_agent_that_is_not_loaded() {
        let runner = FakeRunner {
            answers: vec![(
                "launchctl print gui/501/dev.ahab.ahab".into(),
                Ran {
                    success: false,
                    ..Ran::default()
                },
            )],
            ..FakeRunner::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let launchd = Launchd::new(dir.path(), 501);
        launchd.start(&runner, false).unwrap();
        let calls = runner.calls();
        assert!(calls[1].starts_with("launchctl bootstrap gui/501 "));
        assert_eq!(calls[2], "launchctl enable gui/501/dev.ahab.ahab");
        assert_eq!(calls[3], "launchctl kickstart gui/501/dev.ahab.ahab");
    }

    #[test]
    fn follows_the_log_file() {
        let (program, args) = launchd().logs_command(true, 50);
        assert_eq!(program, "tail");
        assert_eq!(
            args,
            ["-n", "50", "-F", "/Users/me/Library/Logs/ahab/ahab.log"]
        );
    }
}
