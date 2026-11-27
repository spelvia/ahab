//! The commands API's types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

/// A command to run, and where.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RunPluginCommand)]
pub struct RunCommandDto {
    /// The command's input, as its schema says.
    #[serde(default)]
    pub input: Value,
    /// The machine to run it on.
    #[serde(default)]
    pub machine: Option<Uuid>,
    /// A project: the command runs on its machine, and gets its folder.
    #[serde(default)]
    pub project: Option<Uuid>,
}

/// What the command answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginCommandResult)]
pub struct CommandResultDto {
    pub output: Value,
}
