//! The system's `ssh` and `scp`, so the person's own SSH
//! setup applies: `~/.ssh/config`, agents, keys, `ProxyJump`. Ahab never asks
//! for or handles a password: every call runs in `BatchMode`, so a host that
//! needs interaction (a password, a passphrase, an unknown host key) fails
//! at once, and the error says what to do.
//!
//! Tests replace the programs with `AHAB_TEST_SSH` and `AHAB_TEST_SCP`.

use std::path::Path;
use std::process::{Command, Stdio};

/// The variables tests use to replace `ssh` and `scp`.
pub const TEST_SSH: &str = "AHAB_TEST_SSH";
pub const TEST_SCP: &str = "AHAB_TEST_SCP";

/// Where to connect: `you@host`, `host`, or `ssh://you@host:2222`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// `you@host` or `host`, as `ssh` takes it.
    pub target: String,
    pub port: Option<u16>,
}

impl Destination {
    /// Reads a destination; `None` when it is not one.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (rest, from_url) = match text.strip_prefix("ssh://") {
            Some(rest) => (rest.trim_end_matches('/'), true),
            None => (text, false),
        };
        let (target, port) = match rest.rsplit_once(':') {
            Some((target, port))
                if from_url && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) =>
            {
                (target, Some(port.parse().ok()?))
            }
            _ => (rest, None),
        };
        // `ssh` takes an IPv6 address without brackets.
        let target = target.replace(['[', ']'], "");
        let host = target.rsplit('@').next().unwrap_or_default();
        let valid = !host.is_empty()
            && !target.starts_with('-')
            && !target.contains(['/', ' ', '\'', '"', '\\'])
            && (from_url || !target.contains(':'));
        valid.then_some(Self { target, port })
    }

    /// The host part, without the user.
    pub fn host(&self) -> &str {
        self.target.rsplit('@').next().unwrap_or(&self.target)
    }
}

impl std::fmt::Display for Destination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.port {
            Some(port) => write!(f, "ssh://{}:{port}", self.target),
            None => f.write_str(&self.target),
        }
    }
}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshError {
    /// `ssh` or `scp` is not installed.
    #[error("{0} is not installed here, or cannot be started: {1}")]
    Missing(String, String),
    /// The host wants a password, a passphrase or a host key accepted.
    #[error("{0}")]
    NeedsInteraction(String),
    /// The host cannot be reached.
    #[error("{0}")]
    Unreachable(String),
    /// Anything else `ssh` or `scp` reported.
    #[error("{0}")]
    Failed(String),
}

/// What a command on the host printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ran {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// A connection to one host.
#[derive(Debug, Clone)]
pub struct Ssh {
    destination: Destination,
    ssh: String,
    scp: String,
}

/// Options for every call: no questions, and no endless wait.
const OPTIONS: [&str; 4] = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=20"];

impl Ssh {
    pub fn new(destination: Destination) -> Self {
        let program = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        Self {
            destination,
            ssh: program(TEST_SSH, "ssh"),
            scp: program(TEST_SCP, "scp"),
        }
    }

    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Runs `script` with `sh` on the host (whatever the login shell is),
    /// without standard input. A command that ran and failed is a `Ran`
    /// with `success` false; only `ssh` failing to connect is an error.
    pub fn run(&self, script: &str) -> Result<Ran, SshError> {
        let mut command = Command::new(&self.ssh);
        command.args(OPTIONS);
        if let Some(port) = self.destination.port {
            command.args(["-p", &port.to_string()]);
        }
        command
            .args(["-T", "--", &self.destination.target])
            .arg(format!("sh -c {}", quote(script)));
        let ran = output(&self.ssh, &mut command)?;
        // 255 is ssh's own failure; a command on the host rarely uses it.
        if ran.code == Some(255) {
            return Err(classify(&self.destination, &ran.stderr));
        }
        Ok(ran)
    }

    /// Copies the local file `from` to `to` on the host, relative to the
    /// remote user's home folder.
    pub fn copy(&self, from: &Path, to: &str) -> Result<(), SshError> {
        let mut command = Command::new(&self.scp);
        command.args(OPTIONS).arg("-q");
        if let Some(port) = self.destination.port {
            command.args(["-P", &port.to_string()]);
        }
        let host = &self.destination.target;
        let remote = if host.rsplit('@').next().is_some_and(|h| h.contains(':')) {
            // An IPv6 address.
            let (user, address) = match host.rsplit_once('@') {
                Some((user, address)) => (format!("{user}@"), address),
                None => (String::new(), host.as_str()),
            };
            format!("{user}[{address}]:{to}")
        } else {
            format!("{host}:{to}")
        };
        command.arg("--").arg(from).arg(remote);
        let ran = output(&self.scp, &mut command)?;
        if ran.success {
            Ok(())
        } else {
            Err(classify(&self.destination, &ran.stderr))
        }
    }

    /// The host name `ssh` connects to after `~/.ssh/config` (`HostName`),
    /// if it says.
    pub fn resolved_host(&self) -> Option<String> {
        let mut command = Command::new(&self.ssh);
        command.args(["-G", "--", &self.destination.target]);
        let ran = output(&self.ssh, &mut command).ok()?;
        ran.stdout.lines().find_map(|line| {
            line.strip_prefix("hostname ")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
    }
}

fn output(program: &str, command: &mut Command) -> Result<Ran, SshError> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| SshError::Missing(program.to_owned(), error.to_string()))?;
    Ok(Ran {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// What `ssh`'s complaint means, with what to do about it.
fn classify(destination: &Destination, stderr: &str) -> SshError {
    let said = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("Warning: Permanently added"))
        .collect::<Vec<_>>()
        .join(" ");
    let said = if said.is_empty() {
        "ssh failed without saying why".to_owned()
    } else {
        said
    };
    let lower = said.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    if has(&[
        "host key verification failed",
        "remote host identification has changed",
    ]) {
        return SshError::NeedsInteraction(format!(
            "{said}. SSH does not know {host}'s host key (or it changed). Connect once with \
             `ssh {destination}` to check and accept it, then run this again.",
            host = destination.host(),
        ));
    }
    if has(&[
        "permission denied",
        "passphrase",
        "no more authentication methods",
        "keyboard-interactive",
        "too many authentication failures",
    ]) {
        return SshError::NeedsInteraction(format!(
            "{said}. Ahab connects without asking for passwords: {destination} must accept \
             your SSH key without a question. Load the key into your SSH agent (`ssh-add`) or \
             set it up in ~/.ssh/config, check that `ssh {destination} true` works without a \
             prompt, then run this again.",
        ));
    }
    if has(&[
        "could not resolve",
        "connection refused",
        "timed out",
        "no route to host",
        "network is unreachable",
        "connection closed",
        "connection reset",
    ]) {
        return SshError::Unreachable(format!(
            "{said}. Check that {host} is on and reachable, and that `ssh {destination} true` \
             works.",
            host = destination.host(),
        ));
    }
    SshError::Failed(said)
}

/// `text` as one word for a POSIX shell.
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@,+%".contains(c))
    {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_destinations() {
        let parse = |text: &str| Destination::parse(text);
        assert_eq!(
            parse("me@build-box"),
            Some(Destination {
                target: "me@build-box".into(),
                port: None
            })
        );
        assert_eq!(parse("build-box").unwrap().host(), "build-box");
        let url = parse("ssh://me@build-box:2222").unwrap();
        assert_eq!(
            (url.target.as_str(), url.port),
            ("me@build-box", Some(2222))
        );
        assert_eq!(url.to_string(), "ssh://me@build-box:2222");
        assert_eq!(parse("ssh://me@build-box/").unwrap().port, None);
        assert_eq!(parse("ssh://me@[fd00::1]:22").unwrap().host(), "fd00::1");
        for wrong in ["", "-oProxyCommand=x", "me@", "a b", "host:/path", "x'y"] {
            assert_eq!(parse(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn quotes_for_the_shell() {
        assert_eq!(quote("plain-word_1.0"), "plain-word_1.0");
        assert_eq!(
            quote("https://ahab.example:8180"),
            "https://ahab.example:8180"
        );
        assert_eq!(quote("two words"), "'two words'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn says_what_to_do_when_ssh_fails() {
        let destination = Destination::parse("me@box").unwrap();
        let denied = classify(&destination, "me@box: Permission denied (publickey).\n");
        assert!(matches!(&denied, SshError::NeedsInteraction(text) if text.contains("ssh-add")));
        let unknown = classify(&destination, "Host key verification failed.\r\n");
        assert!(
            matches!(&unknown, SshError::NeedsInteraction(text) if text.contains("Connect once"))
        );
        let gone = classify(
            &destination,
            "ssh: Could not resolve hostname box: Name or service not known\n",
        );
        assert!(matches!(gone, SshError::Unreachable(_)));
        assert_eq!(
            classify(&destination, "something odd\n"),
            SshError::Failed("something odd".into())
        );
    }
}
