//! The daemon's side of Problems: running check commands at their roots,
//! command-line linters on a file, and formatters (stdin → stdout).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio::process::Command;

use crate::modules::projects::Checkouts;
use crate::modules::tools::problems::api::{
    CheckAnswer, CheckResultDto, CheckRootDto, CheckRootsDto,
};
use crate::modules::tools::problems::model::{ByFile, Diagnostic, read_output};
use crate::modules::tools::registry::api::ToolContext;
use crate::modules::tools::registry::daemon::plan::{
    Effective, Roots, applies, find_root, repository_settings,
};
use crate::modules::tools::registry::daemon::{Launch, Readiness, ToolHost};
use crate::modules::tools::registry::model::{ToolKind, ToolRecord};
use crate::shared::sync::lock;

/// How long a check command may take.
const CHECK_TIMEOUT: Duration = Duration::from_mins(15);
/// How long a linter or a formatter may take.
const FILE_TIMEOUT: Duration = Duration::from_secs(60);
/// What is kept of a tool's output for its error.
const OUTPUT_LIMIT: usize = 32 << 20;

/// What a tool printed.
pub struct Ran {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Runs `launch`, giving it `input` on stdin.
pub async fn run(launch: &Launch, input: Option<&str>, timeout: Duration) -> Result<Ran, String> {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&launch.root)
        .env_clear()
        .envs(&launch.env)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", launch.program.display()))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        let input = input.to_owned();
        tokio::spawn(async move {
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| "it took too long and was stopped".to_owned())?
        .map_err(|error| error.to_string())?;
    let text = |bytes: &[u8]| {
        let bytes = &bytes[..bytes.len().min(OUTPUT_LIMIT)];
        String::from_utf8_lossy(bytes).into_owned()
    };
    Ok(Ran {
        stdout: text(&out.stdout),
        stderr: text(&out.stderr),
        success: out.status.success(),
    })
}

/// The last line worth showing of a failed tool.
fn last_line(ran: &Ran) -> String {
    ran.stderr
        .lines()
        .chain(ran.stdout.lines())
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("it failed without saying why")
        .to_owned()
}

/// Runs a linter or a check and reads its problems.
pub async fn diagnose(
    record: &ToolRecord,
    launch: &Launch,
    timeout: Duration,
) -> Result<ByFile, String> {
    let Some(output) = &record.run.output else {
        return Err("its record declares no output".to_owned());
    };
    // Reports written while it ran; a file system's times may be coarse.
    let since = std::time::SystemTime::now() - Duration::from_secs(2);
    let ran = run(launch, None, timeout).await?;
    if !output.reports.is_empty() {
        let root = launch.root.clone();
        let patterns = output.reports.clone();
        let files = tokio::task::spawn_blocking(move || report_files(&root, &patterns, since))
            .await
            .unwrap_or_default();
        if files.is_empty() {
            return if ran.success {
                Ok(ByFile::new())
            } else {
                Err(last_line(&ran))
            };
        }
        let mut all = ByFile::new();
        for file in files {
            let Ok(text) = tokio::fs::read_to_string(&file).await else {
                continue;
            };
            match read_output(output, &text, &launch.root, &record.id) {
                Ok(found) => {
                    for (path, items) in found {
                        let list = all.entry(path).or_default();
                        for item in items {
                            if !list.contains(&item) {
                                list.push(item);
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::info!(tool = %record.id, file = %file.display(), %error, "a report could not be read");
                }
            }
        }
        return Ok(all);
    }
    let text = match output.stream.as_deref() {
        Some("stderr") => ran.stderr.clone(),
        Some("both") => format!("{}\n{}", ran.stdout, ran.stderr),
        _ => ran.stdout.clone(),
    };
    match read_output(output, &text, &launch.root, &record.id) {
        Ok(found) if !found.is_empty() || ran.success => Ok(found),
        Ok(_) if text.trim().is_empty() => Err(last_line(&ran)),
        Ok(found) => Ok(found),
        Err(_) if !ran.success => Err(last_line(&ran)),
        Err(error) => Err(error),
    }
}

/// At most this many report files are read per run.
const MAX_REPORTS: usize = 500;

/// The report files under `root` that `patterns` (globs relative to it)
/// match and that were written after `since`. Patterns that leave the root
/// match nothing; ignore files do not hide reports (`build/` usually is).
pub fn report_files(
    root: &Path,
    patterns: &[String],
    since: std::time::SystemTime,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for pattern in patterns {
        let pattern = pattern.trim_start_matches("./");
        if pattern.is_empty()
            || Path::new(pattern).is_absolute()
            || pattern.split('/').any(|part| part == "..")
        {
            continue;
        }
        let Ok(glob) = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
        else {
            continue;
        };
        let matcher = glob.compile_matcher();
        // Walk from the pattern's folders before its first wildcard.
        let fixed: Vec<&str> = pattern
            .split('/')
            .take_while(|part| !part.contains(['*', '?', '[', '{']))
            .collect();
        let fixed = if fixed.len() == pattern.split('/').count() {
            &fixed[..fixed.len() - 1]
        } else {
            &fixed[..]
        };
        let base = root.join(fixed.join("/"));
        if !base.is_dir() {
            continue;
        }
        let walker = ignore::WalkBuilder::new(&base)
            .standard_filters(false)
            .follow_links(false)
            .max_depth(Some(16))
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            let fresh = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .is_some_and(|modified| modified >= since);
            if fresh && matcher.is_match(relative) && !found.iter().any(|f| f == entry.path()) {
                found.push(entry.path().to_path_buf());
                if found.len() >= MAX_REPORTS {
                    return found;
                }
            }
        }
    }
    found.sort();
    found
}

/// Formats `text` with a formatter's command (stdin → stdout).
pub async fn format(launch: &Launch, text: &str) -> Result<String, String> {
    let ran = run(launch, Some(text), FILE_TIMEOUT).await?;
    if !ran.success {
        return Err(last_line(&ran));
    }
    if ran.stdout.is_empty() && !text.is_empty() {
        return Err("the formatter printed nothing".to_owned());
    }
    Ok(ran.stdout)
}

/// Runs a linter on one file.
pub async fn lint(record: &ToolRecord, launch: &Launch) -> Result<ByFile, String> {
    diagnose(record, launch, FILE_TIMEOUT).await
}

/// `found` with paths relative to `project`; those outside it dropped.
pub fn relative(found: ByFile, project: &Path) -> BTreeMap<String, Vec<Diagnostic>> {
    found
        .into_iter()
        .filter_map(|(path, items)| {
            let path = normalize(&path);
            path.strip_prefix(project)
                .ok()
                .map(|rel| (rel.to_string_lossy().into_owned(), items))
        })
        .collect()
}

/// `a/./b/../c` as `a/c`, without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Where checks run: everywhere, within some repositories or worktrees, or
/// at the roots of some files.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Files (relative to the project) whose roots are checked.
    pub paths: Option<Vec<String>>,
    /// Repositories or worktrees (folders relative to the project).
    pub checkouts: Option<Vec<String>>,
}

/// A check at a root, before it runs.
#[derive(Debug, Clone)]
pub struct Planned {
    pub record: ToolRecord,
    pub root: PathBuf,
    /// The repository or worktree it is in, relative to the project.
    pub checkout: String,
}

/// The check commands that run on `trigger` (or those named).
fn chosen(effective: &Effective, trigger: &str, only: Option<&[String]>) -> Vec<ToolRecord> {
    let mut seen = Vec::new();
    let mut checks: Vec<ToolRecord> = Vec::new();
    for language in effective.languages.keys() {
        for record in effective.tools_for(language) {
            if record.kind != ToolKind::Check || seen.contains(&record.id) {
                continue;
            }
            seen.push(record.id.clone());
            let named = only.is_some_and(|only| only.contains(&record.id));
            if named || (only.is_none() && effective.triggers(record).iter().any(|t| t == trigger))
            {
                checks.push(record.clone());
            }
        }
    }
    checks
}

/// Where `checks` run in `project` within `scope`: each root once per
/// check, found within its repository or worktree (never across two), and
/// kept in `roots` until a folder walked changes. Blocking: it walks the
/// checkouts that changed.
pub fn plan(
    checks: &[ToolRecord],
    project: &Path,
    found: &Checkouts,
    scope: &Scope,
    roots: &Roots,
) -> Vec<Planned> {
    let mut planned: Vec<Planned> = Vec::new();
    let mut add = |record: &ToolRecord, root: PathBuf, checkout: &str| {
        if !planned
            .iter()
            .any(|p| p.record.id == record.id && p.root == root)
        {
            planned.push(Planned {
                record: record.clone(),
                root,
                checkout: checkout.to_owned(),
            });
        }
    };
    if let Some(paths) = &scope.paths {
        for path in paths {
            let clean = path.trim_matches('/');
            if clean.split('/').any(|part| part == "..") || Path::new(clean).is_absolute() {
                continue;
            }
            let checkout = found.of(clean);
            let boundary = project.join(&checkout.path);
            let file = project.join(clean);
            for record in checks {
                if applies(record, &file, &boundary) {
                    add(record, find_root(&file, &boundary, record), &checkout.path);
                }
            }
        }
        return planned;
    }
    let boundaries = found.boundaries();
    roots.keep_only(
        project,
        &boundaries
            .iter()
            .map(|checkout| project.join(&checkout.path))
            .collect::<Vec<_>>(),
    );
    for checkout in boundaries {
        if scope
            .checkouts
            .as_ref()
            .is_some_and(|wanted| !wanted.iter().any(|w| w.trim_matches('/') == checkout.path))
        {
            continue;
        }
        let boundary = project.join(&checkout.path);
        for record in checks {
            for root in roots.of(&boundary, record, &checkout.submodules) {
                add(record, root, &checkout.path);
            }
        }
    }
    planned
}

/// The repositories and worktrees (folders relative to the project) a
/// scope covers: those of its files, those named, or all.
pub fn scoped_checkouts(found: &Checkouts, scope: &Scope) -> Vec<String> {
    let mut folders: Vec<String> = match (&scope.paths, &scope.checkouts) {
        (Some(paths), _) => paths.iter().map(|path| found.of(path).path).collect(),
        (None, Some(checkouts)) => checkouts
            .iter()
            .map(|c| c.trim_matches('/').to_owned())
            .filter(|c| found.boundaries().iter().any(|b| &b.path == c))
            .collect(),
        (None, None) => found.boundaries().into_iter().map(|c| c.path).collect(),
    };
    folders.sort();
    folders.dedup();
    folders
}

/// Where the checks that run on `trigger` would run.
pub async fn roots(
    host: &ToolHost,
    context: &ToolContext,
    project: &Path,
    trigger: &str,
) -> CheckRootsDto {
    let effective = Effective::new(context, repository_settings(project, project));
    let checks = chosen(&effective, trigger, None);
    let found = host.checkouts(project).await;
    let folder = project.to_path_buf();
    let kept = Arc::clone(&host.roots);
    let planned = tokio::task::spawn_blocking(move || {
        plan(&checks, &folder, &found, &Scope::default(), &kept)
    })
    .await
    .unwrap_or_default();
    CheckRootsDto {
        roots: planned
            .into_iter()
            .map(|p| CheckRootDto {
                tool: p.record.id,
                name: p.record.name,
                root: shown(&p.root, project),
                checkout: p.checkout,
            })
            .collect(),
        ask_above: ASK_ABOVE,
    }
}

/// With more roots than this, Check project asks which to check.
pub const ASK_ABOVE: u32 = 5;

/// A folder relative to the project, as shown.
fn shown(root: &Path, project: &Path) -> String {
    root.strip_prefix(project)
        .map(|rel| rel.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The check commands that run on `trigger` (or those named), at their
/// roots within `scope`; at most the machine's `maxChecks` at once, the
/// rest waiting.
pub async fn check(
    host: &ToolHost,
    context: &ToolContext,
    project_id: uuid::Uuid,
    project: &Path,
    trigger: &str,
    only: Option<&[String]>,
    scope: Scope,
) -> CheckAnswer {
    let effective = Effective::new(context, repository_settings(project, project));
    let checks = chosen(&effective, trigger, only);
    let found = host.checkouts(project).await;
    // The repositories an agent's turn changed were used.
    let mut told: Vec<String> = Vec::new();
    for path in scope.paths.iter().flatten() {
        let checkout = found.of(path);
        if !told.contains(&checkout.path) {
            (host.report)(
                "tools.activity",
                serde_json::json!({
                    "project": project_id,
                    "repository": checkout.repository,
                    "worktree": checkout.worktree(),
                }),
            );
            told.push(checkout.path);
        }
    }
    if checks.is_empty() {
        return CheckAnswer::default();
    }
    let folder = project.to_path_buf();
    let kept = Arc::clone(&host.roots);
    let planned =
        tokio::task::spawn_blocking(move || plan(&checks, &folder, &found, &scope, &kept))
            .await
            .unwrap_or_default();
    let max = effective.settings.max_checks();
    let effective = &effective;
    let results = futures_util::future::join_all(planned.into_iter().map(|p| async move {
        let _turn = host.checks.acquire(max).await;
        let mut result = check_at(host, &p.record, &p.root, project, effective).await;
        result.checkout = p.checkout;
        result
    }))
    .await;
    CheckAnswer { results }
}

/// Check commands running on a machine: at most a setting's number at
/// once; the rest wait their turn, in order.
#[derive(Default)]
pub struct Limiter {
    running: std::sync::Mutex<usize>,
    freed: tokio::sync::Notify,
}

/// A running check's place; freed when dropped.
pub struct Turn<'a>(&'a Limiter);

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        *lock(&self.0.running) -= 1;
        self.0.freed.notify_waiters();
    }
}

impl Limiter {
    /// Waits until fewer than `max` run, and takes a place.
    pub async fn acquire(&self, max: u32) -> Turn<'_> {
        let max = usize::try_from(max.max(1)).unwrap_or(1);
        loop {
            let freed = self.freed.notified();
            tokio::pin!(freed);
            freed.as_mut().enable();
            {
                let mut running = lock(&self.running);
                if *running < max {
                    *running += 1;
                    return Turn(self);
                }
            }
            freed.await;
        }
    }

    /// How many run now.
    pub fn running(&self) -> usize {
        *lock(&self.running)
    }
}

/// One check at one root.
pub async fn check_at(
    host: &ToolHost,
    record: &ToolRecord,
    root: &Path,
    project: &Path,
    effective: &Effective,
) -> CheckResultDto {
    let mut result = CheckResultDto {
        tool: record.id.clone(),
        name: record.name.clone(),
        root: shown(root, project),
        ok: false,
        message: None,
        checkout: String::new(),
        files: BTreeMap::new(),
    };
    match host.prepare(record, root, project, effective, None).await {
        Readiness::Ready(launch) => match diagnose(record, &launch, CHECK_TIMEOUT).await {
            Ok(found) => {
                result.ok = true;
                result.files = relative(found, project);
            }
            Err(error) => result.message = Some(error),
        },
        Readiness::Missing { reason, .. } | Readiness::CannotRun { reason, .. } => {
            result.message = Some(reason);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_the_fresh_files_their_globs_match_inside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for file in [
            "build/reports/checkstyle/main.xml",
            "build/reports/checkstyle/test.xml",
            "build/reports/checkstyle/old/nested.xml",
            "app/build/reports/pmd/main.xml",
            "build/reports/checkstyle/main.html",
        ] {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            std::fs::write(root.join(file), "<checkstyle/>").unwrap();
        }
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
        let since = std::time::SystemTime::now() - Duration::from_secs(60);
        let names = |patterns: &[&str], since| {
            let patterns: Vec<String> = patterns.iter().map(|p| (*p).to_owned()).collect();
            report_files(root, &patterns, since)
                .iter()
                .map(|f| f.strip_prefix(root).unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&["build/reports/checkstyle/*.xml"], since),
            [
                "build/reports/checkstyle/main.xml",
                "build/reports/checkstyle/test.xml"
            ]
        );
        assert_eq!(
            names(&["**/pmd/*.xml"], since),
            ["app/build/reports/pmd/main.xml"]
        );
        assert_eq!(
            names(&["build/reports/checkstyle/main.xml"], since),
            ["build/reports/checkstyle/main.xml"]
        );
        assert!(names(&["../*.xml", "/etc/*.xml"], since).is_empty());
        // Reports from before the run are stale.
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        assert!(names(&["build/reports/checkstyle/*.xml"], later).is_empty());
    }

    #[test]
    fn a_turns_files_check_only_their_roots_in_their_checkouts() {
        use crate::modules::projects::{Checkout, CheckoutKind};
        let dir = tempfile::tempdir().unwrap();
        let w = dir.path();
        for folder in [
            "a/.git",
            "a/core/src",
            "a/web/src",
            "b/.git",
            "b/src",
            "loose/src",
        ] {
            std::fs::create_dir_all(w.join(folder)).unwrap();
        }
        for marker in [
            "a/core/Cargo.toml",
            "a/web/Cargo.toml",
            "b/Cargo.toml",
            "loose/Cargo.toml",
        ] {
            std::fs::write(w.join(marker), "[package]\n").unwrap();
        }
        let record: ToolRecord = serde_json::from_value(serde_json::json!({
            "id": "c", "name": "c", "kind": "check",
            "run": { "command": ["true"], "rootMarkers": ["Cargo.toml"] }
        }))
        .unwrap();
        let checkout = |path: &str| Checkout {
            path: path.to_owned(),
            repository: path.to_owned(),
            kind: CheckoutKind::Repository,
            branch: None,
            submodules: Vec::new(),
        };
        let found = Checkouts {
            inside: false,
            items: vec![checkout("a"), checkout("b")],
        };
        let kept = Roots::default();
        let roots = |scope: Scope| {
            plan(std::slice::from_ref(&record), w, &found, &scope, &kept)
                .into_iter()
                .map(|p| (shown(&p.root, w), p.checkout))
                .collect::<Vec<_>>()
        };
        let turn = Scope {
            paths: Some(vec![
                "a/web/src/x.rs".into(),
                "a/web/src/y.rs".into(),
                "../escape".into(),
            ]),
            checkouts: None,
        };
        assert_eq!(roots(turn), [("a/web".to_owned(), "a".to_owned())]);
        let chosen = Scope {
            paths: None,
            checkouts: Some(vec!["b".into(), String::new()]),
        };
        assert_eq!(
            roots(chosen),
            [
                ("b".to_owned(), "b".to_owned()),
                ("loose".to_owned(), String::new())
            ]
        );
        assert_eq!(roots(Scope::default()).len(), 4);
    }

    #[tokio::test]
    async fn checks_wait_their_turn() {
        let limiter = std::sync::Arc::new(Limiter::default());
        let first = limiter.acquire(2).await;
        let _second = limiter.acquire(2).await;
        assert_eq!(limiter.running(), 2);
        let waiting = {
            let limiter = std::sync::Arc::clone(&limiter);
            tokio::spawn(async move {
                let _third = limiter.acquire(2).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished(), "a third waits");
        drop(first);
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(limiter.running(), 1);
    }

    #[test]
    fn paths_outside_the_project_are_dropped() {
        let found = ByFile::from([
            (PathBuf::from("/w/src/../src/a.rs"), vec![]),
            (PathBuf::from("/elsewhere/b.rs"), vec![]),
        ]);
        let kept = relative(found, Path::new("/w"));
        assert_eq!(kept.keys().collect::<Vec<_>>(), ["src/a.rs"]);
    }
}
