//! Watching open files on their machine: the
//! folders the files are in are watched, and after a quiet moment each
//! watched file whose content changed is told with its new version (none
//! once it is gone). Saving a file in Ahab changes its version too; the
//! window that saved it knows that version already and ignores the notice.
//!
//! Editors and tools that save by writing a new file and renaming it over
//! the old one are seen too, as the folder is what is watched.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::modules::projects::files::api::FileWatchFrame;
use crate::modules::projects::files::daemon::file_version;

/// At most this many files per watch.
pub const MAX_FILES: usize = 200;
/// Changes are told once nothing more changed for this long.
const QUIET: Duration = Duration::from_millis(150);

/// The files a watch follows: where each is on disk, by its relative path.
struct Watched {
    /// Absolute path (in its real folder) → the relative path asked for.
    by_place: HashMap<PathBuf, String>,
    folders: BTreeSet<PathBuf>,
}

/// Where each of `paths` lives under `root` (real): paths that leave the
/// project or whose folder is missing are left out.
fn places(root: &Path, paths: &[String]) -> Watched {
    let mut by_place = HashMap::new();
    let mut folders = BTreeSet::new();
    for relative in paths.iter().take(MAX_FILES) {
        let clean: Vec<&str> = relative
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect();
        if clean.is_empty()
            || clean
                .iter()
                .any(|part| *part == ".." || part.contains('\\'))
        {
            continue;
        }
        let (name, parents) = clean.split_last().unwrap_or((&"", &[]));
        let Ok(folder) = std::fs::canonicalize(root.join(parents.join("/"))) else {
            continue;
        };
        if !folder.starts_with(root) {
            continue;
        }
        let place = folder.join(name);
        // A link inside the project: its target's folder is watched too.
        if let Ok(real) = std::fs::canonicalize(&place)
            && real != place
            && real.starts_with(root)
            && let Some(real_folder) = real.parent()
        {
            folders.insert(real_folder.to_path_buf());
            by_place.insert(real, clean.join("/"));
        }
        folders.insert(folder);
        by_place.insert(place, clean.join("/"));
    }
    Watched { by_place, folders }
}

fn start(watched: &Watched, seen: mpsc::Sender<PathBuf>) -> Result<RecommendedWatcher, String> {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in event.paths {
            // A full queue already has changes to look at.
            let _ = seen.try_send(path);
        }
    })
    .map_err(|error| error.to_string())?;
    for folder in &watched.folders {
        watcher
            .watch(folder, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
    }
    Ok(watcher)
}

/// The versions of `paths` under `root`, read off the disk.
async fn versions(root: PathBuf, paths: BTreeSet<String>) -> BTreeMap<String, Option<String>> {
    tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|relative| {
                let version = file_version(&root.join(&relative)).ok().flatten();
                (relative, version)
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Watches `paths` (relative) under `root` (real) until nobody listens on
/// `tell`, telling each change of a file's version.
pub async fn watch(tell: mpsc::Sender<FileWatchFrame>, root: PathBuf, paths: Vec<String>) {
    let watched = places(&root, &paths);
    let (seen, mut changes) = mpsc::channel(512);
    let started = {
        let folders = Watched {
            by_place: HashMap::new(),
            folders: watched.folders.clone(),
        };
        tokio::task::spawn_blocking(move || start(&folders, seen)).await
    };
    let _watcher = match started {
        Ok(Ok(watcher)) => watcher,
        Ok(Err(reason)) => {
            let _ = tell.send(FileWatchFrame::Unwatched { reason }).await;
            return;
        }
        Err(error) => {
            let _ = tell
                .send(FileWatchFrame::Unwatched {
                    reason: error.to_string(),
                })
                .await;
            return;
        }
    };
    let all: BTreeSet<String> = watched.by_place.values().cloned().collect();
    let mut known = versions(root.clone(), all).await;
    if tell
        .send(FileWatchFrame::Watching {
            versions: known.clone(),
        })
        .await
        .is_err()
    {
        return;
    }
    let mut pending: BTreeSet<String> = BTreeSet::new();
    let mut quiet_until = tokio::time::Instant::now();
    loop {
        tokio::select! {
            () = tell.closed() => break,
            change = changes.recv() => {
                let Some(path) = change else { break };
                if let Some(relative) = watched.by_place.get(&path) {
                    pending.insert(relative.clone());
                    quiet_until = tokio::time::Instant::now() + QUIET;
                }
            }
            () = tokio::time::sleep_until(quiet_until), if !pending.is_empty() => {
                let now = versions(root.clone(), std::mem::take(&mut pending)).await;
                for (path, version) in now {
                    if known.get(&path) == Some(&version) {
                        continue;
                    }
                    known.insert(path.clone(), version.clone());
                    if tell.send(FileWatchFrame::Changed { path, version }).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watches_the_folders_of_files_inside_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap().join("ws");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.rs"), "a").unwrap();
        let watched = places(
            &root,
            &[
                "src/a.rs".into(),
                "src/new.rs".into(),
                "../outside.txt".into(),
                "missing/x.rs".into(),
                String::new(),
            ],
        );
        let mut paths: Vec<_> = watched.by_place.values().cloned().collect();
        paths.sort();
        assert_eq!(paths, ["src/a.rs", "src/new.rs"]);
        assert_eq!(
            watched.folders.into_iter().collect::<Vec<_>>(),
            [root.join("src")]
        );
    }

    #[tokio::test]
    async fn tells_changes_deletions_and_nothing_for_the_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("a.txt"), "one").unwrap();
        let (tell, mut told) = mpsc::channel(16);
        let watching = tokio::spawn(watch(tell, root.clone(), vec!["a.txt".into()]));
        let next = async |told: &mut mpsc::Receiver<FileWatchFrame>| {
            tokio::time::timeout(Duration::from_secs(5), told.recv())
                .await
                .unwrap()
                .unwrap()
        };
        let FileWatchFrame::Watching { versions } = next(&mut told).await else {
            panic!("not watching");
        };
        let first = versions["a.txt"].clone().unwrap();
        // The same content again says nothing; new content does.
        std::fs::write(root.join("a.txt"), "one").unwrap();
        std::fs::write(root.join("a.txt"), "two").unwrap();
        let FileWatchFrame::Changed { path, version } = next(&mut told).await else {
            panic!("not changed");
        };
        assert_eq!(path, "a.txt");
        assert_ne!(version.as_deref(), Some(first.as_str()));
        std::fs::remove_file(root.join("a.txt")).unwrap();
        assert_eq!(
            next(&mut told).await,
            FileWatchFrame::Changed {
                path: "a.txt".into(),
                version: None
            }
        );
        drop(told);
        tokio::time::timeout(Duration::from_secs(5), watching)
            .await
            .unwrap()
            .unwrap();
    }
}
