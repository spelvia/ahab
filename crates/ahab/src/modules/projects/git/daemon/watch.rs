//! Watching a checkout on its machine: its files and
//! Git's own (the index, `HEAD`, refs) are watched, and every change, after
//! a quiet moment, is one `changed` notice (which the daemon sends on a
//! channel). The web app then
//! asks for the status once, instead of every few seconds.
//!
//! Only folders the checkout's ignore rules keep are watched (not
//! `node_modules` or `target`), at most [`MAX_FOLDERS`]; a checkout that
//! needs more, or a system that refuses more watches, says `unwatched`, and
//! the web app keeps asking on its own.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ignore::WalkBuilder;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::modules::projects::git::daemon::Places;

/// At most this many folders of a checkout are watched.
pub const MAX_FOLDERS: usize = 10_000;
/// Changes are told once nothing more changed for this long...
const QUIET: Duration = Duration::from_millis(250);
/// ...and at most this often, however busy the folder.
const AT_MOST: Duration = Duration::from_secs(1);

/// What a watch says on its channel, one JSON object per frame.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WatchFrame {
    /// Watching started.
    Watching,
    /// Something in the checkout or its Git changed.
    Changed,
    /// It cannot be watched (too many folders, or the system's limit).
    Unwatched { reason: String },
}

/// What happened to a path, as the watcher thread tells it.
enum Seen {
    Changed,
    /// A new folder in the checkout, to watch too.
    Folder(PathBuf),
}

/// The folders under `root` its ignore rules keep, `.git` aside; `None`
/// when there are more than `limit`.
fn folders(root: &Path, limit: usize) -> Option<Vec<PathBuf>> {
    let mut found = Vec::new();
    let walk = WalkBuilder::new(root)
        .hidden(false)
        .parents(true)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    for entry in walk.flatten() {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            if found.len() >= limit {
                return None;
            }
            found.push(entry.into_path());
        }
    }
    Some(found)
}

/// Whether a change to `path` is worth telling: Git's lock files come and
/// go around every operation, and say nothing by themselves.
fn worth_telling(path: &Path) -> bool {
    path.extension().is_none_or(|extension| extension != "lock")
}

/// Starts watching `places`, the watcher and the folders it watches;
/// `Err` with the reason when it cannot.
fn start(places: &Places, seen: mpsc::Sender<Seen>) -> Result<RecommendedWatcher, String> {
    let root = places.root.clone();
    let git_dir = places.git_dir.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in &event.paths {
            if !worth_telling(path) {
                continue;
            }
            let created_folder = matches!(event.kind, EventKind::Create(_))
                && path.starts_with(&root)
                && !path.starts_with(&git_dir)
                && path.is_dir();
            let what = if created_folder {
                Seen::Folder(path.clone())
            } else {
                Seen::Changed
            };
            // A full queue already has a change to tell.
            let _ = seen.try_send(what);
        }
    })
    .map_err(|error| error.to_string())?;
    let folders = folders(&places.root, MAX_FOLDERS)
        .ok_or_else(|| format!("more than {MAX_FOLDERS} folders"))?;
    for folder in &folders {
        watcher
            .watch(folder, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
    }
    // The index, HEAD and an unfinished merge or rebase; and the refs,
    // which a worktree shares with its repository.
    watcher
        .watch(&places.git_dir, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;
    if places.common != places.git_dir {
        watcher
            .watch(&places.common, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
    }
    let refs = places.common.join("refs");
    if refs.is_dir() {
        watcher
            .watch(&refs, RecursiveMode::Recursive)
            .map_err(|error| error.to_string())?;
    }
    Ok(watcher)
}

async fn send(tell: &mpsc::Sender<WatchFrame>, frame: WatchFrame) -> bool {
    tell.send(frame).await.is_ok()
}

/// Watches `places` until nobody listens on `tell`, telling its changes.
pub async fn watch(tell: mpsc::Sender<WatchFrame>, places: Places) {
    let (seen, mut changes) = mpsc::channel(256);
    let started = {
        let places = Places {
            root: places.root.clone(),
            git_dir: places.git_dir.clone(),
            common: places.common.clone(),
        };
        let seen = seen.clone();
        tokio::task::spawn_blocking(move || start(&places, seen)).await
    };
    let mut watcher = match started {
        Ok(Ok(watcher)) => watcher,
        Ok(Err(reason)) => {
            send(&tell, WatchFrame::Unwatched { reason }).await;
            return;
        }
        Err(error) => {
            send(
                &tell,
                WatchFrame::Unwatched {
                    reason: error.to_string(),
                },
            )
            .await;
            return;
        }
    };
    if !send(&tell, WatchFrame::Watching).await {
        return;
    }
    let mut pending = false;
    let mut last_told = tokio::time::Instant::now() - AT_MOST;
    let mut quiet_until = tokio::time::Instant::now();
    loop {
        let due = quiet_until.max(last_told + AT_MOST);
        tokio::select! {
            () = tell.closed() => break,
            change = changes.recv() => {
                let Some(change) = change else { break };
                if let Seen::Folder(folder) = change {
                    // Its own folders too, as the ignore rules allow.
                    let found = folders(&folder, MAX_FOLDERS).unwrap_or_default();
                    for inner in found {
                        let _ = watcher.watch(&inner, RecursiveMode::NonRecursive);
                    }
                }
                pending = true;
                quiet_until = tokio::time::Instant::now() + QUIET;
            }
            () = tokio::time::sleep_until(due), if pending => {
                pending = false;
                last_told = tokio::time::Instant::now();
                if !send(&tell, WatchFrame::Changed).await {
                    break;
                }
            }
        }
    }
    drop(seen);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watches_the_folders_the_ignore_rules_keep() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join(".github")).unwrap();
        std::fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        let mut found: Vec<_> = folders(root, 100)
            .unwrap()
            .into_iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        assert_eq!(found, ["", ".github", "src", "src/deep"]);
        assert!(folders(root, 2).is_none(), "past the limit");
        assert!(!worth_telling(Path::new(".git/index.lock")));
        assert!(worth_telling(Path::new(".git/index")));
    }
}
