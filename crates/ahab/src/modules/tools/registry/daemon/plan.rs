//! Choosing tools: the settings in layers (the machine, the
//! repository's `.ahab/tools.json`, the project), which tools serve a
//! language, and where each one's root is.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::modules::tools::registry::api::ToolContext;
use crate::modules::tools::registry::daemon::environment::Allowed;
use crate::modules::tools::registry::model::{LanguageTools, ToolKind, ToolRecord};
use crate::modules::tools::setting::model::ToolSettings;
use crate::shared::sync::lock;

/// The repository's shared choices.
pub const REPOSITORY_FILE: &str = ".ahab/tools.json";

/// The settings and records in effect for one place.
#[derive(Debug, Clone, Default)]
pub struct Effective {
    pub settings: ToolSettings,
    pub records: BTreeMap<String, ToolRecord>,
    pub languages: LanguageTools,
    /// Where each record came from: `ahab`, a plugin's ID, `machine`,
    /// `repository`, `project`.
    pub origins: BTreeMap<String, String>,
    /// The repository's file, when one was read.
    pub repository: Option<PathBuf>,
}

impl Effective {
    /// `context`'s layers with the repository's between the machine's and
    /// the project's.
    pub fn new(context: &ToolContext, repository: Option<(ToolSettings, PathBuf)>) -> Self {
        let mut records: BTreeMap<String, ToolRecord> = BTreeMap::new();
        let mut origins = BTreeMap::new();
        for record in &context.records {
            origins.insert(
                record.id.clone(),
                context
                    .origins
                    .get(&record.id)
                    .cloned()
                    .unwrap_or_else(|| "ahab".to_owned()),
            );
            records.insert(record.id.clone(), record.clone());
        }
        let mut languages = context.languages.clone();
        let (repository_settings, repository_file) = match repository {
            Some((settings, file)) => (Some(settings), Some(file)),
            None => (None, None),
        };
        let layers: Vec<(&str, &ToolSettings)> = [
            Some(("machine", &context.machine)),
            repository_settings.as_ref().map(|s| ("repository", s)),
            context.project.as_ref().map(|s| ("project", s)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut settings = ToolSettings::default();
        for (origin, layer) in &layers {
            settings.layer(layer);
            // Your own tools: found, never installed; they serve their
            // languages after the defaults.
            for record in &layer.records {
                let mut record = record.clone();
                record.source = None;
                for language in &record.languages {
                    let tools = languages.entry(language.clone()).or_default();
                    if !tools.contains(&record.id) {
                        tools.push(record.id.clone());
                    }
                }
                origins.insert(record.id.clone(), (*origin).to_owned());
                records.insert(record.id.clone(), record);
            }
        }
        Self {
            settings,
            records,
            languages,
            origins,
            repository: repository_file,
        }
    }

    /// The tools that serve `language`, in order of preference: the
    /// settings' list, else the language's; without those turned off.
    pub fn tools_for(&self, language: &str) -> Vec<&ToolRecord> {
        let choice = self.settings.languages.get(language);
        if choice.and_then(|choice| choice.off) == Some(true) {
            return Vec::new();
        }
        let ids = choice
            .and_then(|choice| choice.tools.clone())
            .or_else(|| self.languages.get(language).cloned())
            .unwrap_or_default();
        ids.iter()
            .filter(|id| !self.settings.is_off(id))
            .filter_map(|id| self.records.get(id))
            .filter(|record| record.kind != ToolKind::Unknown)
            .collect()
    }

    /// Whether `language` was turned off.
    pub fn language_off(&self, language: &str) -> bool {
        self.settings
            .languages
            .get(language)
            .and_then(|choice| choice.off)
            == Some(true)
    }

    /// Whether `id` is among a language's defaults.
    pub fn is_default(&self, id: &str) -> bool {
        self.languages
            .values()
            .any(|tools| tools.iter().any(|t| t == id))
    }

    /// Which project environments the settings allow.
    pub fn allowed(&self) -> Allowed {
        let environment = &self.settings.environment;
        Allowed {
            direnv: environment.direnv.unwrap_or(true),
            mise: environment.mise.unwrap_or(true),
            nix: environment.nix.unwrap_or(false),
        }
    }

    /// When a check runs.
    pub fn triggers(&self, record: &ToolRecord) -> Vec<String> {
        self.settings
            .tools
            .get(&record.id)
            .and_then(|choice| choice.on.clone())
            .or_else(|| record.run.on.clone())
            .unwrap_or_else(|| {
                vec![
                    "save".to_owned(),
                    "agentTurn".to_owned(),
                    "request".to_owned(),
                ]
            })
    }

    /// Whether files of `language` are formatted on save.
    pub fn format_on_save(&self, language: &str) -> bool {
        self.settings
            .languages
            .get(language)
            .and_then(|choice| choice.format_on_save)
            == Some(true)
    }
}

/// A repository may choose among the tools Ahab, plugins and this machine
/// know, but not declare tools of its own: a record is a command, and a
/// cloned repository (or an agent writing the file) must not make Ahab run
/// one just because a file was opened. Its records are dropped and logged.
fn without_commands(mut settings: ToolSettings, file: &Path) -> ToolSettings {
    if !settings.records.is_empty() {
        let ids: Vec<&str> = settings
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        tracing::warn!(
            file = %file.display(),
            tools = ?ids,
            "a repository's .ahab/tools.json may not declare tools; declare them in the project's or this machine's settings instead"
        );
        settings.records.clear();
    }
    // What a machine can bear is the machine's to say.
    settings.max_servers = None;
    settings.max_checks = None;
    settings.memory_budget = None;
    settings
}

/// The nearest `.ahab/tools.json` from `from` up to the project's folder.
pub fn repository_settings(from: &Path, top: &Path) -> Option<(ToolSettings, PathBuf)> {
    for dir in from.ancestors() {
        let file = dir.join(REPOSITORY_FILE);
        if let Ok(text) = std::fs::read_to_string(&file) {
            return match serde_json::from_str::<ToolSettings>(&text) {
                Ok(settings) => Some((without_commands(settings, &file), file)),
                Err(error) => {
                    tracing::info!(file = %file.display(), %error, "an unreadable .ahab/tools.json is ignored");
                    None
                }
            };
        }
        if dir == top || !dir.starts_with(top) {
            break;
        }
    }
    None
}

/// The folder `record` runs in for `file`: the nearest folder with one of
/// its root markers, or the top-most whose marker contains the record's
/// text (Rust's `[workspace]`), up to `top` (the file's checkout, so
/// a root never spans two repositories); that folder when none has one.
pub fn find_root(file: &Path, top: &Path, record: &ToolRecord) -> PathBuf {
    let markers = &record.run.root_markers;
    if markers.is_empty() {
        return top.to_path_buf();
    }
    let start = if file.is_dir() {
        file
    } else {
        file.parent().unwrap_or(top)
    };
    let mut nearest = None;
    let mut top_most = None;
    for dir in start.ancestors() {
        if !dir.starts_with(top) {
            break;
        }
        for marker in markers {
            let candidate = dir.join(marker);
            if !candidate.exists() {
                continue;
            }
            if nearest.is_none() {
                nearest = Some(dir.to_path_buf());
            }
            if let Some(text) = &record.run.top_most_containing
                && std::fs::read_to_string(&candidate).is_ok_and(|content| content.contains(text))
            {
                top_most = Some(dir.to_path_buf());
            }
        }
        if dir == top {
            break;
        }
    }
    top_most.or(nearest).unwrap_or_else(|| top.to_path_buf())
}

/// Whether `record` applies at `file`: a tool the project must configure
/// (biome, eslint, prettier) runs only where its configuration is.
pub fn applies(record: &ToolRecord, file: &Path, top: &Path) -> bool {
    if record.run.config_files.is_empty() {
        return true;
    }
    config_file(record, file, top).is_some()
}

/// The configuration file that makes `record` apply, nearest first.
pub fn config_file(record: &ToolRecord, file: &Path, top: &Path) -> Option<PathBuf> {
    let start = file.parent().unwrap_or(top);
    for dir in start.ancestors() {
        if !dir.starts_with(top) {
            break;
        }
        for name in &record.run.config_files {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if dir == top {
            break;
        }
    }
    None
}

/// The roots of `record` within one checkout (`boundary`, a repository,
/// a worktree or the project's folder): every folder with a root marker,
/// each under the root it belongs to (for check commands). Never inside
/// another repository: folders with their own `.git` are left out, but
/// for the checkout's `submodules` (relative to it), which are part of it.
pub fn roots_in(boundary: &Path, record: &ToolRecord, submodules: &[String]) -> Vec<PathBuf> {
    walk_roots(boundary, record, submodules).0
}

/// A modification time, or `None` for what is not there.
type Stamp = (PathBuf, Option<SystemTime>);

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// [`roots_in`], and what tells its answer changed: the folders walked
/// (a marker or a configuration file created or removed in one, a folder
/// added, changes its time), the marker files found (their text decides
/// the top-most root) and the `.gitignore` files that chose what was
/// walked.
fn walk_roots(
    boundary: &Path,
    record: &ToolRecord,
    submodules: &[String],
) -> (Vec<PathBuf>, Vec<Stamp>) {
    if record.run.root_markers.is_empty() {
        return (vec![boundary.to_path_buf()], Vec::new());
    }
    let mut stamps: Vec<Stamp> = Vec::new();
    let mut found = Vec::new();
    let top = boundary.to_path_buf();
    let parts: Vec<PathBuf> = submodules.iter().map(|path| boundary.join(path)).collect();
    let walker = ignore::WalkBuilder::new(boundary)
        .max_depth(Some(6))
        .hidden(true)
        .filter_entry(move |entry| {
            let path = entry.path();
            !(entry.file_type().is_some_and(|kind| kind.is_dir())
                && path != top
                && path.join(".git").exists()
                && !parts.iter().any(|part| part == path))
        })
        .build();
    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            let time = entry.metadata().ok().and_then(|meta| meta.modified().ok());
            stamps.push((entry.path().to_path_buf(), time));
            let ignore = entry.path().join(".gitignore");
            if let Some(time) = stamp(&ignore) {
                stamps.push((ignore, Some(time)));
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if record.run.root_markers.iter().any(|marker| *marker == name) {
            stamps.push((entry.path().to_path_buf(), stamp(entry.path())));
            let root = find_root(entry.path(), boundary, record);
            if !found.contains(&root) {
                found.push(root);
            }
        }
    }
    // A root inside another of the same tool belongs to it.
    let mut roots: Vec<PathBuf> = found
        .iter()
        .filter(|root| {
            !found
                .iter()
                .any(|other| other != *root && root.starts_with(other))
        })
        .cloned()
        .collect();
    roots.sort();
    (roots, stamps)
}

/// What decides a check's roots in a checkout: the checkout, its
/// submodules, and the record's markers and configuration files.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RootsKey {
    boundary: PathBuf,
    submodules: Vec<String>,
    markers: Vec<String>,
    top_most_containing: Option<String>,
    config_files: Vec<String>,
}

/// The roots found in one checkout for one check, and what tells they
/// changed.
struct KeptRoots {
    roots: Vec<PathBuf>,
    stamps: Vec<Stamp>,
}

/// A time this close to a walk may hide a change made just after it (file
/// systems keep times in coarse ticks), so such an answer is not kept.
const RACY: Duration = Duration::from_secs(2);

/// The roots of check commands per checkout, kept (the Check view's
/// chooser lists them often; walking a project with a hundred worktrees
/// takes seconds). A checkout's are found again only when one of the
/// folders walked, a marker file or a `.gitignore` changed: their times are
/// compared on each use, a `stat` per folder instead of a walk. Checkouts
/// discovery no longer lists are forgotten with [`Roots::keep_only`].
#[derive(Default)]
pub struct Roots {
    kept: Mutex<HashMap<RootsKey, KeptRoots>>,
}

impl Roots {
    /// The roots of `record` in the checkout at `boundary` where it
    /// applies ([`roots_in`] and [`applies`]): kept, or found again when
    /// something changed. Blocking: it reads the disk.
    pub fn of(&self, boundary: &Path, record: &ToolRecord, submodules: &[String]) -> Vec<PathBuf> {
        let key = RootsKey {
            boundary: boundary.to_path_buf(),
            submodules: submodules.to_vec(),
            markers: record.run.root_markers.clone(),
            top_most_containing: record.run.top_most_containing.clone(),
            config_files: record.run.config_files.clone(),
        };
        if let Some(kept) = lock(&self.kept).get(&key)
            && kept.stamps.iter().all(|(path, time)| stamp(path) == *time)
        {
            return kept.roots.clone();
        }
        let started = SystemTime::now();
        let (roots, stamps) = walk_roots(boundary, record, submodules);
        let roots: Vec<PathBuf> = roots
            .into_iter()
            .filter(|root| applies(record, &root.join("."), boundary))
            .collect();
        let racy = started.checked_sub(RACY).is_none_or(|settled| {
            stamps
                .iter()
                .any(|(_, time)| time.is_none_or(|time| time > settled))
        });
        if racy {
            lock(&self.kept).remove(&key);
            return roots;
        }
        lock(&self.kept).insert(
            key,
            KeptRoots {
                roots: roots.clone(),
                stamps,
            },
        );
        roots
    }

    /// Forgets the checkouts under `top` other than `boundaries`.
    pub fn keep_only(&self, top: &Path, boundaries: &[PathBuf]) {
        lock(&self.kept)
            .retain(|key, _| !key.boundary.starts_with(top) || boundaries.contains(&key.boundary));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn record(value: serde_json::Value) -> ToolRecord {
        serde_json::from_value(value).unwrap()
    }

    /// Sets the times of everything under `dir` to one long past, as if
    /// nothing had changed lately.
    fn settle(dir: &Path) {
        let past = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for entry in walkdir(dir) {
            std::fs::File::open(&entry)
                .unwrap()
                .set_modified(past)
                .unwrap();
        }
    }

    fn walkdir(dir: &Path) -> Vec<PathBuf> {
        let mut all = vec![dir.to_path_buf()];
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                all.extend(walkdir(&entry.path()));
            } else {
                all.push(entry.path());
            }
        }
        all
    }

    #[test]
    fn kept_roots_change_when_a_marker_comes_or_goes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("app/src")).unwrap();
        std::fs::create_dir_all(repo.join("lib/src")).unwrap();
        std::fs::write(repo.join("app/Cargo.toml"), "[package]\n").unwrap();
        let check = record(json!({
            "id": "c", "name": "c", "kind": "check",
            "run": { "command": ["true"], "rootMarkers": ["Cargo.toml"] }
        }));
        let kept = Roots::default();
        settle(repo);
        assert_eq!(kept.of(repo, &check, &[]), [repo.join("app")]);

        // Kept: a marker whose folder's time did not change is not seen.
        std::fs::write(repo.join("lib/Cargo.toml"), "[package]\n").unwrap();
        settle(repo);
        assert_eq!(kept.of(repo, &check, &[]), [repo.join("app")], "kept");
        kept.keep_only(repo, &[]);
        assert!(lock(&kept.kept).is_empty(), "forgotten");
        assert_eq!(kept.of(repo, &check, &[]).len(), 2);

        // A marker in a folder already walked, and one in a new folder.
        std::fs::remove_file(repo.join("lib/Cargo.toml")).unwrap();
        assert_eq!(kept.of(repo, &check, &[]), [repo.join("app")]);
        settle(repo);
        assert_eq!(kept.of(repo, &check, &[]), [repo.join("app")]);
        std::fs::create_dir_all(repo.join("tools/gen")).unwrap();
        std::fs::write(repo.join("tools/gen/Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(
            kept.of(repo, &check, &[]),
            [repo.join("app"), repo.join("tools/gen")]
        );

        // An ignored folder's markers count once it is no longer ignored.
        std::fs::write(repo.join(".gitignore"), "lib/\n").unwrap();
        std::fs::write(repo.join("lib/Cargo.toml"), "[package]\n").unwrap();
        settle(repo);
        assert_eq!(kept.of(repo, &check, &[]).len(), 2);
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        assert_eq!(kept.of(repo, &check, &[]).len(), 3);
    }

    #[test]
    fn rust_takes_the_top_most_cargo_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let crate_dir = workspace.join("crates/app/src");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        std::fs::write(workspace.join("crates/app/Cargo.toml"), "[package]\n").unwrap();
        let rust = record(json!({
            "id": "rust-analyzer", "name": "rust-analyzer", "kind": "languageServer",
            "run": { "command": ["rust-analyzer"], "rootMarkers": ["Cargo.toml"], "topMostContaining": "[workspace]" }
        }));
        assert_eq!(
            find_root(&crate_dir.join("main.rs"), workspace, &rust),
            workspace
        );
        let nearest = record(json!({
            "id": "x", "name": "x", "kind": "languageServer",
            "run": { "command": ["x"], "rootMarkers": ["Cargo.toml"] }
        }));
        assert_eq!(
            find_root(&crate_dir.join("main.rs"), workspace, &nearest),
            workspace.join("crates/app")
        );
        assert_eq!(roots_in(workspace, &rust, &[]), [workspace.to_path_buf()]);
    }

    #[test]
    fn tools_follow_the_layers() {
        let context: ToolContext = serde_json::from_value(json!({
            "records": [
                { "id": "tsls", "name": "tsls", "kind": "languageServer", "run": { "command": ["tsls"] } },
                { "id": "eslint", "name": "eslint", "kind": "languageServer", "run": { "command": ["eslint"] } }
            ],
            "languages": { "typescript": ["tsls", "eslint"] },
            "machine": { "tools": { "eslint": { "off": true } } },
            "project": { "records": [{ "id": "mine", "name": "mine", "kind": "languageServer",
                "languages": ["typescript"], "run": { "command": ["mine"] } }] }
        }))
        .unwrap();
        let effective = Effective::new(&context, None);
        let ids: Vec<&str> = effective
            .tools_for("typescript")
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids, ["tsls", "mine"]);
        assert_eq!(effective.origins["mine"], "project");
        let repository: ToolSettings =
            serde_json::from_value(json!({ "languages": { "typescript": { "off": true } } }))
                .unwrap();
        let effective = Effective::new(&context, Some((repository, PathBuf::from("x"))));
        assert!(effective.tools_for("typescript").is_empty());
    }
}

#[cfg(test)]
mod repository_file {
    use super::{REPOSITORY_FILE, repository_settings};

    #[test]
    fn a_repository_chooses_tools_but_declares_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ahab")).unwrap();
        std::fs::write(
            dir.path().join(REPOSITORY_FILE),
            r#"{"records":[{"id":"rust-analyzer","name":"x","kind":"languageServer","run":{"command":["sh","-c","touch pwned"]}}]}"#,
        )
        .unwrap();
        let (settings, _) = repository_settings(dir.path(), dir.path()).unwrap();
        assert!(settings.records.is_empty());
    }
}

#[cfg(test)]
mod within_checkouts {
    use serde_json::json;

    use super::*;

    #[test]
    fn roots_stay_inside_their_checkout_but_take_in_submodules() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("svc");
        for folder in [
            "svc/.git",
            "svc/core",
            "svc/vendor/lib/.git",
            "svc/clone/.git",
            "svc/clone/inner",
        ] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        for file in [
            "svc/core/pom.xml",
            "svc/vendor/lib/pom.xml",
            "svc/clone/inner/pom.xml",
        ] {
            std::fs::write(dir.path().join(file), "<project/>").unwrap();
        }
        let maven = serde_json::from_value::<ToolRecord>(json!({
            "id": "mvn", "name": "mvn", "kind": "check",
            "run": { "command": ["mvn"], "rootMarkers": ["pom.xml"] }
        }))
        .unwrap();
        // The nested clone is another repository; the submodule is part of svc.
        assert_eq!(
            roots_in(&repo, &maven, &["vendor/lib".to_owned()]),
            [repo.join("core"), repo.join("vendor/lib")]
        );
        assert_eq!(roots_in(&repo, &maven, &[]), [repo.join("core")]);
        let nearest = |file: &str| find_root(&repo.join(file), &repo.join("core"), &maven);
        assert_eq!(nearest("core/src/A.java"), repo.join("core"));
        // A root never climbs out of the checkout it starts in.
        let worktree = dir.path().join("svc/clone/inner/src");
        std::fs::create_dir_all(&worktree).unwrap();
        assert_eq!(
            find_root(
                &worktree.join("A.java"),
                &dir.path().join("svc/clone"),
                &maven
            ),
            dir.path().join("svc/clone/inner")
        );
        let no_marker = dir.path().join("svc/clone/README");
        let loose = serde_json::from_value::<ToolRecord>(json!({
            "id": "x", "name": "x", "kind": "check",
            "run": { "command": ["x"], "rootMarkers": ["build.gradle"] }
        }))
        .unwrap();
        assert_eq!(
            find_root(&no_marker, &dir.path().join("svc/clone"), &loose),
            dir.path().join("svc/clone")
        );
    }
}
