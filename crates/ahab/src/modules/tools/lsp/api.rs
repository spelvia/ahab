//! Language servers on the wire: opening a window's channel
//! (`lsp.connect`), the servers a machine runs (`tools.servers`), and what
//! the API returns.
//!
//! A window's channel carries JSON text both ways. From the window:
//! `open` (`path`, `language`, `version`, `text`), `change` (`path`,
//! `version`, `changes` as LSP content changes), `save`, `close`,
//! `request` (`id`, `path`, `method`, `params`, optionally `server`),
//! `cancel` (`id`). From the daemon: `status` (a file's language, root,
//! environment and tools), `diagnostics` (`path`, `source`, `items` as
//! LSP Diagnostics), `response` (`id`, `result` or `error`, `server`,
//! `externals`), `applyEdit` (`edit`, `externals`). Paths are relative to
//! the project; files outside it are `external:<token>`, granted per
//! result.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::tools::registry::api::ToolContext;

/// `lsp.connect`'s params: a window's channel for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub project: Uuid,
    pub channel: Uuid,
    pub context: ToolContext,
    /// The project's pinned repositories and worktrees (their folders),
    /// for a project whose servers start only in those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<Vec<String>>,
}

/// `tools.pins`' params: a project's pinned repositories changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinsRequest {
    pub project: Uuid,
    pub pinned: Vec<String>,
}

/// `tools.servers`' params (all optional; older servers send none).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServersRequest {
    /// The machine's memory budget setting, in MiB.
    #[serde(default)]
    pub memory_budget: Option<u64>,
}

/// A running language server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServerDto {
    pub id: Uuid,
    pub tool: String,
    pub name: String,
    /// Its root, as an absolute path on the machine.
    pub root: String,
    /// The languages of the files open in it.
    pub languages: Vec<String>,
    /// `starting`, `running`, `failed` or `stopped`.
    pub state: String,
    /// Why it failed, or what it is busy with.
    pub detail: Option<String>,
    pub pid: Option<u32>,
    /// Its memory, in bytes, when the machine says.
    pub memory: Option<u64>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: time::OffsetDateTime,
    /// How many files are open in it.
    pub documents: u32,
}

/// `tools.servers`' answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServerListDto {
    pub items: Vec<LanguageServerDto>,
    /// What they take together, in bytes. Absent from older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    /// What they may take together (the machine's memory budget), in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_budget: Option<u64>,
    /// The machine's memory, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_memory: Option<u64>,
}

/// `tools.restart` and `tools.log`'s params.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRef {
    pub server: Uuid,
}

/// A server's log: what it printed, and its LSP traffic when kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerLogDto {
    pub lines: Vec<String>,
}
