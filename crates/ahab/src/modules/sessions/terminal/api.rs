//! The terminals API's request and response types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::sessions::terminal::model::{Terminal, TerminalState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = TerminalState)]
pub enum TerminalStateDto {
    Running,
    /// Its process ended, or its machine restarted; kept until archived.
    Exited,
}

/// A terminal: a shell (or a command) on a project's machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Terminal)]
pub struct TerminalDto {
    pub id: Uuid,
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub machine_id: Uuid,
    /// What it runs; the login shell when absent.
    pub command: Option<String>,
    pub state: TerminalStateDto,
    /// How its process ended; absent while it runs, or when its machine
    /// restarted.
    pub exit_code: Option<i32>,
    pub cols: u16,
    pub rows: u16,
    /// The title its shell set, if any.
    pub title: Option<String>,
    /// The name you gave it, if any: shown over its title.
    pub label: Option<String>,
    /// The plugin that started it, if one did.
    pub plugin: Option<String>,
    /// The plugins whose `terminals.create` hook changed how it started.
    pub hooked_by: Vec<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<&Terminal> for TerminalDto {
    fn from(terminal: &Terminal) -> Self {
        Self {
            id: terminal.id,
            session_id: terminal.session_id,
            project_id: terminal.project_id,
            machine_id: terminal.machine_id,
            command: terminal.command.clone(),
            state: match terminal.state {
                TerminalState::Running => TerminalStateDto::Running,
                TerminalState::Exited => TerminalStateDto::Exited,
            },
            exit_code: terminal.exit_code,
            cols: terminal.cols,
            rows: terminal.rows,
            title: terminal.title.clone(),
            label: terminal.label.clone(),
            plugin: terminal.plugin.clone(),
            hooked_by: terminal.hooked_by.clone(),
            created_at: terminal.created_at,
            updated_at: terminal.updated_at,
        }
    }
}

/// A new terminal in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = StartTerminal)]
pub struct StartTerminalDto {
    /// Else the command, or "Terminal".
    pub title: Option<String>,
    /// A command for the login shell to run; the shell itself otherwise.
    pub command: Option<String>,
    /// The project's folder to file it in; else none.
    pub folder_id: Option<Uuid>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// The plugin starting it, which then sees and stops it.
    #[serde(default)]
    pub plugin: Option<String>,
    /// What it is for, by plugin (`{"docker":{"container":"web"}}`): a
    /// plugin's `terminals.create` hook may start it differently.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub context: Option<serde_json::Value>,
}

/// Terminals, such as those a plugin started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = TerminalList)]
pub struct TerminalListDto {
    pub items: Vec<TerminalDto>,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

/// What to change about a terminal; absent fields stay as they are.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UpdateTerminal)]
pub struct UpdateTerminalDto {
    /// Its name in its session, or `null` to show its shell's title again.
    #[serde(
        default,
        deserialize_with = "crate::modules::sessions::session::api::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>, nullable)]
    pub label: Option<Option<String>>,
    /// Another session of its project to move it to.
    pub session_id: Option<Uuid>,
}
