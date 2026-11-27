//! The API's types for plugins running on machines.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::plugins::host::model::RunState;
use crate::modules::plugins::host::service::MachineState;

/// How a plugin's daemon entry is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginRunState)]
pub enum PluginRunStateDto {
    /// Its build is on the way to the machine.
    Receiving,
    Starting,
    Running,
    /// It crashed and starts again shortly.
    Restarting,
    /// It crashed too often; off until it changes or is enabled again.
    Failed,
    /// It cannot run there (no Node 24, a Rust entry).
    Unavailable,
    Stopped,
}

impl From<RunState> for PluginRunStateDto {
    fn from(state: RunState) -> Self {
        match state {
            RunState::Receiving => Self::Receiving,
            RunState::Starting => Self::Starting,
            RunState::Running => Self::Running,
            RunState::Restarting => Self::Restarting,
            RunState::Failed => Self::Failed,
            RunState::Unavailable => Self::Unavailable,
            RunState::Stopped => Self::Stopped,
        }
    }
}

/// A plugin's daemon entry on one machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginMachineState)]
pub struct PluginMachineStateDto {
    pub plugin: String,
    pub machine: Uuid,
    pub state: PluginRunStateDto,
    /// Why, when it is restarting, failed or unavailable.
    pub message: Option<String>,
    /// Which build runs there.
    pub digest: String,
    /// What it declared when it started (commands, channels, hooks,
    /// events), while running.
    pub declared: Option<Value>,
    /// Since when it is in this state.
    #[serde(with = "crate::shared::time::rfc3339")]
    pub since: OffsetDateTime,
}

impl From<&MachineState> for PluginMachineStateDto {
    fn from(state: &MachineState) -> Self {
        Self {
            plugin: state.report.plugin.clone(),
            machine: state.machine,
            state: state.report.state.into(),
            message: state.report.message.clone(),
            digest: state.report.digest.clone(),
            declared: state.report.declared.clone(),
            since: state.since,
        }
    }
}

/// Plugins' states on the machines online now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginMachineStateList)]
pub struct PluginMachineStateListDto {
    pub items: Vec<PluginMachineStateDto>,
}

/// A plugin's logs on a machine, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginLogs)]
pub struct PluginLogsDto {
    pub lines: Vec<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MachineQuery {
    /// The machine's ID.
    pub machine: Uuid,
}
