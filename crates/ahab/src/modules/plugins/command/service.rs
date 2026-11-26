//! Running a plugin's command on a machine through its daemon.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::machines::{Channel, Presence, is_shared};
use crate::modules::plugins::host::model::RunState;
use crate::modules::plugins::host::service::PluginHostService;
use crate::modules::plugins::plugin::service::PluginService;
use crate::modules::plugins::setting::service::SettingService;
use crate::modules::projects::ProjectService;
use crate::shared::error::{FieldError, ServiceError};

/// The daemon stops a command after ten minutes; this leaves room to answer.
const TIMEOUT: Duration = Duration::from_secs(630);

/// Where a command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Machine(Uuid),
    /// On the project's machine, with its folder.
    Project(Uuid),
    /// Wherever the plugin runs, when that is one machine.
    Anywhere,
}

/// The plugin's own errors, as the API reports them.
fn command_error(code: &str, message: &str) -> Option<ServiceError> {
    let message = message.to_owned();
    match code {
        "command_unknown" => Some(ServiceError::NotFound {
            code: "command_unknown",
            message,
        }),
        "channel_unknown" => Some(ServiceError::NotFound {
            code: "channel_unknown",
            message,
        }),
        "invalid_input" => Some(ServiceError::Invalid(vec![FieldError::new(
            "input", message,
        )])),
        "plugin_not_running" => Some(ServiceError::Conflict {
            code: "plugin_not_running",
            message,
        }),
        "plugin_timeout" => Some(ServiceError::Unavailable {
            code: "plugin_timeout",
            message,
        }),
        code if plugin_code(code) => Some(ServiceError::Plugin {
            code: code.to_owned(),
            message,
        }),
        _ => None,
    }
}

/// Whether `code` is one a plugin chose (`PluginError("cluster_unreachable",
/// …)`): a short `snake_case` word, and none of the codes Ahab itself answers
/// commands with.
fn plugin_code(code: &str) -> bool {
    (1..=64).contains(&code.len())
        && code.starts_with(|c: char| c.is_ascii_lowercase())
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !is_shared(code)
        && !matches!(
            code,
            "command_failed" | "channel_failed" | "method_unknown" | "ahab_unreachable"
        )
}

pub struct CommandService {
    plugins: Arc<PluginService>,
    settings: Arc<SettingService>,
    host: Arc<PluginHostService>,
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
}

impl CommandService {
    pub fn new(
        plugins: Arc<PluginService>,
        settings: Arc<SettingService>,
        host: Arc<PluginHostService>,
        projects: Arc<ProjectService>,
        presence: Arc<Presence>,
    ) -> Self {
        Self {
            plugins,
            settings,
            host,
            projects,
            presence,
        }
    }

    /// The machine `target` means for `plugin`, and the project if one.
    pub async fn machine_for(
        &self,
        plugin: &str,
        target: Target,
    ) -> Result<(Uuid, Option<Uuid>), ServiceError> {
        let found = self.plugins.get(plugin).await?;
        if !found.enabled() {
            return Err(ServiceError::Conflict {
                code: "plugin_disabled",
                message: format!("{} is disabled; enable it first.", found.name),
            });
        }
        let (machine, project) = match target {
            Target::Machine(machine) => (machine, None),
            Target::Project(project) => {
                (self.projects.get(project).await?.machine_id, Some(project))
            }
            Target::Anywhere => {
                let running: Vec<Uuid> = self
                    .host
                    .states(Some(plugin))
                    .into_iter()
                    .filter(|state| state.report.state == RunState::Running)
                    .map(|state| state.machine)
                    .collect();
                match running.as_slice() {
                    [machine] => (*machine, None),
                    [] => {
                        return Err(ServiceError::Conflict {
                            code: "plugin_not_running",
                            message: format!("{} is not running on any machine.", found.name),
                        });
                    }
                    _ => {
                        return Err(ServiceError::Invalid(vec![FieldError::new(
                            "machine",
                            format!("{} runs on several machines: say which", found.name),
                        )]));
                    }
                }
            }
        };
        Ok((machine, project))
    }

    /// Opens `plugin`'s channel `name` with `params` where `target` says.
    /// Frames from it start with a byte: data, or end with a reason.
    pub async fn open_channel(
        &self,
        plugin: &str,
        name: &str,
        params: Value,
        target: Target,
    ) -> Result<Channel, ServiceError> {
        let (machine, _) = self.machine_for(plugin, target).await?;
        match self
            .presence
            .open_channel(
                machine,
                "plugins.channel",
                json!({ "plugin": plugin, "name": name, "params": params }),
                Duration::from_secs(30),
            )
            .await?
        {
            Ok(channel) => Ok(channel),
            Err(error) => Err(command_error(&error.code, &error.message).unwrap_or(
                ServiceError::Conflict {
                    code: "channel_failed",
                    message: error.message,
                },
            )),
        }
    }

    /// Runs `plugin`'s command `name` with `input` where `target` says;
    /// answers what it answered. Dropping the future cancels it.
    pub async fn run(
        &self,
        plugin: &str,
        name: &str,
        input: Value,
        target: Target,
    ) -> Result<Value, ServiceError> {
        let (machine, project) = self.machine_for(plugin, target).await?;
        self.presence
            .ask(
                machine,
                "plugins.command",
                json!({
                    "plugin": plugin,
                    "name": name,
                    "input": input,
                    "project": project,
                    "settings": self.settings.effective(plugin, Some(machine), project).await?,
                }),
                TIMEOUT,
                command_error,
                "command_failed",
            )
            .await
    }
}
