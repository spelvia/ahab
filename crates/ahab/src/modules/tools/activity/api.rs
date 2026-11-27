//! Active and pinned repositories on the wire.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One repository or worktree's use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryActivityDto {
    /// The repository's folder, relative to the project (`""` for the
    /// project's own).
    pub repository: String,
    /// The worktree's folder, when it is one.
    pub worktree: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_active_at: Option<time::OffsetDateTime>,
    pub pinned: bool,
    /// Pinned, or used within the project's active days.
    pub active: bool,
}

/// A project's repositories that were used or pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryActivityListDto {
    pub items: Vec<RepositoryActivityDto>,
    /// How many days a repository stays active after it was used.
    pub active_days: u32,
}

/// `PUT /projects/{id}/repository-pins`: pins or unpins one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinRepositoryDto {
    pub repository: String,
    /// A worktree of it; the repository as a whole (all its worktrees) when
    /// absent.
    #[serde(default)]
    pub worktree: Option<String>,
    pub pinned: bool,
}

/// A daemon's `tools.activity` event: a file was opened in a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub project: uuid::Uuid,
    pub repository: String,
    #[serde(default)]
    pub worktree: Option<String>,
}
