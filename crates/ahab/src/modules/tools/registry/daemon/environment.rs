//! The environment tools start with: the machine's login shell,
//! then the project's direnv, mise or Nix dev shell.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Variables, by name.
pub type Vars = BTreeMap<String, String>;

/// How long the login shell's environment is reused (as Stubb does for
/// agents): one shell for many lookups, a new version manager picked up
/// without a restart.
const LOGIN_FRESH: Duration = Duration::from_secs(300);
/// How long a login shell, direnv or mise may take.
const SHELL_TIMEOUT: Duration = Duration::from_secs(30);
/// `nix develop` may build the shell the first time.
const NIX_TIMEOUT: Duration = Duration::from_secs(600);
/// A project's environment is asked again after this, or when its file
/// changes.
const PROJECT_FRESH: Duration = Duration::from_secs(300);

const MARKER: &str = "__ahab_env__";

/// Where a project's environment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvironmentKind {
    Direnv,
    Mise,
    Nix,
}

/// What a tool starts with, and how it was found.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    pub vars: Arc<Vars>,
    pub kind: Option<EnvironmentKind>,
    /// The file that made it (`.envrc`, `.mise.toml`, `flake.nix`).
    pub file: Option<PathBuf>,
    /// Something to say (an `.envrc` that is not allowed).
    pub note: Option<String>,
}

impl Environment {
    /// Its `PATH`.
    pub fn path(&self) -> Option<&str> {
        self.vars.get("PATH").map(String::as_str)
    }
}

/// Which environments a project may use.
#[derive(Debug, Clone, Copy)]
pub struct Allowed {
    pub direnv: bool,
    pub mise: bool,
    pub nix: bool,
}

type ProjectKey = (PathBuf, bool, bool, bool);
/// A project's environment, when it was found, and its file's time then.
type Found = (Instant, Option<SystemTime>, Environment);

/// The environments this daemon found, kept a while.
pub struct Environments {
    /// Ask the login shell; off in tests, which use the daemon's own.
    login_shell: bool,
    /// Where `nix develop`'s environments are kept, per flake lock.
    cache_dir: PathBuf,
    login: tokio::sync::Mutex<Option<(Instant, Arc<Vars>)>>,
    projects: tokio::sync::Mutex<HashMap<ProjectKey, Found>>,
}

impl Environments {
    pub fn new(login_shell: bool, cache_dir: PathBuf) -> Self {
        Self {
            login_shell,
            cache_dir,
            login: tokio::sync::Mutex::new(None),
            projects: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// The machine's: the login shell's variables, else the daemon's own.
    pub async fn machine(&self) -> Arc<Vars> {
        let mut cached = self.login.lock().await;
        if let Some((at, vars)) = cached.as_ref()
            && at.elapsed() < LOGIN_FRESH
        {
            return Arc::clone(vars);
        }
        let mut vars: Vars = std::env::vars().collect();
        if self.login_shell
            && let Some(login) = login_shell().await
        {
            vars = login;
        }
        let vars = Arc::new(vars);
        *cached = Some((Instant::now(), Arc::clone(&vars)));
        vars
    }

    /// The project's at `root`: direnv, else mise, else (when allowed) a
    /// Nix dev shell; else the machine's.
    pub async fn project(&self, root: &Path, top: &Path, allowed: Allowed) -> Environment {
        let key = (
            root.to_path_buf(),
            allowed.direnv,
            allowed.mise,
            allowed.nix,
        );
        let found = find_files(root, top, allowed);
        let stamp = found
            .as_ref()
            .and_then(|(_, file)| std::fs::metadata(file).and_then(|m| m.modified()).ok());
        {
            let cached = self.projects.lock().await;
            if let Some((at, at_stamp, environment)) = cached.get(&key)
                && at.elapsed() < PROJECT_FRESH
                && *at_stamp == stamp
            {
                return environment.clone();
            }
        }
        let machine = self.machine().await;
        let environment = match found {
            None => Environment {
                vars: Arc::clone(&machine),
                ..Environment::default()
            },
            Some((kind, file)) => self.load(kind, &file, root, &machine).await,
        };
        self.projects
            .lock()
            .await
            .insert(key, (Instant::now(), stamp, environment.clone()));
        environment
    }

    async fn load(
        &self,
        kind: EnvironmentKind,
        file: &Path,
        root: &Path,
        machine: &Arc<Vars>,
    ) -> Environment {
        let fallback = |note: String| Environment {
            vars: Arc::clone(machine),
            kind: Some(kind),
            file: Some(file.to_path_buf()),
            note: Some(note),
        };
        let path = machine.get("PATH").cloned().unwrap_or_default();
        let loaded = match kind {
            EnvironmentKind::Direnv => {
                let Some(direnv) = which("direnv", &path) else {
                    return fallback("direnv is not installed on this machine".to_owned());
                };
                if !direnv_allowed(&direnv, root, machine).await {
                    // Shown, never allowed by Ahab (open question 3).
                    return fallback(format!(
                        "{} is not allowed: run `direnv allow` in a terminal to use it",
                        file.display()
                    ));
                }
                capture(
                    Command::new(direnv).arg("exec").arg(root),
                    root,
                    machine,
                    SHELL_TIMEOUT,
                )
                .await
            }
            EnvironmentKind::Mise => {
                let Some(mise) = which("mise", &path) else {
                    return fallback("mise is not installed on this machine".to_owned());
                };
                capture(
                    Command::new(mise).args(["exec", "--"]),
                    root,
                    machine,
                    SHELL_TIMEOUT,
                )
                .await
            }
            EnvironmentKind::Nix => self.nix(file, root, machine, &path).await,
        };
        match loaded {
            Ok(vars) => Environment {
                vars: Arc::new(vars),
                kind: Some(kind),
                file: Some(file.to_path_buf()),
                note: None,
            },
            Err(error) => fallback(error),
        }
    }

    /// A flake's dev shell, kept per flake lock so it starts once.
    async fn nix(
        &self,
        flake: &Path,
        root: &Path,
        machine: &Arc<Vars>,
        path: &str,
    ) -> Result<Vars, String> {
        let Some(nix) = which("nix", path) else {
            return Err("nix is not installed on this machine".to_owned());
        };
        let folder = flake.parent().unwrap_or(root);
        let lock = std::fs::read(folder.join("flake.lock")).unwrap_or_default();
        let nix_file = std::fs::read(flake).unwrap_or_default();
        let digest = {
            use sha2::Digest as _;
            let mut hash = sha2::Sha256::new();
            hash.update(folder.as_os_str().as_encoded_bytes());
            hash.update(&lock);
            hash.update(&nix_file);
            super::hex(&hash.finalize()[..16])
        };
        let cached = self.cache_dir.join(format!("{digest}.json"));
        if let Ok(text) = std::fs::read_to_string(&cached)
            && let Ok(vars) = serde_json::from_str::<Vars>(&text)
        {
            return Ok(vars);
        }
        let vars = capture(
            Command::new(nix)
                .args(["develop", "--no-warn-dirty"])
                .arg(folder)
                .arg("--command"),
            root,
            machine,
            NIX_TIMEOUT,
        )
        .await?;
        let _ = std::fs::create_dir_all(&self.cache_dir);
        if let Ok(text) = serde_json::to_string(&vars) {
            let _ = std::fs::write(&cached, text);
        }
        Ok(vars)
    }
}

/// The environment file nearest `root`: an `.envrc` (direnv looks above
/// the project's folder too), else mise's or asdf's, else a flake (within the
/// project's folder).
fn find_files(root: &Path, top: &Path, allowed: Allowed) -> Option<(EnvironmentKind, PathBuf)> {
    if allowed.direnv {
        for dir in root.ancestors() {
            let file = dir.join(".envrc");
            if file.is_file() {
                return Some((EnvironmentKind::Direnv, file));
            }
        }
    }
    if allowed.mise {
        for dir in root.ancestors() {
            for name in [".mise.toml", "mise.toml", ".tool-versions"] {
                let file = dir.join(name);
                if file.is_file() {
                    return Some((EnvironmentKind::Mise, file));
                }
            }
        }
    }
    if allowed.nix {
        for dir in root.ancestors() {
            let file = dir.join("flake.nix");
            if file.is_file() {
                return Some((EnvironmentKind::Nix, file));
            }
            if dir == top {
                break;
            }
        }
    }
    None
}

/// Whether direnv allows the `.envrc` for `root`: `direnv status --json`.
async fn direnv_allowed(direnv: &Path, root: &Path, machine: &Vars) -> bool {
    let mut command = Command::new(direnv);
    command
        .args(["status", "--json"])
        .current_dir(root)
        .env_clear()
        .envs(machine)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let Ok(Ok(out)) = tokio::time::timeout(SHELL_TIMEOUT, command.output()).await else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    direnv_status_allows(&text)
}

/// Reads `direnv status --json`: `state.foundRC.allowed` is 0 when allowed.
fn direnv_status_allows(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|status| status["state"]["foundRC"]["allowed"].as_i64())
        == Some(0)
}

/// Runs `prefix env -0` in `root` and reads the variables it prints.
async fn capture(
    prefix: &mut Command,
    root: &Path,
    machine: &Vars,
    timeout: Duration,
) -> Result<Vars, String> {
    prefix
        .args(["env", "-0"])
        .current_dir(root)
        .env_clear()
        .envs(machine)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let out = tokio::time::timeout(timeout, prefix.output())
        .await
        .map_err(|_| "loading the environment took too long".to_owned())?
        .map_err(|error| format!("cannot load the environment: {error}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let last = stderr.lines().rev().find(|line| !line.trim().is_empty());
        return Err(format!(
            "loading the environment failed{}",
            last.map(|line| format!(": {line}")).unwrap_or_default()
        ));
    }
    Ok(parse_env(&out.stdout))
}

/// `env -0`'s output.
pub fn parse_env(bytes: &[u8]) -> Vars {
    bytes
        .split(|&b| b == 0)
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            let (name, value) = entry.split_once('=')?;
            (!name.is_empty() && !name.contains('\n')).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The login shell's whole environment, past what its startup files print.
async fn login_shell() -> Option<Vars> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let mut command = Command::new(shell);
    command
        .args(["-lic", &format!("printf '\\n{MARKER}\\n'; env -0")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(SHELL_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    let marker = format!("\n{MARKER}\n");
    let at = out
        .stdout
        .windows(marker.len())
        .rposition(|window| window == marker.as_bytes())?;
    let vars = parse_env(&out.stdout[at + marker.len()..]);
    vars.contains_key("PATH").then_some(vars)
}

/// An executable called `name` on `path`.
pub fn which(name: &str, path: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let candidate = PathBuf::from(name);
        return is_executable(&candidate).then_some(candidate);
    }
    std::env::split_paths(path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_output_is_read_past_odd_values() {
        let vars = parse_env(b"PATH=/bin:/usr/bin\0MULTI=a\nb\0EMPTY=\0junk\0");
        assert_eq!(vars["PATH"], "/bin:/usr/bin");
        assert_eq!(vars["MULTI"], "a\nb");
        assert_eq!(vars["EMPTY"], "");
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn direnv_says_whether_it_allows() {
        assert!(direnv_status_allows(
            r#"{"config":{},"state":{"foundRC":{"allowed":0,"path":"/w/.envrc"}}}"#
        ));
        assert!(!direnv_status_allows(
            r#"{"state":{"foundRC":{"allowed":1,"path":"/w/.envrc"}}}"#
        ));
        assert!(!direnv_status_allows(r#"{"state":{"foundRC":null}}"#));
    }

    #[test]
    fn the_nearest_environment_file_wins_and_nix_waits_for_its_switch() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("repo");
        let root = top.join("app");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(top.join("flake.nix"), "{}").unwrap();
        let none = Allowed {
            direnv: true,
            mise: true,
            nix: false,
        };
        assert_eq!(find_files(&root, &top, none), None);
        let nix = Allowed { nix: true, ..none };
        assert_eq!(
            find_files(&root, &top, nix).map(|(kind, _)| kind),
            Some(EnvironmentKind::Nix)
        );
        std::fs::write(root.join(".tool-versions"), "nodejs 24\n").unwrap();
        assert_eq!(
            find_files(&root, &top, nix).map(|(kind, _)| kind),
            Some(EnvironmentKind::Mise)
        );
        std::fs::write(top.join(".envrc"), "use flake\n").unwrap();
        assert_eq!(
            find_files(&root, &top, nix).map(|(kind, _)| kind),
            Some(EnvironmentKind::Direnv)
        );
    }
}
