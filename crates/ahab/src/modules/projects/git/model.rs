//! Reading Git's machine-readable output and checking what is asked of it:
//! the pure half of the daemon's Git (no processes, no files).

use std::collections::HashMap;

use crate::modules::projects::git::api::{ChangeDto, CommitDto};
use crate::shared::error::{DaemonError, fail};

/// Sides of a diff above this size are not sent.
pub const MAX_SIDE: usize = 2 * 1024 * 1024;

/// What a failed command said, as a code.
pub fn classify(text: &str) -> DaemonError {
    let message = text.trim().to_owned();
    let has = |needle: &str| text.contains(needle);
    let code = if has("not a git repository") {
        "not_a_repository"
    } else if has("index.lock") || has("Another git process seems to be running") {
        "git_busy"
    } else if has("Please tell me who you are") || has("empty ident") {
        "identity_missing"
    } else if has("nothing to commit") || has("no changes added to commit") {
        "nothing_to_commit"
    } else if has("would be overwritten") {
        "would_overwrite_changes"
    } else if has("terminal prompts disabled")
        || has("could not read Username")
        || has("could not read Password")
        || has("Permission denied (publickey")
        || has("Host key verification failed")
        || has("Authentication failed")
    {
        "credentials_needed"
    } else if has("CONFLICT") || has("fix conflicts") || has("Merge conflict") {
        "merge_conflict"
    } else if has("not fully merged") {
        "branch_not_merged"
    } else if has("is not a valid branch name") || has("not a valid branch name") {
        "invalid_branch"
    } else if has("already exists") {
        "branch_exists"
    } else {
        "git_failed"
    };
    let message = if code == "git_busy" {
        "Git is busy in this repository (another Git process holds its lock); try again.".to_owned()
    } else {
        message
    };
    fail(code, message)
}

/// A path relative to the repository, refused if it could leave it or be
/// read as an option.
pub fn checked(path: &str) -> Result<&str, DaemonError> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.starts_with('-')
        || path.contains('\0')
        || path.contains('\\')
        || path.split('/').any(|part| part == "..");
    if bad {
        Err(fail(
            "invalid_path",
            format!("Not a path in the repository: {path}"),
        ))
    } else {
        Ok(path)
    }
}

pub fn checked_all(paths: &[String]) -> Result<Vec<&str>, DaemonError> {
    paths.iter().map(|path| checked(path)).collect()
}

/// A commit, as Git names one in history (a hash), checked.
pub fn checked_commit(commit: &str) -> Result<&str, DaemonError> {
    if (4..=64).contains(&commit.len()) && commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(commit)
    } else {
        Err(fail("invalid_commit", "Not a commit hash."))
    }
}

pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

/// `added\tremoved\tpath` records of `git diff --numstat -z`, by path.
pub fn numstat(out: &str) -> HashMap<String, (Option<u32>, Option<u32>)> {
    let mut counts = HashMap::new();
    let mut parts = out.split('\0');
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\t');
        let added = fields.next().and_then(|n| n.parse().ok());
        let removed = fields.next().and_then(|n| n.parse().ok());
        let path = match fields.next() {
            // A rename: the old and new paths follow as their own records.
            Some("") => {
                let _old = parts.next();
                parts.next().unwrap_or_default().to_owned()
            }
            Some(path) => path.to_owned(),
            None => continue,
        };
        counts.insert(path, (added, removed));
    }
    counts
}

pub fn status_name(code: char) -> &'static str {
    match code {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'T' => "typeChanged",
        _ => "modified",
    }
}

/// A side as text, or why it cannot be shown.
pub fn side(bytes: Option<Vec<u8>>) -> (Option<String>, bool, bool) {
    match bytes {
        None => (None, false, false),
        Some(bytes) if bytes.len() > MAX_SIDE => (None, false, true),
        Some(bytes) if looks_binary(&bytes) => (None, true, false),
        Some(bytes) => (
            Some(String::from_utf8_lossy(&bytes).into_owned()),
            false,
            false,
        ),
    }
}

pub const COMMIT_FORMAT: &str = "%H%x00%h%x00%an%x00%ae%x00%at%x00%P%x00%D%x00%s";

pub fn commit_from(fields: &[&str]) -> Option<CommitDto> {
    let [hash, short, author, email, time, parents, refs, subject, ..] = fields else {
        return None;
    };
    Some(CommitDto {
        hash: (*hash).to_owned(),
        short: (*short).to_owned(),
        author: (*author).to_owned(),
        email: (*email).to_owned(),
        time: time.parse().unwrap_or(0),
        subject: (*subject).to_owned(),
        parents: parents
            .split(' ')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
        refs: decorations(refs),
    })
}

/// `%D`: `HEAD -> main, origin/main, tag: v1` as `HEAD`, `main`,
/// `origin/main`, `tag: v1`.
pub fn decorations(refs: &str) -> Vec<String> {
    refs.split(", ")
        .flat_map(|item| item.split(" -> "))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `--name-status -z` records: status, path (and the new path for renames).
pub fn name_status(
    out: &str,
    counts: &HashMap<String, (Option<u32>, Option<u32>)>,
) -> Vec<ChangeDto> {
    let mut files = Vec::new();
    let mut parts = out.split('\0').filter(|p| !p.is_empty());
    while let Some(code) = parts.next() {
        let letter = code.chars().next().unwrap_or('M');
        let first = parts.next().unwrap_or_default().to_owned();
        let (path, old_path) = if matches!(letter, 'R' | 'C') {
            (parts.next().unwrap_or_default().to_owned(), Some(first))
        } else {
            (first, None)
        };
        let (added, removed) = counts.get(&path).copied().unwrap_or((None, None));
        files.push(ChangeDto {
            path,
            old_path,
            status: status_name(letter).to_owned(),
            added,
            removed,
        });
    }
    files
}

/// What a checkout found in a project is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum CheckoutKind {
    /// A folder with a `.git` folder: a repository's own checkout.
    Repository,
    /// A folder whose `.git` file names a repository's worktree, or a bare
    /// repository (`gitdir: ./.bare`).
    Worktree,
    /// The project's folder, standing for the repository it is in (or for
    /// one when no `.git` is above a file).
    Project,
    #[serde(other)]
    Unknown,
}

/// A repository or worktree inside a project.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    /// Its folder, relative to the project's (empty for the project).
    pub path: String,
    /// The repository it checks out, the same for all its worktrees: that
    /// repository's folder relative to the project's (absolute when
    /// outside it).
    pub repository: String,
    pub kind: CheckoutKind,
    pub branch: Option<String>,
    /// Its submodules' folders, relative to it: part of it, never their own.
    #[serde(default)]
    pub submodules: Vec<String>,
}

impl Checkout {
    /// The worktree it is, for activity: none for a repository's own
    /// checkout (or the project), else its folder.
    pub fn worktree(&self) -> Option<&str> {
        (self.kind == CheckoutKind::Worktree || self.path != self.repository)
            .then_some(self.path.as_str())
            .filter(|path| !path.is_empty())
    }
}

/// The repositories and worktrees of a project, as the daemon found them.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkouts {
    /// The project's folder is in a repository: it is the one checkout.
    pub inside: bool,
    pub items: Vec<Checkout>,
}

impl Checkouts {
    /// The checkout a path (relative to the project) is in: the deepest
    /// whose folder holds it. The project itself when it is in a
    /// repository, or when no checkout holds the path.
    pub fn of(&self, path: &str) -> Checkout {
        let path = path.trim_matches('/');
        self.items
            .iter()
            .filter(|checkout| {
                checkout.path.is_empty()
                    || path == checkout.path
                    || path.starts_with(&format!("{}/", checkout.path))
            })
            .max_by_key(|checkout| checkout.path.len())
            .cloned()
            .unwrap_or_else(|| Checkout {
                path: String::new(),
                repository: String::new(),
                kind: CheckoutKind::Project,
                branch: None,
                submodules: Vec::new(),
            })
    }

    /// Every checkout to work in, and the project's folder for files in
    /// none (or the project alone, when it is in a repository).
    pub fn boundaries(&self) -> Vec<Checkout> {
        let mut all = self.items.clone();
        if !self.inside {
            all.push(self.of(""));
            all.dedup_by(|a, b| a.path == b.path);
        }
        all
    }
}

/// How a `.git` file's `gitdir` reads, for what the folder holding it is:
/// a submodule's points into a repository's `.git/modules/`.
pub fn is_submodule_gitdir(gitdir: &str) -> bool {
    let gitdir = gitdir.replace('\\', "/");
    gitdir.contains("/.git/modules/") || gitdir.starts_with(".git/modules/")
}

/// The submodules' paths a `.gitmodules` lists.
pub fn gitmodules_paths(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "path").then(|| value.trim().trim_matches('"').to_owned())
        })
        .filter(|path| !path.is_empty())
        .collect()
}

#[cfg(test)]
mod checkouts {
    use super::*;

    fn checkout(path: &str, repository: &str, kind: CheckoutKind) -> Checkout {
        Checkout {
            path: path.to_owned(),
            repository: repository.to_owned(),
            kind,
            branch: None,
            submodules: Vec::new(),
        }
    }

    #[test]
    fn a_path_is_in_the_deepest_checkout_that_holds_it() {
        let found = Checkouts {
            inside: false,
            items: vec![
                checkout("app/main", "app", CheckoutKind::Worktree),
                checkout("app/fix", "app", CheckoutKind::Worktree),
                checkout("svc", "svc", CheckoutKind::Repository),
            ],
        };
        assert_eq!(found.of("app/main/src/a.rs").path, "app/main");
        assert_eq!(found.of("app/main").path, "app/main");
        assert_eq!(found.of("app/mainly/a.rs").kind, CheckoutKind::Project);
        assert_eq!(found.of("svc/pom.xml").path, "svc");
        assert_eq!(found.of("svc/pom.xml").worktree(), None);
        assert_eq!(found.of("app/fix/x").worktree(), Some("app/fix"));
        assert_eq!(found.of("README.md").path, "");
        assert_eq!(Checkouts::default().boundaries().len(), 1);
        assert_eq!(
            found.boundaries().len(),
            4,
            "the checkouts, and the project's loose files"
        );
    }

    #[test]
    fn submodules_are_known_by_their_gitdir_or_gitmodules() {
        assert!(is_submodule_gitdir("../.git/modules/lib"));
        assert!(is_submodule_gitdir(".git/modules/lib"));
        assert!(!is_submodule_gitdir("./.bare"));
        assert!(!is_submodule_gitdir("/r/app/.git/worktrees/fix"));
        assert_eq!(
            gitmodules_paths("[submodule \"lib\"]\n\tpath = vendor/lib\n\turl = x\n"),
            ["vendor/lib"]
        );
    }
}
