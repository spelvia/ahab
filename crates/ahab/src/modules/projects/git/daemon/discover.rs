//! Finding the repositories, worktrees and submodules inside a project's
//! folder, from files alone, as
//! many folders are looked at. What was found is kept per folder and found
//! again only when a `.git` entry, a Git folder or a folder looked into
//! changed: their modification times are compared on every use, a few
//! hundred `stat`s instead of a walk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::modules::projects::git::model::{
    Checkout, CheckoutKind, Checkouts, gitmodules_paths, is_submodule_gitdir,
};
use crate::shared::sync::lock;

/// How deep inside a project's folder repositories are looked for.
pub const SEARCH_DEPTH: usize = 5;
/// At most this many checkouts are listed (each worktree counts). Listing
/// reads a few small files per checkout; Git runs only for opened ones.
pub const MAX_REPOSITORIES: usize = 300;
/// Folders never looked into for repositories.
const SKIPPED: &[&str] = &["node_modules", "target", "vendor", "dist", "build", ".venv"];

/// What was found in one folder, and what tells it changed.
struct Kept {
    checkouts: Arc<Checkouts>,
    /// Folders and files whose modification time changes when a
    /// repository or worktree comes or goes.
    stamps: Vec<(PathBuf, Option<SystemTime>)>,
}

/// The repositories found per project folder, kept.
#[derive(Default)]
pub struct Discovery {
    found: Mutex<HashMap<PathBuf, Kept>>,
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

impl Discovery {
    /// The checkouts in `folder`: kept, or found again when something
    /// changed. Blocking: it reads the disk.
    pub fn checkouts(&self, folder: &Path) -> Arc<Checkouts> {
        {
            let found = lock(&self.found);
            if let Some(kept) = found.get(folder)
                && kept.stamps.iter().all(|(path, time)| stamp(path) == *time)
            {
                return Arc::clone(&kept.checkouts);
            }
        }
        let (checkouts, stamps) = discover(folder);
        let checkouts = Arc::new(checkouts);
        lock(&self.found).insert(
            folder.to_path_buf(),
            Kept {
                checkouts: Arc::clone(&checkouts),
                stamps,
            },
        );
        checkouts
    }

    /// Forgets what was found in `folder`.
    pub fn forget(&self, folder: &Path) {
        lock(&self.found).remove(folder);
    }
}

/// A folder with files checked out from a repository.
pub struct GitCheckout {
    /// The repository's own Git folder (`.git`, `.bare`, `app.git`), shared
    /// by all its worktrees.
    pub common: PathBuf,
    /// This checkout's Git folder.
    pub git_dir: PathBuf,
    pub branch: Option<String>,
    /// A linked worktree (its Git folder is not the repository's).
    pub linked: bool,
}

impl GitCheckout {
    /// Where the repository is: the folder holding its `.git` or `.bare`,
    /// else (`app.git`) its Git folder.
    pub fn home(&self) -> PathBuf {
        let hidden = self
            .common
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'));
        match self.common.parent() {
            Some(parent) if hidden => parent.to_path_buf(),
            _ => self.common.clone(),
        }
    }
}

/// The `gitdir` a `.git` file names.
fn gitdir_of(dot_git: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dot_git).ok()?;
    Some(text.trim().strip_prefix("gitdir:")?.trim().to_owned())
}

/// The checkout in a folder with a `.git`, or `None` when that `.git` is
/// (or points to) a bare repository.
pub fn checkout(dir: &Path) -> Option<GitCheckout> {
    let dot_git = dir.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        dir.join(gitdir_of(&dot_git)?)
    };
    let git_dir = std::fs::canonicalize(&git_dir).ok()?;
    // Git takes a folder for its own only with a `HEAD` (an empty `.git`
    // left somewhere is none).
    if !git_dir.join("HEAD").is_file() {
        return None;
    }
    // A worktree's Git folder names the repository's (`commondir`).
    let common = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(relative) => std::fs::canonicalize(git_dir.join(relative.trim())).ok()?,
        Err(_) => git_dir.clone(),
    };
    let linked = common != git_dir;
    let config = std::fs::read_to_string(common.join("config")).unwrap_or_default();
    let bare = config.lines().any(|line| {
        let line: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        line.eq_ignore_ascii_case("bare=true")
    });
    // A bare repository's linked worktrees have files; it has none.
    if bare && !linked {
        return None;
    }
    let head = std::fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
    let branch = head
        .trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_owned);
    Some(GitCheckout {
        common,
        git_dir,
        branch,
        linked,
    })
}

/// Whether `dir` (with a `.git`) is a submodule of the checkout at `parent`:
/// its `.git` file points into `.git/modules/`, or the parent's
/// `.gitmodules` lists it.
pub fn is_submodule(dir: &Path, parent: &Path) -> bool {
    if gitdir_of(&dir.join(".git")).is_some_and(|gitdir| is_submodule_gitdir(&gitdir)) {
        return true;
    }
    let listed = std::fs::read_to_string(parent.join(".gitmodules"))
        .map(|text| gitmodules_paths(&text))
        .unwrap_or_default();
    dir.strip_prefix(parent)
        .is_ok_and(|relative| listed.iter().any(|path| Path::new(path) == relative))
}

/// A path with `/` between its parts, on every system.
pub fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The checkout `folder` itself is in, when one of its folders (or itself)
/// has a `.git` that is not a bare repository's.
fn enclosing(folder: &Path) -> Option<(PathBuf, GitCheckout)> {
    folder
        .ancestors()
        .filter(|dir| dir.join(".git").exists())
        .find_map(|dir| checkout(dir).map(|found| (dir.to_path_buf(), found)))
}

/// Looks for repositories and worktrees in `folder` (not inside them: a
/// repository's folders are its own, submodules included), up to
/// [`SEARCH_DEPTH`] deep; the bare layout's worktrees (`app/.git` reading
/// `gitdir: ./.bare`, worktrees beside it) count as deep as the repository.
pub fn discover(folder: &Path) -> (Checkouts, Vec<(PathBuf, Option<SystemTime>)>) {
    let mut stamps = vec![(folder.to_path_buf(), stamp(folder))];
    if let Some((dir, found)) = enclosing(folder) {
        stamps.push((dir.join(".git"), stamp(&dir.join(".git"))));
        stamps.push((found.git_dir.clone(), stamp(&found.git_dir)));
        let submodules = submodules_of(&dir)
            .into_iter()
            .filter_map(|path| {
                dir.join(&path)
                    .strip_prefix(folder)
                    .ok()
                    .map(slashes)
                    .filter(|path| !path.is_empty())
            })
            .collect();
        return (
            Checkouts {
                inside: true,
                items: vec![Checkout {
                    path: String::new(),
                    repository: String::new(),
                    kind: CheckoutKind::Project,
                    branch: found.branch,
                    submodules,
                }],
            },
            stamps,
        );
    }
    let mut found: Vec<(String, GitCheckout, PathBuf)> = Vec::new();
    let mut walk = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = walk.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        if dir != folder {
            stamps.push((dir.clone(), stamp(&dir)));
        }
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect();
        children.sort();
        for child in children.into_iter().rev() {
            let name = child
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
                continue;
            }
            let dot_git = child.join(".git");
            if dot_git.exists() {
                stamps.push((dot_git.clone(), stamp(&dot_git)));
                if let Some(checkout) = checkout(&child) {
                    stamps.push((checkout.git_dir.clone(), stamp(&checkout.git_dir)));
                    stamps.push((child.join(".gitmodules"), stamp(&child.join(".gitmodules"))));
                    if let Ok(relative) = child.strip_prefix(folder) {
                        found.push((slashes(relative), checkout, child.clone()));
                    }
                } else {
                    // A bare repository with its worktrees in folders
                    // beside it: the worktrees are listed, as deep as the
                    // repository.
                    walk.push((child, depth));
                }
                // A repository's own folders are its business.
                continue;
            }
            if depth + 1 < SEARCH_DEPTH {
                walk.push((child, depth + 1));
            }
        }
        if found.len() >= MAX_REPOSITORIES {
            break;
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.truncate(MAX_REPOSITORIES);
    let base = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let items = found
        .into_iter()
        .map(|(path, checkout, dir)| {
            let home = checkout.home();
            let repository = home
                .strip_prefix(&base)
                .map_or_else(|_| slashes(&home), slashes);
            let kind = if checkout.linked || repository != path {
                CheckoutKind::Worktree
            } else {
                CheckoutKind::Repository
            };
            Checkout {
                path,
                repository,
                kind,
                branch: checkout.branch,
                submodules: submodules_of(&dir),
            }
        })
        .collect();
    (
        Checkouts {
            inside: false,
            items,
        },
        stamps,
    )
}

/// The submodules a checkout's `.gitmodules` lists that are there.
fn submodules_of(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join(".gitmodules"))
        .map(|text| gitmodules_paths(&text))
        .unwrap_or_default()
        .into_iter()
        .filter(|path| dir.join(path).join(".git").exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(["-c", "protocol.file.allow=always"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q"]);
        std::fs::write(dir.join("README"), "x").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "first"]);
    }

    #[test]
    fn finds_repositories_worktrees_of_the_bare_layout_and_submodules() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        // A plain repository with a submodule.
        repo(&root.join("lib-origin"));
        repo(&root.join("clients/svc"));
        git(
            &root.join("clients/svc"),
            &[
                "submodule",
                "add",
                "-q",
                &root.join("lib-origin").display().to_string(),
                "vendor/lib",
            ],
        );
        // The bare layout: app/.bare, app/.git = "gitdir: ./.bare", worktrees beside.
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        git(
            &app,
            &[
                "clone",
                "-q",
                "--bare",
                &root.join("lib-origin").display().to_string(),
                ".bare",
            ],
        );
        std::fs::write(app.join(".git"), "gitdir: ./.bare\n").unwrap();
        git(&app, &["worktree", "add", "-q", "main", "main"]);
        git(&app, &["worktree", "add", "-q", "-b", "fix", "fix", "main"]);

        let (found, _) = discover(&root);
        assert!(!found.inside);
        let listed: Vec<(&str, &str, CheckoutKind)> = found
            .items
            .iter()
            .map(|c| (c.path.as_str(), c.repository.as_str(), c.kind))
            .collect();
        assert_eq!(
            listed,
            [
                ("app/fix", "app", CheckoutKind::Worktree),
                ("app/main", "app", CheckoutKind::Worktree),
                ("clients/svc", "clients/svc", CheckoutKind::Repository),
                ("lib-origin", "lib-origin", CheckoutKind::Repository),
            ]
        );
        let svc = found.of("clients/svc/vendor/lib/README");
        assert_eq!(svc.path, "clients/svc", "a submodule is part of its parent");
        assert_eq!(svc.submodules, ["vendor/lib"]);
        assert!(is_submodule(
            &root.join("clients/svc/vendor/lib"),
            &root.join("clients/svc")
        ));
        assert_eq!(found.of("app/fix/README").branch.as_deref(), Some("fix"));

        // Inside a repository, the project is the one checkout.
        let (inside, _) = discover(&root.join("clients/svc/vendor"));
        assert!(inside.inside);
        assert_eq!(inside.items[0].kind, CheckoutKind::Project);
        let (top, _) = discover(&root.join("clients/svc"));
        assert_eq!(top.items[0].submodules, ["vendor/lib"]);
    }

    #[test]
    fn what_was_found_is_kept_until_a_git_entry_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        repo(&root.join("one"));
        let discovery = Discovery::default();
        assert_eq!(discovery.checkouts(&root).items.len(), 1);
        let first = discovery.checkouts(&root);
        // Nothing changed: the same list, without looking again.
        assert!(Arc::ptr_eq(&first, &discovery.checkouts(&root)));
        repo(&root.join("group/two"));
        assert_eq!(discovery.checkouts(&root).items.len(), 2);
        // A new worktree in the bare layout is noticed too.
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        git(
            &app,
            &[
                "clone",
                "-q",
                "--bare",
                &root.join("one").display().to_string(),
                ".bare",
            ],
        );
        std::fs::write(app.join(".git"), "gitdir: ./.bare\n").unwrap();
        assert_eq!(
            discovery.checkouts(&root).items.len(),
            2,
            "a bare repository alone is listed by its worktrees"
        );
        git(&app, &["worktree", "add", "-q", "wt", "main"]);
        assert_eq!(discovery.checkouts(&root).items.len(), 3);
    }
}
