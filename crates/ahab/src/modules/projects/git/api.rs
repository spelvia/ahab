//! The Git API's types, which are also the daemon's messages.

use crate::modules::projects::git::model::CheckoutKind;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// One operation, by name, with its checked arguments: the only Git the
/// server can ask a daemon for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GitOp {
    /// The repository the project's folder is in, or those inside it.
    Repositories,
    Status,
    Diff {
        path: String,
        old_path: Option<String>,
        group: DiffGroup,
        commit: Option<String>,
    },
    Stage {
        paths: Vec<String>,
        all: bool,
    },
    Unstage {
        paths: Vec<String>,
        all: bool,
    },
    /// Sets a file's staged content (staging or unstaging hunks and lines),
    /// if its index entry is still `index` (absent: not in the index).
    StagePart {
        path: String,
        content: String,
        index: Option<String>,
    },
    Discard {
        paths: Vec<String>,
    },
    Commit {
        message: String,
        amend: bool,
    },
    Branches,
    Switch {
        branch: String,
        remote: bool,
        stash: bool,
    },
    CreateBranch {
        name: String,
        from: Option<String>,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    Fetch,
    Pull,
    Push,
    Log {
        skip: u32,
        limit: u32,
        /// Every branch, remote branch and tag, not only `HEAD`'s history (the graph).
        #[serde(default)]
        all: bool,
    },
    Show {
        commit: String,
    },
    Stashes,
    Stash {
        message: Option<String>,
        untracked: bool,
    },
    StashApply {
        index: u32,
        pop: bool,
    },
    StashDrop {
        index: u32,
    },
    MarkResolved {
        paths: Vec<String>,
    },
    Continue,
    Abort,
    Init,
}

impl GitOp {
    /// Whether it changes the repository (refused in read-only projects,
    /// one at a time per repository).
    pub fn writes(&self) -> bool {
        !matches!(
            self,
            Self::Repositories
                | Self::Status
                | Self::Diff { .. }
                | Self::Branches
                | Self::Log { .. }
                | Self::Show { .. }
                | Self::Stashes
        )
    }

    /// Whether it talks to a remote (a longer time limit).
    pub fn network(&self) -> bool {
        matches!(self, Self::Fetch | Self::Pull | Self::Push)
    }
}

/// `git.run` on the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRequest {
    pub project: Uuid,
    /// A repository inside the project's folder (its path there); absent
    /// for the repository the folder itself is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(flatten)]
    pub op: GitOp,
}

/// A repository inside the project's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitRepository)]
pub struct RepositoryDto {
    /// Its folder, relative to the project's.
    pub path: String,
    pub branch: Option<String>,
    /// The repository it checks out, the same for all its worktrees: that
    /// repository's folder relative to the project's (absolute when
    /// outside it). Absent from older daemons.
    #[serde(default)]
    pub repository: Option<String>,
    /// A repository's own checkout or a worktree. Absent from
    /// older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<CheckoutKind>,
}

/// Where the project's Git is.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitRepositories)]
pub struct RepositoriesDto {
    /// The project's folder is in a repository (then `repositories` is
    /// empty: that one is used).
    pub inside: bool,
    /// Repositories found inside the folder, otherwise.
    pub repositories: Vec<RepositoryDto>,
}

/// Which two sides a diff compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DiffGroup {
    /// The working tree against the index.
    Changes,
    /// The index against the last commit.
    Staged,
    /// A new file against nothing.
    Untracked,
    /// The working tree against our side of a conflict.
    Conflicts,
    /// A commit against its first parent (with `commit`).
    Commit,
}

/// A changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitChange)]
pub struct ChangeDto {
    /// Relative to the repository.
    pub path: String,
    /// Before a rename or copy.
    pub old_path: Option<String>,
    /// `modified`, `added`, `deleted`, `renamed`, `copied`, `typeChanged`,
    /// `untracked` or `conflicted`.
    #[schema(example = "modified")]
    pub status: String,
    /// Lines added and removed; absent for binary files.
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

/// Where the project stands in Git.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitStatus)]
pub struct StatusDto {
    /// False when the project's folder is in no repository.
    pub repository: bool,
    /// The project's folder within the repository (`""` when it is the
    /// repository; `"packages/web/"` in a larger one).
    pub prefix: String,
    /// The current branch; absent when `HEAD` is detached.
    pub branch: Option<String>,
    /// The commit `HEAD` is at, short; absent before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// An unfinished `merge`, `rebase`, `cherryPick` or `revert`.
    pub operation: Option<String>,
    pub staged: Vec<ChangeDto>,
    pub changes: Vec<ChangeDto>,
    pub untracked: Vec<ChangeDto>,
    pub conflicts: Vec<ChangeDto>,
}

/// A file's two sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitDiff)]
pub struct DiffDto {
    pub path: String,
    pub old_path: Option<String>,
    /// Absent when the file did not exist on that side.
    pub old: Option<String>,
    pub new: Option<String>,
    /// Not text: no sides are sent.
    pub binary: bool,
    /// Larger than 2 MiB: no sides are sent.
    pub too_large: bool,
    /// The file's index entry (a blob ID), for staging part of it; absent
    /// when it is not in the index. Sent only for changes, staged and
    /// untracked files whose sides are exact text (UTF-8), the only ones
    /// whose parts can be staged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
    /// Whether parts of it can be staged or unstaged (see `index`).
    #[serde(default)]
    pub partial: bool,
}

/// A branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitBranch)]
pub struct BranchDto {
    /// `main`, or `origin/main` for a remote one.
    pub name: String,
    pub upstream: Option<String>,
    /// Its last commit, short, with its summary and time.
    pub commit: String,
    pub subject: String,
    pub time: i64,
}

/// The repository's branches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitBranches)]
pub struct BranchesDto {
    pub current: Option<String>,
    pub local: Vec<BranchDto>,
    pub remote: Vec<BranchDto>,
}

/// A commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitCommit)]
pub struct CommitDto {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub email: String,
    /// Seconds since 1970.
    pub time: i64,
    pub subject: String,
    pub parents: Vec<String>,
    /// The refs pointing at it: `HEAD`, `main`, `origin/main`, `tag: v1`.
    /// `HEAD -> main` is given as `HEAD` then `main`.
    #[serde(default)]
    pub refs: Vec<String>,
}

/// A page of history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitLog)]
pub struct LogDto {
    pub commits: Vec<CommitDto>,
    pub more: bool,
}

/// A commit with its message and files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitCommitDetail)]
pub struct CommitDetailDto {
    pub commit: CommitDto,
    pub body: String,
    pub files: Vec<ChangeDto>,
}

/// A stash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitStash)]
pub struct StashDto {
    /// `stash@{index}`.
    pub index: u32,
    pub message: String,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitStashes)]
pub struct StashesDto {
    pub items: Vec<StashDto>,
}

/// What Git said, after an operation that changed something.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitOutput)]
pub struct OutputDto {
    pub output: String,
}

// Request bodies.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = GitStagePart)]
pub struct StagePartDto {
    /// Relative to the repository.
    pub path: String,
    /// The file's whole staged content after the change.
    pub content: String,
    /// The index entry the content was made from (`GitDiff.index`); the
    /// change is refused if it has moved on since.
    pub index: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitPaths)]
pub struct PathsDto {
    /// Relative to the repository.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Every file instead of `paths`.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitCommitRequest)]
pub struct CommitRequestDto {
    pub message: String,
    #[serde(default)]
    pub amend: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitSwitch)]
pub struct SwitchDto {
    pub branch: String,
    /// `branch` is a remote branch: switch to a local one tracking it.
    #[serde(default)]
    pub remote: bool,
    /// Stash uncommitted changes first.
    #[serde(default)]
    pub stash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitCreateBranch)]
pub struct CreateBranchDto {
    pub name: String,
    /// A branch or commit to start from; else the current one.
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitRenameBranch)]
pub struct RenameBranchDto {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = GitStashRequest)]
pub struct StashRequestDto {
    pub message: Option<String>,
    /// Include untracked files.
    #[serde(default)]
    pub untracked: bool,
}
