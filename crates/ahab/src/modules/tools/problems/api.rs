//! Problems on the wire: a daemon's checks (`tools.check`), its
//! diagnostics events, and what the API returns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::tools::problems::model::{Counts, Diagnostic, severity_name};
use crate::modules::tools::registry::api::ToolContext;

/// `tools.check`'s params.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRequest {
    pub project: Uuid,
    pub context: ToolContext,
    /// `save`, `agentTurn` or `request`.
    pub trigger: String,
    /// Only these checks (by tool ID), whatever their triggers.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Only the roots these files are in (relative to the project): what
    /// a save or an agent's turn changed. Older daemons check everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// Only the roots within these repositories or worktrees (their folders
    /// relative to the project; `""` for files in none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkouts: Option<Vec<String>>,
}

/// A check's root, before it runs (`tools.checkRoots`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckRootDto {
    pub tool: String,
    pub name: String,
    /// The root, relative to the project (empty for its folder).
    pub root: String,
    /// The repository or worktree it is in, relative to the project
    /// (empty for files in none).
    pub checkout: String,
}

/// `tools.checkRoots`' answer, and `GET /projects/{id}/check-roots`:
/// where Check project would run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckRootsDto {
    pub roots: Vec<CheckRootDto>,
    /// With more roots than this, Check project asks which to check.
    pub ask_above: u32,
}

/// One check at one root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResultDto {
    pub tool: String,
    pub name: String,
    /// The root, relative to the project (empty for its folder).
    pub root: String,
    /// It ran and its output was read.
    pub ok: bool,
    pub message: Option<String>,
    /// The repository or worktree the root is in.
    #[serde(default)]
    pub checkout: String,
    /// Its problems by file, relative to the project.
    #[serde(default)]
    pub files: BTreeMap<String, Vec<Diagnostic>>,
}

/// `tools.check`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckAnswer {
    pub results: Vec<CheckResultDto>,
}

/// A daemon's `tools.diagnostics` event: a tool's problems in a
/// project, replacing what that tool said of those files (or of every
/// file, with `replaceAll`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsEvent {
    pub project: Uuid,
    pub source: String,
    #[serde(default)]
    pub files: BTreeMap<String, Vec<Diagnostic>>,
    #[serde(default)]
    pub replace_all: bool,
    /// With `replaceAll`: only of the files under this folder (a check's
    /// root), relative to the project. Older daemons send none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
}

/// A problem's severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SeverityDto {
    Error,
    Warning,
    Information,
    Hint,
}

impl SeverityDto {
    pub fn of(severity: u8) -> Self {
        match severity_name(severity) {
            "warning" => Self::Warning,
            "information" => Self::Information,
            "hint" => Self::Hint,
            _ => Self::Error,
        }
    }
}

/// One problem, flattened for lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDto {
    /// Relative to the project.
    pub path: String,
    /// The tool's record ID (`rust-analyzer`, `cargo-clippy`).
    pub source: String,
    pub severity: SeverityDto,
    /// From 1.
    pub line: u32,
    /// From 1.
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub code: Option<String>,
    pub message: String,
    /// A plugin's fixes for it (plugin API 0.1.3): each a `title` and
    /// `edits` (`range`, `newText`) to its file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>)]
    pub fixes: Option<Vec<serde_json::Value>>,
}

impl ProblemDto {
    pub fn new(path: &str, source: &str, diagnostic: &Diagnostic) -> Self {
        let range = diagnostic.range;
        Self {
            path: path.to_owned(),
            source: source.to_owned(),
            severity: SeverityDto::of(diagnostic.severity()),
            line: range.start.line + 1,
            column: range.start.character + 1,
            end_line: range.end.line + 1,
            end_column: range.end.character + 1,
            code: diagnostic.code_text(),
            message: diagnostic.message.clone(),
            fixes: diagnostic
                .data
                .as_ref()
                .and_then(|data| data.get("fixes"))
                .and_then(serde_json::Value::as_array)
                .filter(|fixes| !fixes.is_empty())
                .cloned(),
        }
    }
}

/// Counts by severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountsDto {
    pub errors: u32,
    pub warnings: u32,
    pub information: u32,
    pub hints: u32,
}

impl From<Counts> for CountsDto {
    fn from(counts: Counts) -> Self {
        Self {
            errors: counts.errors,
            warnings: counts.warnings,
            information: counts.information,
            hints: counts.hints,
        }
    }
}

/// A project's problems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemListDto {
    pub items: Vec<ProblemDto>,
    /// Of every problem, before the filters.
    pub counts: CountsDto,
    /// When the snapshot last changed.
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProblemsQuery {
    /// The lowest severity: `error`, `warning`, `information`, `hint`.
    pub severity: Option<String>,
    /// Only this tool's.
    pub source: Option<String>,
    /// Only this file's, or those under this folder.
    pub path: Option<String>,
    /// Only those whose message, code or path contains this.
    pub text: Option<String>,
}

/// A project's totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSummaryDto {
    pub total: CountsDto,
}

/// `POST /projects/{id}/checks`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunChecksDto {
    /// Only these checks, by tool ID; every check that runs on request when
    /// absent.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Only in these repositories or worktrees (folders relative to the
    /// project, `""` for files in none); everywhere when absent.
    #[serde(default)]
    pub checkouts: Option<Vec<String>>,
}

/// One check that ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckRunDto {
    pub tool: String,
    pub name: String,
    pub root: String,
    /// The repository or worktree the root is in.
    #[serde(default)]
    pub checkout: String,
    pub ok: bool,
    pub message: Option<String>,
    pub counts: CountsDto,
}

/// What `POST /projects/{id}/checks` ran, and the problems after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChecksDto {
    pub checks: Vec<CheckRunDto>,
    pub counts: CountsDto,
}

/// A new problem after a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnFileDto {
    pub path: String,
    pub new: Vec<ProblemDto>,
    pub fixed: CountsDto,
}

/// What an agent's turn changed: "3 new errors, 1 fixed".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnDiagnosticsDto {
    pub chat: Uuid,
    pub turn: String,
    pub new: CountsDto,
    pub fixed: CountsDto,
    pub files: Vec<TurnFileDto>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}
