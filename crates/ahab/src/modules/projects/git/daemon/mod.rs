//! Running the machine's own `git` for a project: its
//! configuration, hooks and credentials apply; it never waits for a
//! person; only named operations with checked arguments run; one operation
//! that writes at a time per repository.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use std::time::SystemTime;

use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::modules::projects::git::api::{
    BranchDto, BranchesDto, ChangeDto, CommitDetailDto, CommitDto, DiffDto, DiffGroup, GitOp,
    LogDto, OutputDto, RepositoriesDto, RepositoryDto, StashDto, StashesDto, StatusDto,
};
use crate::modules::projects::git::daemon::discover::Discovery;
use crate::modules::projects::git::model::{
    COMMIT_FORMAT, MAX_SIDE, checked, checked_all, checked_commit, classify, commit_from,
    looks_binary, name_status, numstat, side, status_name,
};
use crate::shared::error::{DaemonError, fail};
use crate::shared::sync::lock;

/// Local operations stop after this long.
const LOCAL: Duration = Duration::from_secs(60);
/// Fetch, pull and push stop after this long.
const NETWORK: Duration = Duration::from_secs(300);
/// Untracked files above this size have no line count.
const COUNT_LIMIT: u64 = 1024 * 1024;

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, DaemonError> {
    serde_json::to_value(value).map_err(|error| fail("git_failed", error.to_string()))
}

/// Runs `git` in `dir`, never waiting for a person: no prompts, no editor,
/// no pager, C locale, colour off.
async fn git(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    limit: Duration,
) -> Result<std::process::Output, DaemonError> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(["-c", "color.ui=false", "-c", "core.quotepath=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .env("GCM_INTERACTIVE", "never")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        // SSH fails instead of asking for a passphrase or a host key.
        command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            fail("git_missing", "Git is not installed on this machine.")
        } else {
            fail("git_failed", error.to_string())
        }
    })?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin
            .write_all(input)
            .await
            .map_err(|error| fail("git_failed", error.to_string()))?;
    }
    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(fail("git_failed", error.to_string())),
        Err(_) => Err(fail(
            "git_timeout",
            format!("Git did not finish within {} seconds.", limit.as_secs()),
        )),
    }
}

/// Runs `git` and answers its standard output, or what went wrong.
async fn run(dir: &Path, args: &[&str], limit: Duration) -> Result<String, DaemonError> {
    let output = git(dir, args, None, limit).await?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(classify(&format!("{stderr}\n{stdout}")))
    }
}

/// Bytes of a blob (`HEAD:path`, `:path`); `None` when it does not exist.
async fn blob(dir: &Path, spec: &str) -> Result<Option<Vec<u8>>, DaemonError> {
    let output = git(dir, &["show", spec], None, LOCAL).await?;
    Ok(output.status.success().then_some(output.stdout))
}

/// A branch name, checked by Git itself.
async fn checked_branch<'a>(dir: &Path, name: &'a str) -> Result<&'a str, DaemonError> {
    if name.starts_with('-') || name.is_empty() {
        return Err(fail("invalid_branch", format!("Not a branch name: {name}")));
    }
    run(dir, &["check-ref-format", "--branch", name], LOCAL)
        .await
        .map_err(|_| fail("invalid_branch", format!("Not a branch name: {name}")))?;
    Ok(name)
}

/// The repository a folder is in, and the folder's place in it.
pub struct Repository {
    pub root: PathBuf,
    pub prefix: String,
}

async fn repository(folder: &Path) -> Result<Option<Repository>, DaemonError> {
    match run(
        folder,
        &["rev-parse", "--show-toplevel", "--show-prefix"],
        LOCAL,
    )
    .await
    {
        Ok(out) => {
            let mut lines = out.lines();
            let root = PathBuf::from(lines.next().unwrap_or_default());
            let prefix = lines.next().unwrap_or_default().to_owned();
            Ok(Some(Repository { root, prefix }))
        }
        Err(error) if error.code == "not_a_repository" => Ok(None),
        Err(error) => Err(error),
    }
}

/// Where a checkout's files and Git's own are: what watching it watches.
pub struct Places {
    /// The checkout's top folder.
    pub root: PathBuf,
    /// Its Git folder (`.git`, or a worktree's `.git/worktrees/<name>`).
    pub git_dir: PathBuf,
    /// The repository's shared Git folder, with the refs.
    pub common: PathBuf,
}

/// The places of the checkout that `repository_path` (inside `folder`, or
/// `folder` itself) is in; `not_a_repository` when there is none.
pub async fn places(folder: &Path, repository_path: Option<&str>) -> Result<Places, DaemonError> {
    let folder = match repository_path.filter(|p| !p.is_empty()) {
        Some(path) => folder.join(checked(path)?),
        None => folder.to_path_buf(),
    };
    let out = run(
        &folder,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ],
        LOCAL,
    )
    .await?;
    let mut lines = out.lines().map(PathBuf::from);
    match (lines.next(), lines.next(), lines.next()) {
        (Some(root), Some(git_dir), Some(common)) => Ok(Places {
            root,
            git_dir,
            common,
        }),
        _ => Err(fail(
            "not_a_repository",
            "The project is not in a Git repository.",
        )),
    }
}

/// Git, for every project on this machine.
#[derive(Default)]
pub struct Git {
    /// One writing operation at a time per repository.
    locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    /// The repositories found in each project's folder.
    pub discovery: Arc<Discovery>,
}

impl Git {
    fn lock(&self, root: &Path) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(lock(&self.locks).entry(root.to_path_buf()).or_default())
    }

    /// Runs `op` for the project whose folder is `folder`, in the
    /// repository `repository` inside it when given.
    pub async fn run(
        &self,
        folder: &Path,
        repository_path: Option<&str>,
        read_only: bool,
        op: GitOp,
    ) -> Result<Value, DaemonError> {
        if op.writes() && read_only {
            return Err(fail("read_only", "This project is read-only."));
        }
        if op == GitOp::Repositories {
            return to_value(&repositories(&self.discovery, folder).await?);
        }
        let inner;
        let folder = match repository_path.filter(|p| !p.is_empty()) {
            Some(path) => {
                inner = folder.join(checked(path)?);
                if !inner.is_dir() {
                    return Err(fail(
                        "not_a_repository",
                        format!("There is no folder {path} in the project."),
                    ));
                }
                inner.as_path()
            }
            None => folder,
        };
        if op == GitOp::Init {
            run(folder, &["init", "-q"], LOCAL).await?;
            return to_value(&OutputDto::default());
        }
        let Some(repo) = repository(folder).await? else {
            if op == GitOp::Status {
                return to_value(&StatusDto::default());
            }
            return Err(fail(
                "not_a_repository",
                "The project is not in a Git repository.",
            ));
        };
        let limit = if op.network() { NETWORK } else { LOCAL };
        let lock = self.lock(&repo.root);
        let _held = if op.writes() {
            Some(lock.lock().await)
        } else {
            None
        };
        let dir = repo.root.as_path();
        match op {
            GitOp::Status => to_value(&status(&repo).await?),
            GitOp::Diff {
                path,
                old_path,
                group,
                commit,
            } => to_value(&diff(dir, &path, old_path.as_deref(), group, commit.as_deref()).await?),
            GitOp::Stage { paths, all } => {
                if all {
                    run(dir, &["add", "-A"], limit).await?;
                } else {
                    let paths = checked_all(&paths)?;
                    run(dir, &[&["add", "-A", "--"][..], &paths].concat(), limit).await?;
                }
                to_value(&OutputDto::default())
            }
            GitOp::Unstage { paths, all } => {
                let born = has_head(dir).await;
                let paths = if all { vec!["."] } else { checked_all(&paths)? };
                if born {
                    run(
                        dir,
                        &[&["restore", "--staged", "--"][..], &paths].concat(),
                        limit,
                    )
                    .await?;
                } else {
                    let args = [&["rm", "--cached", "-r", "-q", "--"][..], &paths].concat();
                    run(dir, &args, limit).await?;
                }
                to_value(&OutputDto::default())
            }
            GitOp::StagePart {
                path,
                content,
                index,
            } => {
                stage_part(dir, checked(&path)?, &content, index.as_deref()).await?;
                to_value(&OutputDto::default())
            }
            GitOp::Discard { paths } => {
                let paths = checked_all(&paths)?;
                // Only inside the project's folder: discarding deletes.
                if let Some(outside) = paths.iter().find(|p| !p.starts_with(&repo.prefix)) {
                    return Err(fail(
                        "path_outside_folder",
                        format!("{outside} is outside the project's folder."),
                    ));
                }
                let tracked: HashSet<String> = run(
                    dir,
                    &[&["ls-files", "-z", "--"][..], &paths].concat(),
                    limit,
                )
                .await?
                .split('\0')
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect();
                let restore: Vec<&str> = paths
                    .iter()
                    .copied()
                    .filter(|p| tracked.contains(*p))
                    .collect();
                if !restore.is_empty() {
                    run(
                        dir,
                        &[&["restore", "--worktree", "--"][..], &restore].concat(),
                        limit,
                    )
                    .await?;
                }
                for path in paths.iter().filter(|p| !tracked.contains(**p)) {
                    let file = dir.join(path);
                    if file.is_file() || file.is_symlink() {
                        std::fs::remove_file(&file)
                            .map_err(|error| fail("git_failed", error.to_string()))?;
                    }
                }
                to_value(&OutputDto::default())
            }
            GitOp::Commit { message, amend } => {
                let message = message.trim();
                if message.is_empty() && !amend {
                    return Err(fail("invalid_message", "Write a commit message."));
                }
                let mut args = vec!["commit", "-q"];
                if amend {
                    args.push("--amend");
                }
                if message.is_empty() {
                    args.push("--no-edit");
                } else {
                    args.extend(["-F", "-"]);
                }
                let output = git(dir, &args, Some(message.as_bytes()), limit).await?;
                if !output.status.success() {
                    let text = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&output.stderr),
                        String::from_utf8_lossy(&output.stdout)
                    );
                    let mut error = classify(&text);
                    if error.code == "git_failed" {
                        // Most other failures of a commit are its hooks.
                        error.code = "hook_failed";
                    }
                    return Err(error);
                }
                to_value(&OutputDto::default())
            }
            GitOp::Branches => to_value(&branches(dir).await?),
            GitOp::Switch {
                branch,
                remote,
                stash,
            } => {
                if stash {
                    let message = format!("Before switching to {branch}");
                    run(dir, &["stash", "push", "-u", "-m", &message], limit).await?;
                }
                if remote {
                    let local = branch
                        .split_once('/')
                        .map_or(branch.as_str(), |(_, rest)| rest);
                    let local = checked_branch(dir, local).await?;
                    let exists = run(
                        dir,
                        &[
                            "rev-parse",
                            "--verify",
                            "-q",
                            &format!("refs/heads/{local}"),
                        ],
                        limit,
                    )
                    .await
                    .is_ok();
                    if exists {
                        run(dir, &["switch", local], limit).await?;
                    } else {
                        let remote_ref = format!("refs/remotes/{branch}");
                        run(dir, &["rev-parse", "--verify", "-q", &remote_ref], limit)
                            .await
                            .map_err(|_| {
                                fail("invalid_branch", format!("No remote branch {branch}."))
                            })?;
                        run(dir, &["switch", "-c", local, "--track", &branch], limit).await?;
                    }
                } else {
                    let branch = checked_branch(dir, &branch).await?;
                    run(dir, &["switch", branch], limit).await?;
                }
                to_value(&OutputDto::default())
            }
            GitOp::CreateBranch { name, from } => {
                let name = checked_branch(dir, &name).await?;
                let mut args = vec!["switch", "-c", name];
                if let Some(from) = from.as_deref() {
                    if from.starts_with('-') {
                        return Err(fail("invalid_branch", "Not a starting point."));
                    }
                    args.push(from);
                }
                run(dir, &args, limit).await?;
                to_value(&OutputDto::default())
            }
            GitOp::RenameBranch { from, to } => {
                let from = checked_branch(dir, &from).await?;
                let to = checked_branch(dir, &to).await?;
                run(dir, &["branch", "-m", from, to], limit).await?;
                to_value(&OutputDto::default())
            }
            GitOp::DeleteBranch { name, force } => {
                let name = checked_branch(dir, &name).await?;
                run(
                    dir,
                    &["branch", if force { "-D" } else { "-d" }, name],
                    limit,
                )
                .await?;
                to_value(&OutputDto::default())
            }
            GitOp::Fetch => output(run(dir, &["fetch", "--all", "--prune"], limit).await?),
            GitOp::Pull => output(run(dir, &["pull"], limit).await?),
            GitOp::Push => {
                let upstream = run(dir, &["rev-parse", "--abbrev-ref", "@{upstream}"], limit).await;
                if upstream.is_ok() {
                    output(run(dir, &["push"], limit).await?)
                } else {
                    let remotes = run(dir, &["remote"], limit).await?;
                    let remotes: Vec<&str> = remotes.lines().collect();
                    let remote = if remotes.contains(&"origin") {
                        "origin"
                    } else {
                        remotes
                            .first()
                            .copied()
                            .ok_or_else(|| fail("no_remote", "This repository has no remote."))?
                    };
                    output(run(dir, &["push", "-u", remote, "HEAD"], limit).await?)
                }
            }
            GitOp::Log {
                skip,
                limit: count,
                all,
            } => to_value(&log(dir, skip, count.clamp(1, 200), all).await?),
            GitOp::Show { commit } => to_value(&show(dir, checked_commit(&commit)?).await?),
            GitOp::Stashes => to_value(&stashes(dir).await?),
            GitOp::Stash { message, untracked } => {
                let mut args = vec!["stash", "push"];
                if untracked {
                    args.push("-u");
                }
                let message = message.unwrap_or_default();
                if !message.trim().is_empty() {
                    args.extend(["-m", message.trim()]);
                }
                output(run(dir, &args, limit).await?)
            }
            GitOp::StashApply { index, pop } => {
                let name = format!("stash@{{{index}}}");
                output(
                    run(
                        dir,
                        &["stash", if pop { "pop" } else { "apply" }, &name],
                        limit,
                    )
                    .await?,
                )
            }
            GitOp::StashDrop { index } => output(
                run(
                    dir,
                    &["stash", "drop", &format!("stash@{{{index}}}")],
                    limit,
                )
                .await?,
            ),
            GitOp::MarkResolved { paths } => {
                let paths = checked_all(&paths)?;
                run(dir, &[&["add", "--"][..], &paths].concat(), limit).await?;
                to_value(&OutputDto::default())
            }
            GitOp::Continue | GitOp::Abort => {
                let continuing = op == GitOp::Continue;
                let args: &[&str] = match (operation(dir).await?.as_deref(), continuing) {
                    (Some("merge"), true) => &["commit", "--no-edit"],
                    (Some("merge"), false) => &["merge", "--abort"],
                    (Some("rebase"), true) => &["rebase", "--continue"],
                    (Some("rebase"), false) => &["rebase", "--abort"],
                    (Some("cherryPick"), true) => &["cherry-pick", "--continue"],
                    (Some("cherryPick"), false) => &["cherry-pick", "--abort"],
                    (Some("revert"), true) => &["revert", "--continue"],
                    (Some("revert"), false) => &["revert", "--abort"],
                    _ => {
                        return Err(fail(
                            "nothing_to_continue",
                            "No merge or rebase is in progress.",
                        ));
                    }
                };
                output(run(dir, args, limit).await?)
            }
            GitOp::Init | GitOp::Repositories => unreachable_init(),
        }
    }
}

/// The repository the folder is in, or the repositories inside it: kept,
/// and found again when their `.git` entries change.
async fn repositories(
    discovery: &Arc<Discovery>,
    folder: &Path,
) -> Result<RepositoriesDto, DaemonError> {
    let discovery = Arc::clone(discovery);
    let folder = folder.to_path_buf();
    let found = tokio::task::spawn_blocking(move || discovery.checkouts(&folder))
        .await
        .map_err(|error| fail("git_failed", error.to_string()))?;
    Ok(RepositoriesDto {
        inside: found.inside,
        repositories: if found.inside {
            Vec::new()
        } else {
            found
                .items
                .iter()
                .map(|checkout| RepositoryDto {
                    path: checkout.path.clone(),
                    branch: checkout.branch.clone(),
                    repository: Some(checkout.repository.clone()),
                    kind: Some(checkout.kind),
                })
                .collect()
        },
    })
}

/// `Init` is answered before the repository is looked up.
fn unreachable_init() -> Result<Value, DaemonError> {
    to_value(&OutputDto::default())
}

fn output(text: String) -> Result<Value, DaemonError> {
    to_value(&OutputDto {
        output: text.trim().to_owned(),
    })
}

async fn has_head(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--verify", "-q", "HEAD"], LOCAL)
        .await
        .is_ok()
}

/// An unfinished merge, rebase, cherry-pick or revert.
async fn operation(dir: &Path) -> Result<Option<String>, DaemonError> {
    const MARKERS: [(&str, &str); 5] = [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherryPick"),
        ("REVERT_HEAD", "revert"),
    ];
    // One `rev-parse` answers every marker's path, a line each.
    let mut args = vec!["rev-parse"];
    for (marker, _) in MARKERS {
        args.extend(["--git-path", marker]);
    }
    let paths = run(dir, &args, LOCAL).await?;
    Ok(paths
        .lines()
        .zip(MARKERS)
        .find(|(path, _)| dir.join(path.trim()).exists())
        .map(|(_, (_, name))| name.to_owned()))
}

/// Untracked files' line counts, kept while a file's size and modification
/// time stay the same: the status is asked for every few seconds.
static LINE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, Counted>>> = LazyLock::new(Mutex::default);

/// A file's size and modification time when counted, and its lines.
type Counted = (u64, SystemTime, Option<u32>);

/// An untracked file's lines, from the cache when it has not changed.
fn counted(path: &Path) -> Option<u32> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    if let Some((len, at, count)) = lock(&LINE_COUNTS).get(path)
        && *len == meta.len()
        && *at == modified
    {
        return *count;
    }
    let count = count_lines(path);
    let mut cache = lock(&LINE_COUNTS);
    if cache.len() > 20_000 {
        cache.clear();
    }
    cache.insert(path.to_path_buf(), (meta.len(), modified, count));
    count
}

/// Lines in a text file, for untracked files' counts.
fn count_lines(path: &Path) -> Option<u32> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > COUNT_LIMIT {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if looks_binary(&bytes) {
        return None;
    }
    let lines = bytes.iter().filter(|b| **b == b'\n').count()
        + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    u32::try_from(lines).ok()
}

/// The repository's status, from `git status --porcelain=v2`.
async fn status(repo: &Repository) -> Result<StatusDto, DaemonError> {
    let dir = repo.root.as_path();
    // At once: the status (without taking Git's optional index lock, so an
    // agent's Git never waits for it), the lines changed and those staged.
    let (out, unstaged, staged_counts, operation) = tokio::join!(
        run(
            dir,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
            LOCAL,
        ),
        run(dir, &["diff", "--numstat", "-z", "-M"], LOCAL),
        run(dir, &["diff", "--cached", "--numstat", "-z", "-M"], LOCAL),
        operation(dir),
    );
    let out = out?;
    let unstaged = numstat(&unstaged?);
    let staged_counts = numstat(&staged_counts.unwrap_or_default());
    let mut status = StatusDto {
        repository: true,
        prefix: repo.prefix.clone(),
        ..StatusDto::default()
    };
    let mut records = out.split('\0');
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => {
                    status.head = Some(value.chars().take(7).collect());
                }
                "branch.head" if value != "(detached)" => status.branch = Some(value.to_owned()),
                "branch.upstream" => status.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    let mut parts = value.split(' ');
                    status.ahead = parts
                        .next()
                        .and_then(|a| a.trim_start_matches('+').parse().ok())
                        .unwrap_or(0);
                    status.behind = parts
                        .next()
                        .and_then(|b| b.trim_start_matches('-').parse().ok())
                        .unwrap_or(0);
                }
                _ => {}
            }
            continue;
        }
        let kind = record.chars().next().unwrap_or(' ');
        match kind {
            '1' | '2' => {
                // `1 XY sub mH mI mW hH hI path`; `2` adds a score, then the
                // old path as the next record.
                let fields = if kind == '1' { 9 } else { 10 };
                let parts: Vec<&str> = record.splitn(fields, ' ').collect();
                let Some(path) = parts.last().map(|p| (*p).to_owned()) else {
                    continue;
                };
                let old_path = if kind == '2' {
                    records.next().map(str::to_owned)
                } else {
                    None
                };
                let xy: Vec<char> = parts.get(1).copied().unwrap_or("..").chars().collect();
                let (x, y) = (
                    xy.first().copied().unwrap_or('.'),
                    xy.get(1).copied().unwrap_or('.'),
                );
                if x != '.' {
                    let (added, removed) =
                        staged_counts.get(&path).copied().unwrap_or((None, None));
                    status.staged.push(ChangeDto {
                        path: path.clone(),
                        old_path: old_path.clone(),
                        status: status_name(x).to_owned(),
                        added,
                        removed,
                    });
                }
                if y != '.' {
                    let (added, removed) = unstaged.get(&path).copied().unwrap_or((None, None));
                    status.changes.push(ChangeDto {
                        path,
                        old_path: if x == '.' { old_path } else { None },
                        status: status_name(y).to_owned(),
                        added,
                        removed,
                    });
                }
            }
            'u' => {
                let parts: Vec<&str> = record.splitn(11, ' ').collect();
                if let Some(path) = parts.last() {
                    status.conflicts.push(ChangeDto {
                        path: (*path).to_owned(),
                        old_path: None,
                        status: "conflicted".to_owned(),
                        added: None,
                        removed: None,
                    });
                }
            }
            '?' => {
                let path = record.get(2..).unwrap_or_default().to_owned();
                status.untracked.push(ChangeDto {
                    added: None,
                    removed: Some(0),
                    path,
                    old_path: None,
                    status: "untracked".to_owned(),
                });
            }
            _ => {}
        }
    }
    status.operation = operation?;
    // Untracked files' lines, read off the async threads.
    let files: Vec<PathBuf> = status.untracked.iter().map(|c| dir.join(&c.path)).collect();
    let counts = tokio::task::spawn_blocking(move || {
        files.iter().map(|file| counted(file)).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    for (change, count) in status.untracked.iter_mut().zip(counts) {
        change.added = count;
    }
    // The project's own files first, then the rest of the repository.
    let prefix = repo.prefix.clone();
    for list in [
        &mut status.staged,
        &mut status.changes,
        &mut status.untracked,
        &mut status.conflicts,
    ] {
        list.sort_by(|a, b| {
            (!a.path.starts_with(&prefix), &a.path).cmp(&(!b.path.starts_with(&prefix), &b.path))
        });
    }
    Ok(status)
}

/// A file's two sides for a group, or for a commit.
async fn diff(
    dir: &Path,
    path: &str,
    old_path: Option<&str>,
    group: DiffGroup,
    commit: Option<&str>,
) -> Result<DiffDto, DaemonError> {
    let path = checked(path)?;
    let old_path = old_path.map(checked).transpose()?;
    let before = old_path.unwrap_or(path);
    // The file as it is, read off the async threads, and only up to just
    // past the limit (a larger file is not shown anyway).
    let worktree = || async {
        let file = dir.join(path);
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&file)
                .ok()
                .filter(|_| file.is_file())?
                .take(MAX_SIDE as u64 + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            Some(bytes)
        })
        .await
        .ok()
        .flatten()
    };
    let (old, new) = match group {
        DiffGroup::Changes => (blob(dir, &format!(":{before}")).await?, worktree().await),
        DiffGroup::Staged => (
            blob(dir, &format!("HEAD:{before}")).await?,
            blob(dir, &format!(":{path}")).await?,
        ),
        DiffGroup::Untracked => (None, worktree().await),
        DiffGroup::Conflicts => (blob(dir, &format!(":2:{path}")).await?, worktree().await),
        DiffGroup::Commit => {
            let commit = checked_commit(commit.unwrap_or_default())?;
            (
                blob(dir, &format!("{commit}^:{before}")).await?,
                blob(dir, &format!("{commit}:{path}")).await?,
            )
        }
    };
    // Parts can be staged only where both sides are exactly the text shown.
    let exact = |bytes: &Option<Vec<u8>>| {
        bytes
            .as_ref()
            .is_none_or(|bytes| bytes.len() <= MAX_SIDE && std::str::from_utf8(bytes).is_ok())
    };
    let partial = matches!(
        group,
        DiffGroup::Changes | DiffGroup::Staged | DiffGroup::Untracked
    ) && old_path.is_none()
        && exact(&old)
        && exact(&new);
    let index = if partial {
        index_entry(dir, path).await?.map(|(_, id)| id)
    } else {
        None
    };
    let (old, old_binary, old_large) = side(old);
    let (new, new_binary, new_large) = side(new);
    let binary = old_binary || new_binary;
    let too_large = old_large || new_large;
    let (old, new) = if binary || too_large {
        (None, None)
    } else {
        (old, new)
    };
    Ok(DiffDto {
        path: path.to_owned(),
        old_path: old_path.map(str::to_owned),
        old,
        new,
        binary,
        too_large,
        partial: partial && !binary && !too_large,
        index,
    })
}

/// A file's index entry: its mode and blob ID.
async fn index_entry(dir: &Path, path: &str) -> Result<Option<(String, String)>, DaemonError> {
    let out = run(dir, &["ls-files", "--stage", "-z", "--", path], LOCAL).await?;
    // `<mode> <id> <stage>\t<path>`; only the first (stage 0) entry.
    Ok(out.split('\0').next().and_then(|entry| {
        let (head, _) = entry.split_once('\t')?;
        let mut fields = head.split(' ');
        Some((fields.next()?.to_owned(), fields.next()?.to_owned()))
    }))
}

/// Sets a file's staged content, if its index entry is still `expected`:
/// the content goes through Git's clean filters (line endings) as if it
/// were the file, and keeps the entry's mode (or the file's, when new).
async fn stage_part(
    dir: &Path,
    path: &str,
    content: &str,
    expected: Option<&str>,
) -> Result<(), DaemonError> {
    let entry = index_entry(dir, path).await?;
    if entry.as_ref().map(|(_, id)| id.as_str()) != expected {
        return Err(fail(
            "index_changed",
            "The file's staged changes moved on since they were shown; look again.",
        ));
    }
    let mode = if let Some((mode, _)) = entry {
        mode
    } else {
        let executable = std::fs::metadata(dir.join(path)).is_ok_and(|meta| {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        });
        if executable { "100755" } else { "100644" }.to_owned()
    };
    let output = git(
        dir,
        &["hash-object", "-w", "--stdin", &format!("--path={path}")],
        Some(content.as_bytes()),
        LOCAL,
    )
    .await?;
    if !output.status.success() {
        return Err(classify(&String::from_utf8_lossy(&output.stderr)));
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    run(
        dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("{mode},{id},{path}"),
        ],
        LOCAL,
    )
    .await?;
    Ok(())
}

async fn branches(dir: &Path) -> Result<BranchesDto, DaemonError> {
    let format = "%(refname)%00%(refname:short)%00%(upstream:short)%00%(objectname:short)%00%(committerdate:unix)%00%(subject)%00%(HEAD)%1e";
    let out = run(
        dir,
        &[
            "for-each-ref",
            &format!("--format={format}"),
            "refs/heads",
            "refs/remotes",
        ],
        LOCAL,
    )
    .await?;
    let mut list = BranchesDto {
        current: None,
        local: Vec::new(),
        remote: Vec::new(),
    };
    for record in out.split('\u{1e}') {
        let fields: Vec<&str> = record.trim_start_matches('\n').split('\0').collect();
        let [full, name, upstream, commit, time, subject, head] = fields[..] else {
            continue;
        };
        if full.ends_with("/HEAD") {
            continue;
        }
        let branch = BranchDto {
            name: name.to_owned(),
            upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
            commit: commit.to_owned(),
            subject: subject.to_owned(),
            time: time.parse().unwrap_or(0),
        };
        if full.starts_with("refs/heads/") {
            if head == "*" {
                list.current = Some(name.to_owned());
            }
            list.local.push(branch);
        } else {
            list.remote.push(branch);
        }
    }
    list.local
        .sort_by_key(|branch| std::cmp::Reverse(branch.time));
    list.remote
        .sort_by_key(|branch| std::cmp::Reverse(branch.time));
    Ok(list)
}

async fn log(dir: &Path, skip: u32, limit: u32, all: bool) -> Result<LogDto, DaemonError> {
    if !has_head(dir).await {
        return Ok(LogDto {
            commits: Vec::new(),
            more: false,
        });
    }
    let format = format!("--format={COMMIT_FORMAT}%x1e");
    let skip = format!("--skip={skip}");
    let count = (limit + 1).to_string();
    let mut args = vec![
        "log",
        &format,
        &skip,
        "-n",
        &count,
        "--decorate-refs-exclude=refs/stash",
    ];
    if all {
        // Parents after their children, so the graph's lanes stay unbroken.
        args.extend(["--topo-order", "--branches", "--remotes", "--tags", "HEAD"]);
    }
    let out = run(dir, &args, LOCAL).await?;
    let mut commits: Vec<CommitDto> = out
        .split('\u{1e}')
        .filter_map(|record| {
            let fields: Vec<&str> = record.trim_start_matches('\n').split('\0').collect();
            commit_from(&fields)
        })
        .collect();
    let more = commits.len() > limit as usize;
    commits.truncate(limit as usize);
    Ok(LogDto { commits, more })
}

async fn show(dir: &Path, commit: &str) -> Result<CommitDetailDto, DaemonError> {
    let out = run(
        dir,
        &[
            "show",
            "-s",
            &format!("--format={COMMIT_FORMAT}%x00%b"),
            commit,
        ],
        LOCAL,
    )
    .await?;
    let fields: Vec<&str> = out.split('\0').collect();
    let info = commit_from(&fields).ok_or_else(|| fail("invalid_commit", "No such commit."))?;
    let body = fields.get(8).copied().unwrap_or_default().trim().to_owned();
    // Against the first parent; a first commit against nothing.
    let range: Vec<String> = if info.parents.is_empty() {
        vec!["--root".into(), commit.into()]
    } else {
        vec![format!("{commit}^1"), commit.into()]
    };
    let range: Vec<&str> = range.iter().map(String::as_str).collect();
    let base: &[&str] = if info.parents.is_empty() {
        &["diff-tree", "--no-commit-id", "-r", "-z", "-M"]
    } else {
        &["diff", "-z", "-M"]
    };
    let counts = numstat(&run(dir, &[base, &["--numstat"], &range].concat(), LOCAL).await?);
    let names = run(dir, &[base, &["--name-status"], &range].concat(), LOCAL).await?;
    Ok(CommitDetailDto {
        commit: info,
        body,
        files: name_status(&names, &counts),
    })
}

async fn stashes(dir: &Path) -> Result<StashesDto, DaemonError> {
    let out = run(
        dir,
        &["stash", "list", "--format=%gd%x00%s%x00%ct%x1e"],
        LOCAL,
    )
    .await?;
    let items = out
        .split('\u{1e}')
        .filter_map(|record| {
            let fields: Vec<&str> = record.trim_start_matches('\n').split('\0').collect();
            let [name, message, time] = fields[..] else {
                return None;
            };
            let index = name
                .trim_start_matches("stash@{")
                .trim_end_matches('}')
                .parse()
                .ok()?;
            Some(StashDto {
                index,
                message: message.to_owned(),
                time: time.parse().unwrap_or(0),
            })
        })
        .collect();
    Ok(StashesDto { items })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sh(dir: &Path, args: &[&str]) {
        let output = git(dir, args, None, LOCAL).await.unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        sh(&root, &["init", "-q", "-b", "main"]).await;
        sh(&root, &["config", "user.name", "Ahab Test"]).await;
        sh(&root, &["config", "user.email", "test@example.com"]).await;
        sh(&root, &["config", "commit.gpgsign", "false"]).await;
        std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        sh(&root, &["add", "a.txt"]).await;
        sh(&root, &["commit", "-q", "-m", "First"]).await;
        (dir, root)
    }

    async fn ask(git: &Git, root: &Path, op: GitOp) -> Value {
        git.run(root, None, false, op).await.unwrap()
    }

    #[tokio::test]
    async fn status_groups_changes_with_their_lines() {
        let (_dir, root) = repo().await;
        std::fs::write(root.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
        std::fs::write(root.join("new.txt"), "x\ny\n").unwrap();
        let git = Git::default();
        let status: StatusDto =
            serde_json::from_value(ask(&git, &root, GitOp::Status).await).unwrap();
        assert!(status.repository);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.changes[0].path, "a.txt");
        assert_eq!(
            (status.changes[0].added, status.changes[0].removed),
            (Some(2), Some(1))
        );
        assert_eq!(status.untracked[0].path, "new.txt");
        assert_eq!(status.untracked[0].added, Some(2));

        ask(
            &git,
            &root,
            GitOp::Stage {
                paths: vec!["a.txt".into()],
                all: false,
            },
        )
        .await;
        let status: StatusDto =
            serde_json::from_value(ask(&git, &root, GitOp::Status).await).unwrap();
        assert_eq!(status.staged[0].path, "a.txt");
        assert!(status.changes.is_empty());

        let diff: DiffDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Diff {
                    path: "a.txt".into(),
                    old_path: None,
                    group: DiffGroup::Staged,
                    commit: None,
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(diff.old.as_deref(), Some("one\ntwo\nthree\n"));
        assert_eq!(diff.new.as_deref(), Some("one\n2\nthree\nfour\n"));
    }

    async fn changes(git: &Git, root: &Path) -> DiffDto {
        let value = ask(
            git,
            root,
            GitOp::Diff {
                path: "a.txt".into(),
                old_path: None,
                group: DiffGroup::Changes,
                commit: None,
            },
        )
        .await;
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test]
    async fn stages_part_of_a_file_only_from_what_was_shown() {
        let (_dir, root) = repo().await;
        std::fs::write(root.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
        let git = Git::default();
        let shown = changes(&git, &root).await;
        assert!(shown.partial);
        let index = shown.index.clone();
        assert!(index.is_some());

        // Stage only the added last line.
        ask(
            &git,
            &root,
            GitOp::StagePart {
                path: "a.txt".into(),
                content: "one\ntwo\nthree\nfour\n".into(),
                index: index.clone(),
            },
        )
        .await;
        let status: StatusDto =
            serde_json::from_value(ask(&git, &root, GitOp::Status).await).unwrap();
        assert_eq!(status.staged[0].path, "a.txt");
        assert_eq!(status.changes[0].path, "a.txt", "the rest stays unstaged");
        assert_eq!(
            changes(&git, &root).await.old.as_deref(),
            Some("one\ntwo\nthree\nfour\n")
        );

        // The index moved on: the old view's request is refused.
        let stale = git
            .run(
                &root,
                None,
                false,
                GitOp::StagePart {
                    path: "a.txt".into(),
                    content: "x\n".into(),
                    index,
                },
            )
            .await;
        assert_eq!(stale.unwrap_err().code, "index_changed");

        // A new file's lines, staged in part.
        std::fs::write(root.join("new.txt"), "a\nb\n").unwrap();
        ask(
            &git,
            &root,
            GitOp::StagePart {
                path: "new.txt".into(),
                content: "a\n".into(),
                index: None,
            },
        )
        .await;
        let status: StatusDto =
            serde_json::from_value(ask(&git, &root, GitOp::Status).await).unwrap();
        assert!(status.staged.iter().any(|change| change.path == "new.txt"));
    }

    #[tokio::test]
    async fn commits_branches_history_and_stashes() {
        let (_dir, root) = repo().await;
        let git = Git::default();
        std::fs::write(root.join("b.txt"), "b\n").unwrap();
        ask(
            &git,
            &root,
            GitOp::Stage {
                paths: Vec::new(),
                all: true,
            },
        )
        .await;
        ask(
            &git,
            &root,
            GitOp::Commit {
                message: "Second\n\nWith a body.".into(),
                amend: false,
            },
        )
        .await;
        let log: LogDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Log {
                    skip: 0,
                    limit: 10,
                    all: false,
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(
            log.commits
                .iter()
                .map(|c| c.subject.as_str())
                .collect::<Vec<_>>(),
            ["Second", "First"]
        );
        assert_eq!(log.commits[0].refs, ["HEAD", "main"]);
        let detail: CommitDetailDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Show {
                    commit: log.commits[0].hash.clone(),
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(detail.body, "With a body.");
        assert_eq!(detail.files[0].path, "b.txt");
        assert_eq!(detail.files[0].status, "added");
        let first: CommitDetailDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Show {
                    commit: log.commits[1].hash.clone(),
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(
            first.files[0].path, "a.txt",
            "a first commit lists its files"
        );

        let nothing = git
            .run(
                &root,
                None,
                false,
                GitOp::Commit {
                    message: "Empty".into(),
                    amend: false,
                },
            )
            .await;
        assert_eq!(nothing.unwrap_err().code, "nothing_to_commit");

        ask(
            &git,
            &root,
            GitOp::CreateBranch {
                name: "feature".into(),
                from: None,
            },
        )
        .await;
        let branches: BranchesDto =
            serde_json::from_value(ask(&git, &root, GitOp::Branches).await).unwrap();
        assert_eq!(branches.current.as_deref(), Some("feature"));
        assert_eq!(branches.local.len(), 2);
        ask(
            &git,
            &root,
            GitOp::Switch {
                branch: "main".into(),
                remote: false,
                stash: false,
            },
        )
        .await;
        let bad = git
            .run(
                &root,
                None,
                false,
                GitOp::CreateBranch {
                    name: "no..dots".into(),
                    from: None,
                },
            )
            .await;
        assert_eq!(bad.unwrap_err().code, "invalid_branch");

        // A commit on another branch shows in the graph's log, not in HEAD's.
        sh(&root, &["commit", "--allow-empty", "-q", "-m", "Elsewhere"]).await;
        sh(&root, &["branch", "-f", "feature", "HEAD"]).await;
        sh(&root, &["reset", "-q", "--hard", "HEAD~1"]).await;
        let head: LogDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Log {
                    skip: 0,
                    limit: 10,
                    all: false,
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(head.commits.len(), 2);
        let graph: LogDto = serde_json::from_value(
            ask(
                &git,
                &root,
                GitOp::Log {
                    skip: 0,
                    limit: 10,
                    all: true,
                },
            )
            .await,
        )
        .unwrap();
        assert_eq!(graph.commits[0].subject, "Elsewhere");
        assert_eq!(graph.commits[0].refs, ["feature"]);
        assert_eq!(graph.commits[1].refs, ["HEAD", "main"]);

        std::fs::write(root.join("a.txt"), "changed\n").unwrap();
        ask(
            &git,
            &root,
            GitOp::Stash {
                message: Some("Half done".into()),
                untracked: false,
            },
        )
        .await;
        let stashes: StashesDto =
            serde_json::from_value(ask(&git, &root, GitOp::Stashes).await).unwrap();
        assert_eq!(stashes.items[0].index, 0);
        assert!(stashes.items[0].message.contains("Half done"));
        ask(
            &git,
            &root,
            GitOp::StashApply {
                index: 0,
                pop: true,
            },
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "changed\n"
        );
    }

    #[tokio::test]
    async fn discards_only_inside_the_project_and_refuses_odd_paths() {
        let (_dir, root) = repo().await;
        let git = Git::default();
        std::fs::write(root.join("a.txt"), "changed\n").unwrap();
        std::fs::write(root.join("junk.txt"), "x\n").unwrap();
        ask(
            &git,
            &root,
            GitOp::Discard {
                paths: vec!["a.txt".into(), "junk.txt".into()],
            },
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
        assert!(!root.join("junk.txt").exists());

        for path in ["../x", "/etc/passwd", "--force", ""] {
            let refused = git
                .run(
                    &root,
                    None,
                    false,
                    GitOp::Stage {
                        paths: vec![path.into()],
                        all: false,
                    },
                )
                .await;
            assert_eq!(refused.unwrap_err().code, "invalid_path", "{path}");
        }
        let read_only = git
            .run(
                &root,
                None,
                true,
                GitOp::Stage {
                    paths: Vec::new(),
                    all: true,
                },
            )
            .await;
        assert_eq!(read_only.unwrap_err().code, "read_only");

        let elsewhere = tempfile::tempdir().unwrap();
        let none: StatusDto = serde_json::from_value(
            git.run(elsewhere.path(), None, false, GitOp::Status)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!none.repository);
    }

    #[tokio::test]
    async fn lists_the_worktrees_of_a_bare_repository_not_the_repository() {
        // repositories/clients/app/{.bare, .git = "gitdir: ./.bare", main, fix}
        let outer = tempfile::tempdir().unwrap();
        let (_dir, origin) = repo().await;
        let app = outer.path().join("repositories/clients/app");
        std::fs::create_dir_all(&app).unwrap();
        sh(
            &app,
            &["clone", "-q", "--bare", origin.to_str().unwrap(), ".bare"],
        )
        .await;
        std::fs::write(app.join(".git"), "gitdir: ./.bare\n").unwrap();
        sh(&app, &["worktree", "add", "-q", "main", "main"]).await;
        sh(&app, &["worktree", "add", "-q", "-b", "fix", "fix", "main"]).await;

        let git = Git::default();
        let found: RepositoriesDto = serde_json::from_value(
            git.run(outer.path(), None, false, GitOp::Repositories)
                .await
                .unwrap(),
        )
        .unwrap();
        let paths: Vec<_> = found.repositories.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "repositories/clients/app/fix",
                "repositories/clients/app/main"
            ]
        );
        assert_eq!(found.repositories[0].branch.as_deref(), Some("fix"));
        for checkout in &found.repositories {
            assert_eq!(
                checkout.repository.as_deref(),
                Some("repositories/clients/app"),
                "both checkouts are of one repository"
            );
        }

        // A repository's own checkout and its worktree beside it: one group.
        let svc = outer.path().join("services/svc");
        std::fs::create_dir_all(svc.parent().unwrap()).unwrap();
        sh(
            outer.path(),
            &["clone", "-q", origin.to_str().unwrap(), "services/svc"],
        )
        .await;
        sh(
            &svc,
            &["worktree", "add", "-q", "-b", "next", "../svc-next"],
        )
        .await;
        let found: RepositoriesDto = serde_json::from_value(
            git.run(outer.path(), None, false, GitOp::Repositories)
                .await
                .unwrap(),
        )
        .unwrap();
        let services: Vec<_> = found
            .repositories
            .iter()
            .filter(|r| r.path.starts_with("services/"))
            .map(|r| {
                (
                    r.path.as_str(),
                    r.branch.as_deref(),
                    r.repository.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            services,
            [
                ("services/svc", Some("main"), Some("services/svc")),
                ("services/svc-next", Some("next"), Some("services/svc")),
            ]
        );
        let status: StatusDto = serde_json::from_value(
            git.run(
                outer.path(),
                Some("repositories/clients/app/main"),
                false,
                GitOp::Status,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert!(status.repository);
        assert_eq!(status.branch.as_deref(), Some("main"));
    }

    #[tokio::test]
    async fn finds_repositories_inside_a_folder_that_is_not_one() {
        let outer = tempfile::tempdir().unwrap();
        let (_dir, inner) = repo().await;
        let target = outer.path().join("app");
        std::fs::rename(&inner, &target).unwrap();
        std::fs::create_dir_all(outer.path().join("node_modules/pkg/.git")).unwrap();
        std::fs::create_dir_all(outer.path().join("docs")).unwrap();
        let git = Git::default();
        let found: RepositoriesDto = serde_json::from_value(
            git.run(outer.path(), None, false, GitOp::Repositories)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!found.inside);
        assert_eq!(found.repositories.len(), 1, "not node_modules");
        assert_eq!(found.repositories[0].path, "app");
        assert_eq!(found.repositories[0].branch.as_deref(), Some("main"));
        assert_eq!(found.repositories[0].repository.as_deref(), Some("app"));

        let status: StatusDto = serde_json::from_value(
            git.run(outer.path(), Some("app"), false, GitOp::Status)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(status.repository);
        assert_eq!(status.branch.as_deref(), Some("main"));
        let escape = git
            .run(outer.path(), Some("../x"), false, GitOp::Status)
            .await;
        assert_eq!(escape.unwrap_err().code, "invalid_path");

        let inside: RepositoriesDto = serde_json::from_value(
            git.run(&target, None, false, GitOp::Repositories)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(inside.inside);
    }
}

pub mod discover;
pub mod watch;
