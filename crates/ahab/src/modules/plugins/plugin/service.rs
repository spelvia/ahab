//! Installing plugins (from a folder or a Git repository), enabling them,
//! placing them on machines, and removing them. The server keeps each
//! plugin's installed files in its own folder, by digest.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::machines::MachineService;
use crate::modules::plugins::plugin::model::{
    MANIFEST, Manifest, Placement, Plugin, Source, digest_of, inside, needed_by, unmet,
};
use crate::modules::plugins::plugin::store::PluginRepository;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

/// A Git clone, `npm ci` and the build stop after this long.
const BUILD_LIMIT: Duration = Duration::from_secs(600);

/// Where plugins are kept, and the Node that builds them.
#[derive(Debug, Clone)]
pub struct PluginPaths {
    /// The server's copies of installed plugins: `<store>/<id>/<digest>/`.
    pub store: PathBuf,
    /// Node 24 or later; `node` on `PATH` when absent.
    pub node: Option<PathBuf>,
}

/// What installing answered: the plugin, and what its manifest should fix.
pub struct Installed {
    pub plugin: Plugin,
    pub warnings: Vec<String>,
}

pub struct PluginService {
    repo: Arc<dyn PluginRepository>,
    machines: Arc<MachineService>,
    paths: PluginPaths,
    clock: Arc<dyn Clock>,
    changed: broadcast::Sender<String>,
}

fn not_found(id: &str) -> ServiceError {
    ServiceError::NotFound {
        code: "plugin_not_found",
        message: format!("There is no plugin {id}."),
    }
}

fn invalid(field: &'static str, message: impl Into<String>) -> ServiceError {
    ServiceError::Invalid(vec![FieldError::new(field, message)])
}

fn io(context: &str, error: &std::io::Error) -> ServiceError {
    ServiceError::Internal(format!("{context}: {error}"))
}

/// The files of a plugin folder that are installed: the manifest, its
/// README, licence and overviews, every file under the folders its
/// entries are in (`dist/`), and the files its languages name (grammars,
/// queries). Relative paths, sorted.
fn package_files(root: &Path, manifest: &Manifest) -> Result<Vec<PathBuf>, ServiceError> {
    let mut files = Vec::new();
    let top: Vec<String> = std::fs::read_dir(root)
        .map_err(|error| io("reading the plugin's folder", &error))?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for name in top {
        let upper = name.to_ascii_uppercase();
        if name == MANIFEST
            || upper.starts_with("README")
            || upper.starts_with("LICENSE")
            || upper.starts_with("OVERVIEW")
            || upper.starts_with("CHANGELOG")
        {
            files.push(PathBuf::from(name));
        }
    }
    for path in manifest.contributes.files() {
        files.push(PathBuf::from(path));
    }
    let mut folders = std::collections::BTreeSet::new();
    for (_, entry) in manifest.entries.all() {
        for path in entry.paths() {
            let path = Path::new(path);
            match path.components().next() {
                Some(first) if path.components().count() > 1 => {
                    folders.insert(PathBuf::from(first.as_os_str()));
                }
                _ => {
                    files.push(path.to_path_buf());
                }
            }
        }
    }
    for folder in folders {
        let mut stack = vec![folder];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(root.join(&dir))
                .map_err(|error| io(&format!("reading {}", dir.display()), &error))?;
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let relative = dir.join(entry.file_name());
                if kind.is_dir() {
                    stack.push(relative);
                } else if kind.is_file() {
                    files.push(relative);
                }
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// The digest of the files under `root`.
fn digest(root: &Path, files: &[PathBuf]) -> Result<String, ServiceError> {
    let mut read = Vec::with_capacity(files.len());
    for file in files {
        let bytes = std::fs::read(root.join(file))
            .map_err(|error| io(&format!("reading {}", file.display()), &error))?;
        read.push((file.to_string_lossy().replace('\\', "/"), bytes));
    }
    Ok(digest_of(&read))
}

/// A plugin folder read and checked: its manifest, warnings and files.
fn read_folder(root: &Path) -> Result<(Manifest, Vec<String>, Vec<PathBuf>), ServiceError> {
    let text = std::fs::read_to_string(root.join(MANIFEST))
        .map_err(|_| invalid("location", format!("{} has no {MANIFEST}", root.display())))?;
    let (mut manifest, warnings) = Manifest::parse(&text).map_err(ServiceError::Invalid)?;
    // Settings named by a file (`dist/settings.json`, as the build writes
    // it) are read into the manifest.
    if let Some(serde_json::Value::String(path)) = &manifest.settings {
        let schema = inside(path)
            .then(|| std::fs::read_to_string(root.join(path)).ok())
            .flatten()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter(|schema| schema["type"] == "object")
            .ok_or_else(|| {
                invalid(
                    "settings",
                    format!(
                        "{path} is not a settings schema (a JSON Schema object); build the plugin"
                    ),
                )
            })?;
        manifest.settings = Some(schema);
    }
    let mut missing = Vec::new();
    for (name, entry) in manifest.entries.all() {
        for path in entry.paths() {
            if !root.join(path).is_file() {
                missing.push(format!("{name} ({path})"));
            }
        }
    }
    if !missing.is_empty() {
        return Err(invalid(
            "entries",
            format!(
                "not built: {} missing; run the plugin's build first",
                missing.join(", ")
            ),
        ));
    }
    let absent: Vec<String> = manifest
        .contributes
        .files()
        .into_iter()
        .filter(|path| !root.join(path).is_file())
        .collect();
    if !absent.is_empty() {
        return Err(invalid(
            "contributes.languages",
            format!("missing: {}", absent.join(", ")),
        ));
    }
    let files = package_files(root, &manifest)?;
    Ok((manifest, warnings, files))
}

async fn run(program: &Path, args: &[&str], dir: &Path, what: &str) -> Result<(), ServiceError> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(BUILD_LIMIT, child)
        .await
        .map_err(|_| ServiceError::Conflict {
            code: "plugin_build_failed",
            message: format!("{what} took longer than ten minutes."),
        })?
        .map_err(|error| ServiceError::Conflict {
            code: "plugin_build_failed",
            message: format!("{what} could not start ({}): {error}", program.display()),
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: String = stderr
        .lines()
        .rev()
        .take(15)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Err(ServiceError::Conflict {
        code: "plugin_build_failed",
        message: format!("{what} failed:\n{tail}"),
    })
}

impl PluginService {
    pub fn new(
        repo: Arc<dyn PluginRepository>,
        machines: Arc<MachineService>,
        paths: PluginPaths,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let (changed, _) = broadcast::channel(64);
        Self {
            repo,
            machines,
            paths,
            clock,
            changed,
        }
    }

    /// The IDs of plugins as they change (installed, updated, enabled,
    /// disabled, placed, removed).
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.changed.subscribe()
    }

    /// Tells everyone following plugins that `id` changed (its settings).
    pub fn touch(&self, id: &str) {
        self.tell(id);
    }

    fn tell(&self, id: &str) {
        let _ = self.changed.send(id.to_owned());
    }

    pub async fn list(&self) -> Result<Vec<Plugin>, ServiceError> {
        Ok(self.repo.list().await?)
    }

    pub async fn get(&self, id: &str) -> Result<Plugin, ServiceError> {
        self.repo.get(id).await?.ok_or_else(|| not_found(id))
    }

    /// The folder holding a plugin's installed files.
    pub fn files_dir(&self, plugin: &Plugin) -> PathBuf {
        self.paths.store.join(&plugin.id).join(&plugin.digest)
    }

    /// Node's own `npm`, beside it.
    fn npm(&self) -> PathBuf {
        match &self.paths.node {
            Some(node) => node.with_file_name("npm"),
            None => PathBuf::from("npm"),
        }
    }

    /// Clones `url` into a new folder under the store and builds it when
    /// its entries are not committed.
    async fn fetch(&self, url: &str) -> Result<(tempfile::TempDir, PathBuf), ServiceError> {
        if url.starts_with('-') {
            return Err(invalid("location", "not a Git URL"));
        }
        let incoming = self.paths.store.join(".incoming");
        tokio::fs::create_dir_all(&incoming)
            .await
            .map_err(|error| io("creating the store", &error))?;
        let temp =
            tempfile::tempdir_in(&incoming).map_err(|error| io("a temporary folder", &error))?;
        let root = temp.path().join("plugin");
        let target = root.to_string_lossy().into_owned();
        run(
            Path::new("git"),
            &["clone", "--depth", "1", "--", url, &target],
            temp.path(),
            "git clone",
        )
        .await?;
        let text = tokio::fs::read_to_string(root.join(MANIFEST))
            .await
            .unwrap_or_default();
        let built = Manifest::parse(&text).is_ok_and(|(manifest, _)| {
            manifest
                .entries
                .all()
                .all(|(_, entry)| entry.paths().iter().all(|path| root.join(path).is_file()))
        });
        if !built && root.join("package.json").is_file() {
            let npm = self.npm();
            let install: &[&str] = if root.join("package-lock.json").is_file() {
                &["ci", "--no-audit", "--no-fund"]
            } else {
                &["install", "--no-audit", "--no-fund"]
            };
            run(&npm, install, &root, "npm install").await?;
            run(&npm, &["run", "build"], &root, "npm run build").await?;
        }
        Ok((temp, root))
    }

    /// Copies a checked folder into the store; answers the plugin as it
    /// would be saved (not saved yet).
    async fn copy_in(
        &self,
        root: &Path,
        source: Source,
        location: &str,
        previous: Option<&Plugin>,
    ) -> Result<Installed, ServiceError> {
        let root_owned = root.to_path_buf();
        let (manifest, warnings, files, digest) = tokio::task::spawn_blocking(move || {
            let (manifest, warnings, files) = read_folder(&root_owned)?;
            let digest = digest(&root_owned, &files)?;
            Ok::<_, ServiceError>((manifest, warnings, files, digest))
        })
        .await
        .map_err(|error| ServiceError::Internal(error.to_string()))??;
        if let Some(previous) = previous
            && previous.id != manifest.id
        {
            return Err(invalid(
                "id",
                format!("the folder now holds {}, not {}", manifest.id, previous.id),
            ));
        }
        let target = self.paths.store.join(&manifest.id).join(&digest);
        if !target.is_dir() {
            let staging = self
                .paths
                .store
                .join(&manifest.id)
                .join(format!(".{digest}"));
            let from = root.to_path_buf();
            let into = staging.clone();
            tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                if into.exists() {
                    std::fs::remove_dir_all(&into)?;
                }
                for file in &files {
                    let to = into.join(file);
                    if let Some(parent) = to.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(from.join(file), to)?;
                }
                Ok(())
            })
            .await
            .map_err(|error| ServiceError::Internal(error.to_string()))?
            .map_err(|error| io("copying the plugin", &error))?;
            tokio::fs::rename(&staging, &target)
                .await
                .map_err(|error| io("storing the plugin", &error))?;
        }
        let now = self.clock.now();
        let position = match previous {
            Some(previous) => previous.position,
            None => {
                self.repo
                    .list()
                    .await?
                    .iter()
                    .map(|plugin| plugin.position)
                    .max()
                    .unwrap_or(0)
                    + 1
            }
        };
        let plugin = Plugin {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            source,
            location: location.to_owned(),
            digest,
            enabled_at: previous.and_then(|previous| previous.enabled_at),
            position,
            machines: previous
                .map(|previous| previous.machines.clone())
                .unwrap_or_default(),
            created_at: previous.map_or(now, |previous| previous.created_at),
            updated_at: now,
            manifest,
        };
        Ok(Installed { plugin, warnings })
    }

    /// Removes a plugin's stored builds other than its current one.
    async fn prune(&self, plugin: &Plugin) {
        let Ok(mut entries) = tokio::fs::read_dir(self.paths.store.join(&plugin.id)).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_name() != plugin.digest.as_str() {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }

    /// Installs a plugin from a folder on the server's host, or a Git URL.
    /// It starts disabled.
    pub async fn install(&self, source: Source, location: &str) -> Result<Installed, ServiceError> {
        let location = location.trim();
        if location.is_empty() {
            return Err(invalid("location", "must not be empty"));
        }
        let (_temp, root, location) = match source {
            Source::Git => {
                let (temp, root) = self.fetch(location).await?;
                (Some(temp), root, location.to_owned())
            }
            Source::Folder | Source::Dev => {
                let root = std::fs::canonicalize(location)
                    .map_err(|_| invalid("location", format!("there is no folder {location}")))?;
                let shown = root.display().to_string();
                (None, root, shown)
            }
        };
        let text = tokio::fs::read_to_string(root.join(MANIFEST))
            .await
            .unwrap_or_default();
        if let Ok((manifest, _)) = Manifest::parse(&text)
            && self.repo.get(&manifest.id).await?.is_some()
        {
            return Err(ServiceError::Conflict {
                code: "plugin_exists",
                message: format!(
                    "{} is installed already; update it instead (ahab plugins update {}).",
                    manifest.id, manifest.id
                ),
            });
        }
        let installed = self.copy_in(&root, source, &location, None).await?;
        self.repo.save(&installed.plugin).await?;
        self.tell(&installed.plugin.id);
        Ok(installed)
    }

    /// Installs a plugin again from where it came from (its folder, or its
    /// Git repository's latest commit), keeping whether it is enabled.
    pub async fn update(&self, id: &str) -> Result<Installed, ServiceError> {
        let previous = self.get(id).await?;
        let (_temp, root) = match previous.source {
            Source::Git => {
                let (temp, root) = self.fetch(&previous.location).await?;
                (Some(temp), root)
            }
            Source::Folder | Source::Dev => (None, PathBuf::from(&previous.location)),
        };
        let installed = self
            .copy_in(&root, previous.source, &previous.location, Some(&previous))
            .await?;
        self.repo.save(&installed.plugin).await?;
        self.prune(&installed.plugin).await;
        if installed.plugin.digest != previous.digest {
            self.tell(id);
        }
        Ok(installed)
    }

    async fn check_machines(&self, machines: &[Uuid]) -> Result<(), ServiceError> {
        for machine in machines {
            self.machines.get(*machine).await?;
        }
        Ok(())
    }

    /// Turns a plugin on; with `machines`, those are where it runs when its
    /// manifest asks to be placed.
    pub async fn enable(
        &self,
        id: &str,
        machines: Option<Vec<Uuid>>,
    ) -> Result<Plugin, ServiceError> {
        let mut plugin = self.get(id).await?;
        let unmet = unmet(&plugin.manifest, &self.list().await?);
        if !unmet.is_empty() {
            return Err(ServiceError::Conflict {
                code: "plugin_dependency_missing",
                message: format!("{id} needs other plugins first: {}.", unmet.join("; ")),
            });
        }
        if let Some(machines) = machines {
            self.check_machines(&machines).await?;
            plugin.machines = machines;
        }
        if plugin.enabled_at.is_none() {
            plugin.enabled_at = Some(self.clock.now());
        }
        plugin.updated_at = self.clock.now();
        self.repo.save(&plugin).await?;
        self.tell(id);
        Ok(plugin)
    }

    /// Turns a plugin off everywhere.
    pub async fn disable(&self, id: &str) -> Result<Plugin, ServiceError> {
        let mut plugin = self.get(id).await?;
        let plugins = self.list().await?;
        let needed = needed_by(id, &plugins);
        if !needed.is_empty() {
            return Err(ServiceError::Conflict {
                code: "plugin_required",
                message: format!("{} need {id}: disable them first.", needed.join(", ")),
            });
        }
        plugin.enabled_at = None;
        plugin.updated_at = self.clock.now();
        self.repo.save(&plugin).await?;
        self.tell(id);
        Ok(plugin)
    }

    /// Changes where it runs (for `chosen` plugins) or its place among
    /// interceptors.
    pub async fn change(
        &self,
        id: &str,
        machines: Option<Vec<Uuid>>,
        position: Option<i32>,
    ) -> Result<Plugin, ServiceError> {
        let mut plugin = self.get(id).await?;
        if let Some(machines) = machines {
            if plugin.manifest.machines != Placement::Chosen {
                return Err(invalid(
                    "machines",
                    "this plugin runs on every machine; its manifest does not ask to be placed",
                ));
            }
            self.check_machines(&machines).await?;
            plugin.machines = machines;
        }
        if let Some(position) = position {
            plugin.position = position;
        }
        plugin.updated_at = self.clock.now();
        self.repo.save(&plugin).await?;
        self.tell(id);
        Ok(plugin)
    }

    /// Removes a disabled plugin and its files.
    pub async fn remove(&self, id: &str) -> Result<(), ServiceError> {
        let plugin = self.get(id).await?;
        if plugin.enabled() {
            return Err(ServiceError::Conflict {
                code: "plugin_enabled",
                message: format!("Disable {id} before removing it."),
            });
        }
        self.repo.delete(id).await?;
        let _ = tokio::fs::remove_dir_all(self.paths.store.join(id)).await;
        self.tell(id);
        Ok(())
    }

    /// One of a plugin's installed files (its web entry, a picture): the
    /// bytes, and their content type.
    pub async fn file(
        &self,
        id: &str,
        path: &str,
    ) -> Result<(Vec<u8>, &'static str), ServiceError> {
        let plugin = self.get(id).await?;
        let missing = || ServiceError::NotFound {
            code: "plugin_file_not_found",
            message: format!("{id} has no file {path}."),
        };
        if !inside(path) {
            return Err(missing());
        }
        let bytes = tokio::fs::read(self.files_dir(&plugin).join(path))
            .await
            .map_err(|_| missing())?;
        let kind = match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("js" | "mjs") => "text/javascript; charset=utf-8",
            Some("json" | "map") => "application/json",
            Some("css") => "text/css; charset=utf-8",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("md") => "text/markdown; charset=utf-8",
            // A language's grammar and queries.
            Some("wasm") => "application/wasm",
            Some("gz") => "application/gzip",
            Some("scm") => "text/plain; charset=utf-8",
            Some("html") => "text/html; charset=utf-8",
            _ => "application/octet-stream",
        };
        Ok((bytes, kind))
    }
}
