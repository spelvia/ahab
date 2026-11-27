//! The registry on the server: Ahab's bundled records and each language's
//! tools, plugins' contributions over them, and asking machines what they
//! have and to install.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::machines::{MachineService, Presence};
use crate::modules::plugins::PluginService;
use crate::modules::tools::registry::api::{
    InstallRequest, MachineToolsDto, StatusRequest, ToolContext, ToolJobDto,
};
use crate::modules::tools::registry::model::{LanguageTools, ToolRecord, with_contributed};
use crate::modules::tools::setting::model::ToolSettings;
use crate::modules::tools::setting::service::ToolSettingService;
use crate::modules::tools::setting::store::Owner;
use crate::shared::error::ServiceError;

/// Ahab's records, one file per tool (`tools/registry/<id>.json`).
#[derive(rust_embed::RustEmbed)]
#[folder = "../../tools/registry"]
struct Registry;

/// The bundled languages, whose `tools` list theirs.
const LANGUAGES: &str = include_str!("../../../../../../apps/ui/languages/languages.json");

/// How long a machine may take to list its tools (it asks each its version).
const STATUS_TIMEOUT: Duration = Duration::from_secs(60);

/// Ahab's records, valid ones only, by ID.
pub fn bundled() -> Vec<ToolRecord> {
    let mut records: Vec<ToolRecord> = Registry::iter()
        .filter_map(|name| {
            let file = Registry::get(&name)?;
            match serde_json::from_slice::<ToolRecord>(&file.data) {
                Ok(record) if record.problems().is_empty() => Some(record),
                Ok(record) => {
                    tracing::warn!(tool = %record.id, problems = ?record.problems(), "a bundled tool record is invalid");
                    None
                }
                Err(error) => {
                    tracing::warn!(file = %name, %error, "a bundled tool record is unreadable");
                    None
                }
            }
        })
        .collect();
    records.sort_by(|a, b| a.id.cmp(&b.id));
    records
}

/// Each bundled language's tools, in order of preference.
pub fn bundled_languages() -> LanguageTools {
    languages_of(&serde_json::from_str::<Vec<Value>>(LANGUAGES).unwrap_or_default())
}

/// The `tools` of language declarations.
fn languages_of(declarations: &[Value]) -> LanguageTools {
    declarations
        .iter()
        .filter_map(|language| {
            let id = language["id"].as_str()?;
            let tools: Vec<String> = language["tools"]
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            Some((id.to_owned(), tools))
        })
        .collect()
}

pub struct ToolService {
    pub settings: Arc<ToolSettingService>,
    plugins: Arc<PluginService>,
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
}

fn machine_error(code: &str, message: &str) -> Option<ServiceError> {
    match code {
        "tool_busy" => Some(ServiceError::Conflict {
            code: "tool_busy",
            message: message.to_owned(),
        }),
        "server_not_found" => Some(ServiceError::NotFound {
            code: "language_server_not_found",
            message: message.to_owned(),
        }),
        _ => None,
    }
}

impl ToolService {
    pub fn new(
        settings: Arc<ToolSettingService>,
        plugins: Arc<PluginService>,
        machines: Arc<MachineService>,
        presence: Arc<Presence>,
    ) -> Self {
        Self {
            settings,
            plugins,
            machines,
            presence,
        }
    }

    pub fn presence(&self) -> &Arc<Presence> {
        &self.presence
    }

    /// A machine's own tool settings.
    pub async fn machine_settings(&self, machine: Uuid) -> Result<ToolSettings, ServiceError> {
        self.settings.get(Owner::Machine(machine)).await
    }

    /// What a daemon needs to choose tools on `machine`, for `project`.
    pub async fn context(
        &self,
        machine: Uuid,
        project: Option<Uuid>,
    ) -> Result<ToolContext, ServiceError> {
        let mut records = bundled();
        let mut languages = bundled_languages();
        let mut origins = BTreeMap::new();
        for plugin in self.plugins.list().await? {
            if !plugin.enabled() {
                continue;
            }
            let contributes = &plugin.manifest.contributes;
            let contributed: Vec<ToolRecord> = contributes
                .tools
                .iter()
                .filter_map(|value| serde_json::from_value::<ToolRecord>(value.clone()).ok())
                .filter(|record| record.problems().is_empty())
                .collect();
            for record in &contributed {
                origins.insert(record.id.clone(), plugin.id.clone());
            }
            with_contributed(&mut records, &mut languages, contributed);
            // A plugin's language lists its tools as Ahab's do.
            for (language, tools) in languages_of(&contributes.languages) {
                languages.insert(language, tools);
            }
        }
        let machine_settings = self.settings.get(Owner::Machine(machine)).await?;
        let project_settings = match project {
            Some(project) => Some(self.settings.get(Owner::Project(project)).await?),
            None => None,
        };
        Ok(ToolContext {
            records,
            languages,
            machine: machine_settings,
            project: project_settings,
            origins,
        })
    }

    /// The registry with `machine`'s states, and its runtimes.
    pub async fn machine_tools(
        &self,
        machine: Uuid,
        rescan: bool,
    ) -> Result<MachineToolsDto, ServiceError> {
        self.machines.get(machine).await?;
        let context = self.context(machine, None).await?;
        self.presence
            .ask(
                machine,
                "tools.status",
                serde_json::to_value(StatusRequest { context })
                    .map(|mut value| {
                        value["rescan"] = json!(rescan);
                        value
                    })
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
                STATUS_TIMEOUT,
                machine_error,
                "tools_failed",
            )
            .await
    }

    /// Starts installing, updating or removing `tool` on `machine`; its
    /// progress follows as `tools.job` events.
    pub async fn change_tool(
        &self,
        machine: Uuid,
        tool: &str,
        action: &str,
    ) -> Result<ToolJobDto, ServiceError> {
        self.machines.get(machine).await?;
        let context = self.context(machine, None).await?;
        let mut records: Vec<ToolRecord> = context.records.clone();
        records.extend(context.machine.records.iter().cloned());
        let Some(record) = records.into_iter().find(|record| record.id == tool) else {
            return Err(ServiceError::NotFound {
                code: "tool_not_found",
                message: format!("The registry has no tool {tool}."),
            });
        };
        if action != "remove" {
            let installable = record
                .source
                .as_ref()
                .is_some_and(|source| source.version().is_some());
            if !installable {
                return Err(ServiceError::Conflict {
                    code: "tool_not_installable",
                    message: format!(
                        "Ahab does not install {}{}",
                        record.name,
                        record
                            .hint
                            .as_ref()
                            .map(|hint| format!(": {hint}"))
                            .unwrap_or_default()
                    ),
                });
            }
        }
        let request = InstallRequest {
            record,
            action: action.to_owned(),
            context,
        };
        self.presence
            .ask(
                machine,
                "tools.install",
                serde_json::to_value(request)
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
                Duration::from_secs(30),
                machine_error,
                "tools_failed",
            )
            .await
    }

    /// Asks `machine`'s daemon `method` about its language servers.
    pub async fn ask_machine<T: serde::de::DeserializeOwned>(
        &self,
        machine: Uuid,
        method: &str,
        params: Value,
    ) -> Result<T, ServiceError> {
        self.machines.get(machine).await?;
        self.presence
            .ask(
                machine,
                method,
                params,
                Duration::from_secs(30),
                machine_error,
                "tools_failed",
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_record_is_valid_and_every_language_names_known_tools() {
        let records = bundled();
        assert!(records.len() >= 25, "only {} records", records.len());
        for name in Registry::iter() {
            let record: ToolRecord = serde_json::from_slice(&Registry::get(&name).unwrap().data)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                format!("{}.json", record.id),
                name,
                "a record's file is named after it"
            );
            assert!(
                record.problems().is_empty(),
                "{name}: {:?}",
                record.problems()
            );
            assert!(record.license.is_some(), "{name} records its license");
            assert!(record.source.is_some(), "{name} says where it comes from");
        }
        let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
        for (language, tools) in bundled_languages() {
            for tool in tools {
                assert!(
                    ids.contains(&tool.as_str()),
                    "{language} lists an unknown tool {tool}"
                );
            }
        }
        assert_eq!(bundled_languages()["rust"][0], "rust-analyzer");
    }
}
