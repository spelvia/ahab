//! The daemon's tools: finding them and their runtimes, the
//! environment they start in, and what a machine's LSPs tab shows
//! (`tools.status`). Language servers are `lsp`'s, checks `problems`'.

pub mod environment;
pub mod find;
pub mod install;
pub mod plan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::modules::projects::Checkouts;
use crate::modules::tools::problems::daemon::Limiter;
use crate::modules::tools::registry::api::{
    MachineToolsDto, PlaceDto, RuntimeDto, ToolContext, ToolStateDto, ToolStatusDto,
};
use crate::modules::tools::registry::daemon::environment::{Environment, Environments, Vars};
use crate::modules::tools::registry::daemon::find::{
    Found, Lookup, Place, Runtime, find, installed, missing_runtime, runtimes, version_of,
};
use crate::modules::tools::registry::daemon::install::Installs;
use crate::modules::tools::registry::daemon::plan::{Effective, Roots};
use crate::modules::tools::registry::model::{ToolRecord, platform};

/// How the daemon's tools behave; tests turn the login shell off.
#[derive(Debug, Clone)]
pub struct ToolOptions {
    /// Take the machine's environment from the login shell (as agents are
    /// found), not only the daemon's own.
    pub login_shell: bool,
    /// Where GitHub releases are downloaded from.
    pub github: String,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            login_shell: true,
            github: "https://github.com".to_owned(),
        }
    }
}

/// Runtimes are looked for again after this.
const RUNTIMES_FRESH: Duration = Duration::from_secs(120);

/// Tells the server something (an install's progress), as a live event.
pub type Report = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// The repositories and worktrees in a project's folder (the Git
/// discovery, kept by the daemon's Git). Blocking: it may read the disk.
pub type CheckoutsOf = Arc<dyn Fn(&Path) -> Arc<Checkouts> + Send + Sync>;

/// What the daemon keeps for tools.
pub struct ToolHost {
    /// Where Ahab installs tools: `<data>/tools/<id>/<version>/`.
    pub tools_dir: PathBuf,
    /// Tools' own data per root (`${cache}`: jdtls's workspace).
    pub data_dir: PathBuf,
    pub environments: Environments,
    pub plugin_node: Option<PathBuf>,
    runtimes: tokio::sync::Mutex<HashMap<String, (Instant, Vec<Runtime>)>>,
    pub installs: Installs,
    pub report: Report,
    checkouts: CheckoutsOf,
    /// Check commands running at once on this machine (`maxChecks`).
    pub checks: Arc<Limiter>,
    /// Check commands' roots per checkout, kept.
    pub roots: Arc<Roots>,
}

/// A tool ready to start: its command, environment and folder.
#[derive(Debug, Clone)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vars,
    pub root: PathBuf,
    pub found: Found,
    pub environment: Environment,
}

/// Whether a tool can start, and if not why.
#[derive(Debug, Clone)]
pub enum Readiness {
    Ready(Box<Launch>),
    /// Not found; `installable` when Ahab can install it here.
    Missing {
        reason: String,
        installable: bool,
        environment: Environment,
    },
    /// Found or installable, but its runtime is missing.
    CannotRun {
        reason: String,
        environment: Environment,
    },
}

impl ToolHost {
    pub fn new(
        data_dir: &Path,
        options: &ToolOptions,
        plugin_node: Option<PathBuf>,
        report: Report,
        checkouts: CheckoutsOf,
    ) -> Self {
        let tools_dir = data_dir.join("tools");
        Self {
            environments: Environments::new(
                options.login_shell,
                data_dir.join("tool-environments"),
            ),
            data_dir: data_dir.join("tool-data"),
            installs: Installs::new(tools_dir.clone()),
            tools_dir,
            plugin_node,
            runtimes: tokio::sync::Mutex::new(HashMap::new()),
            report,
            checkouts,
            checks: Arc::new(Limiter::default()),
            roots: Arc::new(Roots::default()),
        }
    }

    /// The repositories and worktrees in `folder`.
    pub async fn checkouts(&self, folder: &Path) -> Arc<Checkouts> {
        let find = Arc::clone(&self.checkouts);
        let folder = folder.to_path_buf();
        tokio::task::spawn_blocking(move || find(&folder))
            .await
            .unwrap_or_default()
    }

    /// The runtimes on `path`, kept a while.
    pub async fn runtimes(&self, path: &str, vars: &Vars) -> Vec<Runtime> {
        let mut cached = self.runtimes.lock().await;
        if let Some((at, found)) = cached.get(path)
            && at.elapsed() < RUNTIMES_FRESH
        {
            return found.clone();
        }
        let found = runtimes(path, vars, self.plugin_node.as_deref()).await;
        cached.insert(path.to_owned(), (Instant::now(), found.clone()));
        found
    }

    /// Forgets what was found, for "Scan again".
    pub async fn forget(&self) {
        self.runtimes.lock().await.clear();
    }

    /// Whether `record` can start for `file` under `root`, and how.
    pub async fn prepare(
        &self,
        record: &ToolRecord,
        root: &Path,
        top: &Path,
        effective: &Effective,
        file: Option<&Path>,
    ) -> Readiness {
        let environment = self
            .environments
            .project(root, top, effective.allowed())
            .await;
        let machine = self.environments.machine().await;
        let machine_path = machine.get("PATH").cloned().unwrap_or_default();
        let choice = effective.settings.tools.get(&record.id);
        let lookup = Lookup {
            root: Some(root),
            top: Some(top),
            environment: environment.kind.and(environment.path()),
            machine: &machine_path,
            vars: &environment.vars,
            tools_dir: &self.tools_dir,
            setting: choice.and_then(|choice| choice.path.as_deref()),
        };
        let env_path = environment
            .path()
            .map_or_else(|| machine_path.clone(), str::to_owned);
        let found_runtimes = self.runtimes(&env_path, &environment.vars).await;
        let Some(found) = find(record, &lookup).await else {
            let installable = record
                .source
                .as_ref()
                .is_some_and(|source| source.installable_on(&platform()));
            let reason = match &record.hint {
                Some(hint) => format!("not found ({hint})"),
                None => "not found".to_owned(),
            };
            return Readiness::Missing {
                reason,
                installable,
                environment,
            };
        };
        if let Some(reason) = missing_runtime(record, &found_runtimes) {
            return Readiness::CannotRun {
                reason,
                environment,
            };
        }
        let mut env: Vars = (*environment.vars).clone();
        env.extend(effective.settings.environment.env.clone());
        env.extend(record.run.env.clone());
        if let Some(choice) = choice {
            env.extend(choice.env.clone());
        }
        // Tools find each other: nil its nixfmt, bash-language-server shfmt.
        let mut path_dirs = self.installed_bin_dirs();
        if let Some(path) = env.get("PATH") {
            path_dirs.extend(std::env::split_paths(path));
        }
        if let Ok(joined) = std::env::join_paths(path_dirs) {
            env.insert("PATH".to_owned(), joined.to_string_lossy().into_owned());
        }
        let cache = self.data_dir.join(&record.id).join(short_hash(root));
        let args = record
            .run
            .command
            .iter()
            .skip(1)
            .map(|arg| {
                expand(
                    arg,
                    &Variables {
                        path: file,
                        root,
                        tool_dir: found.tool_dir.as_deref(),
                        cache: &cache,
                    },
                )
            })
            .collect();
        Readiness::Ready(Box::new(Launch {
            program: found.path.clone(),
            args,
            env,
            root: root.to_path_buf(),
            found,
            environment,
        }))
    }

    /// The folders of every binary Ahab installed.
    fn installed_bin_dirs(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.tools_dir) else {
            return Vec::new();
        };
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if let Some((tool, dir)) = installed(&self.tools_dir, &id) {
                for relative in tool.bin.values() {
                    if let Some(parent) = dir.join(relative).parent()
                        && !dirs.iter().any(|known: &PathBuf| known == parent)
                    {
                        dirs.push(parent.to_path_buf());
                    }
                }
            }
        }
        dirs
    }

    /// Every record's state on this machine, and its runtimes.
    pub async fn status(&self, context: &ToolContext) -> MachineToolsDto {
        let effective = Effective::new(context, None);
        let vars = self.environments.machine().await;
        let path = vars.get("PATH").cloned().unwrap_or_default();
        let found_runtimes = self.runtimes(&path, &vars).await;
        let platform = platform();
        let records: Vec<&ToolRecord> = effective.records.values().collect();
        let tools = futures_util::future::join_all(records.into_iter().map(|record| {
            let effective = &effective;
            let vars = &vars;
            let path = &path;
            let found_runtimes = &found_runtimes;
            let platform = &platform;
            async move {
                self.tool_status(record, effective, vars, path, found_runtimes, platform)
                    .await
            }
        }))
        .await;
        let mut tools = tools;
        tools.sort_by(|a, b| (a.languages.first(), &a.name).cmp(&(b.languages.first(), &b.name)));
        MachineToolsDto {
            platform,
            runtimes: found_runtimes
                .into_iter()
                .map(|runtime| RuntimeDto {
                    id: runtime.id,
                    name: runtime.name,
                    version: runtime.version,
                    path: runtime.path,
                })
                .collect(),
            tools,
            jobs: self.installs.jobs(),
        }
    }

    async fn tool_status(
        &self,
        record: &ToolRecord,
        effective: &Effective,
        vars: &Vars,
        path: &str,
        found_runtimes: &[Runtime],
        platform: &str,
    ) -> ToolStatusDto {
        let mut status = ToolStatusDto {
            id: record.id.clone(),
            name: record.name.clone(),
            kind: record.kind.as_str().to_owned(),
            languages: record.languages.clone(),
            description: record.description.clone(),
            homepage: record.homepage.clone(),
            license: record.license.clone(),
            source: record.source.as_ref().map(|s| s.type_name().to_owned()),
            version: record
                .source
                .as_ref()
                .and_then(|s| s.version())
                .map(str::to_owned),
            state: ToolStateDto::Missing,
            place: None,
            detail: None,
            path: None,
            found_version: None,
            update_available: false,
            reason: None,
            hint: record.hint.clone(),
            origin: effective
                .origins
                .get(&record.id)
                .cloned()
                .unwrap_or_else(|| "ahab".to_owned()),
            default: effective.is_default(&record.id),
        };
        if effective.settings.is_off(&record.id) {
            status.state = ToolStateDto::Off;
            return status;
        }
        let lookup = Lookup {
            root: None,
            top: None,
            environment: None,
            machine: path,
            vars,
            tools_dir: &self.tools_dir,
            setting: effective
                .settings
                .tools
                .get(&record.id)
                .and_then(|choice| choice.path.as_deref()),
        };
        if let Some(found) = find(record, &lookup).await {
            status.place = Some(place_dto(found.place));
            status.detail = Some(found.detail.clone());
            status.path = Some(found.path.display().to_string());
            if found.place == Place::Ahab {
                status.state = ToolStateDto::Installed;
                if let Some((tool, _)) = installed(&self.tools_dir, &record.id) {
                    status.update_available = status.version.as_ref() != Some(&tool.version);
                    status.found_version = Some(tool.version);
                }
            } else {
                status.state = ToolStateDto::Found;
                let args = record
                    .run
                    .version_args
                    .clone()
                    .unwrap_or_else(|| vec!["--version".to_owned()]);
                status.found_version = version_of(&found.path, &args, vars).await;
            }
            if let Some(reason) = missing_runtime(record, found_runtimes) {
                status.state = ToolStateDto::CannotRun;
                status.reason = Some(reason);
            }
            return status;
        }
        let Some(source) = &record.source else {
            status.reason = Some("not found on this machine".to_owned());
            return status;
        };
        if !source.installable_on(platform) {
            if source.asset(platform).is_some() || source.version().is_none() {
                status.reason = Some("not found on this machine".to_owned());
            } else {
                status.state = ToolStateDto::CannotRun;
                status.reason = Some(format!("no build for {platform}"));
            }
            return status;
        }
        let needs = source.installer().into_iter().map(|runtime| {
            crate::modules::tools::registry::model::Requirement {
                runtime: runtime.to_owned(),
                min: None,
            }
        });
        let mut check = record.clone();
        check.requires.extend(needs);
        status.state = match missing_runtime(&check, found_runtimes) {
            Some(reason) => {
                status.reason = Some(reason);
                ToolStateDto::CannotRun
            }
            None => ToolStateDto::Available,
        };
        status
    }
}

fn place_dto(place: Place) -> PlaceDto {
    match place {
        Place::Setting => PlaceDto::Setting,
        Place::Project => PlaceDto::Project,
        Place::Environment => PlaceDto::Environment,
        Place::Ahab => PlaceDto::Ahab,
        Place::Path => PlaceDto::Path,
    }
}

/// What `${…}` in a command stands for.
pub struct Variables<'a> {
    pub path: Option<&'a Path>,
    pub root: &'a Path,
    pub tool_dir: Option<&'a Path>,
    pub cache: &'a Path,
}

/// `arg` with its variables filled in.
pub fn expand(arg: &str, variables: &Variables<'_>) -> String {
    let display = |path: &Path| path.display().to_string();
    let mut out = arg.replace("${root}", &display(variables.root));
    out = out.replace("${cache}", &display(variables.cache));
    if let Some(path) = variables.path {
        out = out.replace("${path}", &display(path));
    }
    if let Some(dir) = variables.tool_dir {
        out = out.replace("${toolDir}", &display(dir));
    }
    out
}

/// A short, stable name for a folder.
pub fn short_hash(path: &Path) -> String {
    use sha2::Digest as _;
    hex(&sha2::Sha256::digest(path.as_os_str().as_encoded_bytes())[..8])
}

/// Bytes as lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}
