//! Finding a tool, first that exists wins unless a setting names
//! one: the project (`node_modules/.bin`, `.venv/bin`, the Rust toolchain
//! it asks for), the project's environment, installed by Ahab, the
//! machine's login shell. And asking tools and runtimes for their version.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::modules::tools::registry::daemon::environment::{Vars, is_executable, which};
use crate::modules::tools::registry::model::ToolRecord;
use crate::shared::sync::lock;

/// How long `--version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a tool was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// A path the settings name.
    Setting,
    /// In the project: `node_modules/.bin`, `.venv`, its Rust toolchain.
    Project,
    /// On the project environment's `PATH` (direnv, mise, Nix).
    Environment,
    /// Installed by Ahab on this machine.
    Ahab,
    /// On the machine's `PATH`.
    Path,
}

/// A tool found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub place: Place,
    /// Where exactly, to show (`node_modules/.bin`, `rust-toolchain.toml`).
    pub detail: String,
    /// The installed tool's folder, for `${toolDir}`.
    pub tool_dir: Option<PathBuf>,
}

/// What finding a tool looks at.
pub struct Lookup<'a> {
    /// The tool's root, or the machine's home for machine-wide lookups.
    pub root: Option<&'a Path>,
    /// The Ahab project's folder: the language project's places are looked
    /// for up to it.
    pub top: Option<&'a Path>,
    /// The project environment's `PATH`, when it has its own.
    pub environment: Option<&'a str>,
    /// The machine's `PATH`.
    pub machine: &'a str,
    /// The machine's variables, for `rustup which`.
    pub vars: &'a Vars,
    /// Where Ahab installs tools.
    pub tools_dir: &'a Path,
    /// A path the settings name.
    pub setting: Option<&'a str>,
}

/// Binaries a Rust toolchain file (`rust-toolchain.toml`) decides.
const RUSTUP: [&str; 4] = ["cargo", "rustfmt", "rust-analyzer", "cargo-clippy"];

/// Finds `record`'s program.
pub async fn find(record: &ToolRecord, lookup: &Lookup<'_>) -> Option<Found> {
    let program = record.program()?;
    if let Some(path) = lookup.setting {
        let path = PathBuf::from(path);
        return is_executable(&path).then(|| Found {
            path,
            place: Place::Setting,
            detail: "set in the settings".to_owned(),
            tool_dir: None,
        });
    }
    if program.contains('/') {
        let path = PathBuf::from(program);
        return is_executable(&path).then(|| Found {
            path,
            place: Place::Setting,
            detail: "its record's path".to_owned(),
            tool_dir: None,
        });
    }
    if let (Some(root), Some(top)) = (lookup.root, lookup.top)
        && let Some(found) = in_project(program, root, top, lookup).await
    {
        return Some(found);
    }
    let environment = match lookup.environment.and_then(|path| which(program, path)) {
        Some(path) => through_rustup(program, path, lookup).await,
        None => None,
    };
    if let Some(path) = environment {
        return Some(Found {
            path,
            place: Place::Environment,
            detail: "the project's environment".to_owned(),
            tool_dir: None,
        });
    }
    if let Some((path, dir)) = installed_bin(lookup.tools_dir, &record.id, program) {
        return Some(Found {
            path,
            place: Place::Ahab,
            detail: "installed by Ahab".to_owned(),
            tool_dir: Some(dir),
        });
    }
    let path = through_rustup(program, which(program, lookup.machine)?, lookup).await?;
    Some(Found {
        detail: describe_path(&path),
        path,
        place: Place::Path,
        tool_dir: None,
    })
}

/// `path` itself, unless it is rustup's proxy for `program`: then the
/// active toolchain's binary, or nothing when the toolchain lacks it. A
/// proxy without its component (`rustup component add rust-analyzer` not
/// run) falls back to itself until it dies, so it is no tool at all.
async fn through_rustup(program: &str, path: PathBuf, lookup: &Lookup<'_>) -> Option<PathBuf> {
    let is_proxy = RUSTUP.contains(&program)
        && std::fs::canonicalize(&path)
            .ok()
            .and_then(|real| real.file_name().map(|name| name == "rustup"))
            .unwrap_or(false);
    if !is_proxy {
        return Some(path);
    }
    let dir = lookup.root.or(lookup.top).unwrap_or_else(|| Path::new("/"));
    rustup_which(program, dir, lookup).await
}

async fn in_project(program: &str, root: &Path, top: &Path, lookup: &Lookup<'_>) -> Option<Found> {
    for dir in root.ancestors() {
        for (folder, detail) in [
            ("node_modules/.bin", "node_modules/.bin"),
            (".venv/bin", ".venv"),
            ("venv/bin", "venv"),
        ] {
            let candidate = dir.join(folder).join(program);
            if is_executable(&candidate) {
                return Some(Found {
                    path: candidate,
                    place: Place::Project,
                    detail: detail.to_owned(),
                    tool_dir: None,
                });
            }
        }
        if RUSTUP.contains(&program)
            && let Some(file) = ["rust-toolchain.toml", "rust-toolchain"]
                .iter()
                .map(|name| dir.join(name))
                .find(|file| file.is_file())
            && let Some(path) = rustup_which(program, dir, lookup).await
        {
            return Some(Found {
                path,
                place: Place::Project,
                detail: file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                tool_dir: None,
            });
        }
        if dir == top {
            break;
        }
    }
    None
}

/// `rustup which <program>` in `dir`: the toolchain's binary.
async fn rustup_which(program: &str, dir: &Path, lookup: &Lookup<'_>) -> Option<PathBuf> {
    let rustup = which("rustup", lookup.environment.unwrap_or(lookup.machine))?;
    let mut command = Command::new(rustup);
    command
        .args(["which", program])
        .current_dir(dir)
        .env_clear()
        .envs(lookup.vars)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(VERSION_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    (out.status.success() && is_executable(&path)).then_some(path)
}

/// What an installed tool records about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTool {
    pub id: String,
    pub version: String,
    /// Its binaries' paths, relative to its folder.
    pub bin: std::collections::BTreeMap<String, String>,
    pub source: String,
    #[serde(default)]
    pub installed_at: Option<String>,
}

/// The file in an installed version's folder that describes it.
pub const INSTALLED_FILE: &str = ".ahab-tool.json";

/// The version of `id` Ahab installed and uses now, and its folder.
pub fn installed(tools_dir: &Path, id: &str) -> Option<(InstalledTool, PathBuf)> {
    let current = tools_dir.join(id).join("current");
    let dir = std::fs::canonicalize(&current).ok()?;
    let text = std::fs::read_to_string(dir.join(INSTALLED_FILE)).ok()?;
    let tool: InstalledTool = serde_json::from_str(&text).ok()?;
    Some((tool, dir))
}

fn installed_bin(tools_dir: &Path, id: &str, program: &str) -> Option<(PathBuf, PathBuf)> {
    let (tool, dir) = installed(tools_dir, id)?;
    let relative = tool.bin.get(program)?;
    let path = dir.join(relative);
    is_executable(&path).then_some((path, dir))
}

/// Says where on `PATH` a tool is, by the version manager its path shows.
fn describe_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    for (part, name) in [
        ("/.cargo/bin/", "cargo"),
        ("/.rustup/", "rustup"),
        ("/fnm/", "fnm"),
        ("/.nvm/", "nvm"),
        ("/.volta/", "volta"),
        ("/mise/", "mise"),
        ("/.asdf/", "asdf"),
        ("/go/bin/", "go"),
        ("/nix/store/", "Nix"),
        ("/.nix-profile/", "Nix"),
        ("/run/current-system/", "NixOS"),
        ("/homebrew/", "Homebrew"),
        ("/linuxbrew/", "Homebrew"),
        ("/.local/bin/", "~/.local/bin"),
    ] {
        if text.contains(part) {
            return format!("on PATH ({name})");
        }
    }
    "on PATH".to_owned()
}

type VersionKey = (PathBuf, Vec<String>);
/// A version found, and the file's time then.
type Known = (Option<SystemTime>, Option<String>);
static VERSIONS: LazyLock<Mutex<HashMap<VersionKey, Known>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `path --version` (or the record's arguments), its first version-looking
/// word; kept until the file changes.
pub async fn version_of(path: &Path, args: &[String], vars: &Vars) -> Option<String> {
    if args.is_empty() {
        return None;
    }
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = (path.to_path_buf(), args.to_vec());
    if let Some((at, version)) = lock(&VERSIONS).get(&key)
        && *at == modified
    {
        return version.clone();
    }
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .envs(vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let version = match tokio::time::timeout(VERSION_TIMEOUT, command.output()).await {
        Ok(Ok(out)) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            pick_version(&text)
        }
        _ => None,
    };
    lock(&VERSIONS).insert(key, (modified, version.clone()));
    version
}

static VERSION: LazyLock<Option<regex::Regex>> = LazyLock::new(|| {
    regex::Regex::new(r"\d{4}-\d{2}-\d{2}|\d+\.\d+(?:\.\d+)?(?:[-+][0-9A-Za-z.]+)?").ok()
});

/// The first version in a tool's answer (`rust-analyzer 1.91.0 (2026-11-10)`
/// is `1.91.0`, `go version go1.23.1` is `1.23.1`).
pub fn pick_version(text: &str) -> Option<String> {
    let pattern = VERSION.as_ref()?;
    text.lines()
        .find_map(|line| pattern.find(line))
        .map(|found| found.as_str().to_owned())
}

/// A runtime on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    /// `node`, `java`, `python`, `go`, `rust`.
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub major: Option<u32>,
    pub path: Option<String>,
}

/// The runtimes tools need, as `path` finds them; Node is the plugins' when
/// one is set and the environment has none.
pub async fn runtimes(path: &str, vars: &Vars, plugin_node: Option<&Path>) -> Vec<Runtime> {
    let probes: [(&str, &str, &str, &[&str]); 5] = [
        ("node", "Node", "node", &["--version"]),
        ("java", "Java", "java", &["-version"]),
        ("python", "Python", "python3", &["--version"]),
        ("go", "Go", "go", &["version"]),
        ("rust", "Rust", "cargo", &["--version"]),
    ];
    let mut found = Vec::new();
    for (id, name, program, args) in probes {
        let located = which(program, path).or_else(|| {
            (id == "node")
                .then(|| plugin_node.map(Path::to_path_buf))
                .flatten()
                .filter(|node| is_executable(node))
        });
        let version = match &located {
            Some(located) => {
                let args: Vec<String> = args.iter().map(|&arg| arg.to_owned()).collect();
                version_of(located, &args, vars).await
            }
            None => None,
        };
        found.push(Runtime {
            id: id.to_owned(),
            name: name.to_owned(),
            major: version.as_deref().and_then(major_of),
            version,
            path: located.map(|path| path.display().to_string()),
        });
    }
    found
}

/// A version's major number; Java's `1.8` is 8.
pub fn major_of(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1
        && let Some(second) = parts.next().and_then(|part| part.parse().ok())
        && second >= 5
    {
        return Some(second);
    }
    Some(first)
}

/// Why `record` cannot run with these runtimes, if it cannot: "needs Java
/// 21 or later (found: Java 17)".
pub fn missing_runtime(record: &ToolRecord, runtimes: &[Runtime]) -> Option<String> {
    for requirement in &record.requires {
        // Python runs installers (basedpyright brings its own Node);
        // only what running needs counts here, plus Java for jdtls.
        let runtime = runtimes.iter().find(|r| r.id == requirement.runtime);
        let name = runtime.map_or_else(|| requirement.runtime.clone(), |r| r.name.clone());
        let wanted = requirement
            .min
            .map_or_else(|| name.clone(), |min| format!("{name} {min} or later"));
        match runtime.and_then(|r| r.major) {
            None if runtime.and_then(|r| r.path.as_ref()).is_none() => {
                return Some(format!("needs {wanted}"));
            }
            Some(major) if requirement.min.is_some_and(|min| major < min) => {
                return Some(format!(
                    "needs {wanted} (found: {name} {})",
                    runtime
                        .and_then(|r| r.version.clone())
                        .unwrap_or_else(|| major.to_string())
                ));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_what_tools_print() {
        assert_eq!(
            pick_version("rust-analyzer 1.91.0 (f8297e3 2026-11-10)").as_deref(),
            Some("1.91.0")
        );
        assert_eq!(
            pick_version("go version go1.23.1 linux/amd64").as_deref(),
            Some("1.23.1")
        );
        assert_eq!(
            pick_version("openjdk version \"21.0.2\" 2024-01-16").as_deref(),
            Some("21.0.2")
        );
        assert_eq!(pick_version("v24.18.0").as_deref(), Some("24.18.0"));
        assert_eq!(pick_version("nothing here"), None);
        assert_eq!(major_of("21.0.2"), Some(21));
        assert_eq!(major_of("1.8.0_392"), Some(8));
        assert_eq!(major_of("3.12.1"), Some(3));
    }

    #[test]
    fn a_missing_runtime_is_explained() {
        let record: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "jdtls", "name": "jdtls", "kind": "languageServer",
            "requires": [{ "runtime": "java", "min": 21 }],
            "run": { "command": ["jdtls"] }
        }))
        .unwrap();
        let java = |version: &str| Runtime {
            id: "java".to_owned(),
            name: "Java".to_owned(),
            version: Some(version.to_owned()),
            major: major_of(version),
            path: Some("/usr/bin/java".to_owned()),
        };
        assert_eq!(
            missing_runtime(&record, &[java("17.0.9")]).as_deref(),
            Some("needs Java 21 or later (found: Java 17.0.9)")
        );
        assert_eq!(missing_runtime(&record, &[java("21.0.2")]), None);
        assert_eq!(
            missing_runtime(&record, &[]).as_deref(),
            Some("needs java 21 or later")
        );
    }

    #[tokio::test]
    async fn the_project_comes_before_the_machine() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("repo");
        let root = top.join("web");
        let bin = top.join("node_modules/.bin");
        let machine = dir.path().join("bin");
        for folder in [&root, &bin, &machine] {
            std::fs::create_dir_all(folder).unwrap();
        }
        for file in [bin.join("tsc"), machine.join("tsc")] {
            std::fs::write(&file, "#!/bin/sh\necho 5.9.3\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let record: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "tsc", "name": "tsc", "kind": "check",
            "run": { "command": ["tsc"], "output": { "format": "sarif" } }
        }))
        .unwrap();
        let vars = Vars::new();
        let machine_path = machine.display().to_string();
        let lookup = Lookup {
            root: Some(&root),
            top: Some(&top),
            environment: None,
            machine: &machine_path,
            vars: &vars,
            tools_dir: dir.path(),
            setting: None,
        };
        let found = find(&record, &lookup).await.unwrap();
        assert_eq!(found.place, Place::Project);
        assert_eq!(found.detail, "node_modules/.bin");
        let outside = Lookup {
            root: None,
            top: None,
            ..lookup
        };
        let found = find(&record, &outside).await.unwrap();
        assert_eq!(found.place, Place::Path);
        assert_eq!(
            version_of(&found.path, &["--version".to_owned()], &vars)
                .await
                .as_deref(),
            Some("5.9.3")
        );
    }
}

#[cfg(all(test, unix))]
mod rustup_proxy {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::{Lookup, through_rustup};
    use crate::modules::tools::registry::daemon::environment::Vars;

    #[expect(
        clippy::unwrap_used,
        reason = "a test helper; a failed write should fail the test"
    )]
    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    async fn a_proxy_without_its_component_is_no_tool() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        // rustup's proxies are links to rustup itself; `rustup which` fails
        // when the toolchain lacks the component.
        script(&bin.join("rustup"), "exit 1");
        std::os::unix::fs::symlink(bin.join("rustup"), bin.join("rust-analyzer")).unwrap();
        let path_var = bin.to_string_lossy().into_owned();
        let vars = Vars::default();
        let lookup = Lookup {
            root: Some(dir.path()),
            top: Some(dir.path()),
            environment: None,
            machine: &path_var,
            vars: &vars,
            tools_dir: dir.path(),
            setting: None,
        };
        let found = through_rustup("rust-analyzer", bin.join("rust-analyzer"), &lookup).await;
        assert_eq!(found, None);

        // With the component, the toolchain's own binary is used.
        let real = dir.path().join("real-rust-analyzer");
        script(&real, "exit 0");
        script(&bin.join("rustup"), &format!("echo {}", real.display()));
        let found = through_rustup("rust-analyzer", bin.join("rust-analyzer"), &lookup).await;
        assert_eq!(found, Some(real.clone()));

        // Anything that is not the proxy is left alone.
        let plain: PathBuf = real.clone();
        assert_eq!(
            through_rustup("taplo", plain.clone(), &lookup).await,
            Some(plain)
        );
    }
}
