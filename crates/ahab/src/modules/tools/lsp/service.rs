//! Language servers on the server: a window's channel to its project's
//! machine, and what a machine runs.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use crate::modules::machines::{Channel, Presence, shared_error};
use crate::modules::projects::ProjectService;
use crate::modules::tools::activity::service::RepositoryActivityService;
use crate::modules::tools::lsp::api::{LanguageServerListDto, ServerLogDto};
use crate::modules::tools::registry::service::ToolService;
use crate::shared::error::ServiceError;

pub struct LanguageServerService {
    tools: Arc<ToolService>,
    projects: Arc<ProjectService>,
    activity: Arc<RepositoryActivityService>,
    presence: Arc<Presence>,
}

impl LanguageServerService {
    pub fn new(
        tools: Arc<ToolService>,
        projects: Arc<ProjectService>,
        activity: Arc<RepositoryActivityService>,
        presence: Arc<Presence>,
    ) -> Self {
        Self {
            tools,
            projects,
            activity,
            presence,
        }
    }

    /// A window's channel for `project`'s files; the project's machine
    /// and project, for checking the caller may reach it.
    pub async fn connect(&self, project: Uuid) -> Result<(Channel, Uuid), ServiceError> {
        let found = self.projects.get(project).await?;
        let context = self.tools.context(found.machine_id, Some(found.id)).await?;
        let pinned = self.activity.pinned(project).await?;
        let params = json!({ "project": project, "context": context, "pinned": pinned });
        match self
            .presence
            .open_channel(
                found.machine_id,
                "lsp.connect",
                params,
                Duration::from_secs(15),
            )
            .await?
        {
            Ok(channel) => Ok((channel, found.machine_id)),
            Err(error) => Err(shared_error(error, "language_tools_failed")),
        }
    }

    /// The machine of a project.
    pub async fn machine_of(&self, project: Uuid) -> Result<Uuid, ServiceError> {
        Ok(self.projects.get(project).await?.machine_id)
    }

    pub async fn servers(&self, machine: Uuid) -> Result<LanguageServerListDto, ServiceError> {
        let budget = self.tools.machine_settings(machine).await?.memory_budget;
        self.tools
            .ask_machine(machine, "tools.servers", json!({ "memoryBudget": budget }))
            .await
    }

    pub async fn restart(&self, machine: Uuid, server: Uuid) -> Result<(), ServiceError> {
        let _: serde_json::Value = self
            .tools
            .ask_machine(machine, "tools.restart", json!({ "server": server }))
            .await?;
        Ok(())
    }

    pub async fn log(&self, machine: Uuid, server: Uuid) -> Result<ServerLogDto, ServiceError> {
        self.tools
            .ask_machine(machine, "tools.log", json!({ "server": server }))
            .await
    }
}
