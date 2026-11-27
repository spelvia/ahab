//! The registry on the wire: what the server sends a daemon to choose and
//! start tools (`ToolContext`), what a daemon reports (`tools.status`),
//! and what the API returns.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::modules::tools::registry::model::{LanguageTools, ToolRecord};
use crate::modules::tools::setting::model::ToolSettings;

/// Everything a daemon needs to choose tools: the records (Ahab's and
/// plugins'), each language's tools, and the settings of the machine and
/// the project. The daemon adds the repository's `.ahab/tools.json`
/// between them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolContext {
    #[serde(default)]
    pub records: Vec<ToolRecord>,
    #[serde(default)]
    pub languages: LanguageTools,
    #[serde(default)]
    pub machine: ToolSettings,
    #[serde(default)]
    pub project: Option<ToolSettings>,
    /// Where records came from that are not Ahab's: a plugin's ID.
    #[serde(default)]
    pub origins: std::collections::BTreeMap<String, String>,
}

/// `tools.status`'s params.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusRequest {
    pub context: ToolContext,
}

/// A tool's state on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ToolStateDto {
    /// Installed by Ahab in its data folder.
    Installed,
    /// Found: in a project, on `PATH`, in a toolchain.
    Found,
    /// Not here; Ahab can install it.
    Available,
    /// Not here, and Ahab cannot install it here (a `system` tool, your own).
    Missing,
    /// Its runtime is missing, or the platform has no build of it.
    CannotRun,
    /// Turned off on this machine.
    Off,
    /// A state from a newer daemon.
    #[serde(other)]
    Unknown,
}

/// Where a found tool is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlaceDto {
    Setting,
    Project,
    Environment,
    Ahab,
    Path,
    #[serde(other)]
    Unknown,
}

/// One tool on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatusDto {
    pub id: String,
    pub name: String,
    /// `languageServer`, `formatter`, `linter` or `check`.
    pub kind: String,
    pub languages: Vec<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// `npm`, `pypi`, `cargo`, `go`, `githubRelease`, `download`,
    /// `system`, or absent for your own tools.
    pub source: Option<String>,
    /// The version the registry pins.
    pub version: Option<String>,
    pub state: ToolStateDto,
    pub place: Option<PlaceDto>,
    /// Where exactly (`on PATH (fnm)`, `node_modules/.bin`).
    pub detail: Option<String>,
    pub path: Option<String>,
    /// The version it reports, or the version Ahab installed.
    pub found_version: Option<String>,
    /// Ahab installed an older version than the registry pins.
    #[serde(default)]
    pub update_available: bool,
    /// Why it cannot run, or how to get it without Ahab.
    pub reason: Option<String>,
    pub hint: Option<String>,
    /// Where its record comes from: `ahab`, a plugin's ID, `machine`.
    pub origin: String,
    /// It is one of a language's defaults.
    #[serde(default)]
    pub default: bool,
}

/// A runtime on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDto {
    /// `node`, `java`, `python`, `go`, `rust`.
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub path: Option<String>,
}

/// An install, update or removal running on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolJobDto {
    pub tool: String,
    /// `install`, `update` or `remove`.
    pub action: String,
    /// The step it is at.
    pub step: String,
    /// `running`, `done` or `failed`.
    pub state: String,
    pub message: Option<String>,
    /// The output so far, last lines.
    #[serde(default)]
    pub output: Vec<String>,
}

/// `tools.status`'s answer, and `GET /machines/{id}/tools`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MachineToolsDto {
    pub platform: String,
    pub runtimes: Vec<RuntimeDto>,
    pub tools: Vec<ToolStatusDto>,
    #[serde(default)]
    pub jobs: Vec<ToolJobDto>,
}

/// `tools.install`'s params.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub record: ToolRecord,
    /// `install`, `update` or `remove`.
    pub action: String,
    pub context: ToolContext,
}
