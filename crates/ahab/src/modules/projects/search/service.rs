//! Searching through a project's machine.

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::machines::{Channel, Presence, shared_error};
use crate::modules::projects::project::service::ProjectService;
use crate::modules::projects::search::api::{FileIndexDto, SearchQueryDto, SearchResultsDto};
use crate::shared::error::{FieldError, ServiceError};

/// The daemon stops searching after 10 seconds; this leaves room to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct SearchService {
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
}

/// The daemon's own search errors, as the API reports them.
fn search_error(code: &str, message: &str) -> Option<ServiceError> {
    (code == "invalid_pattern")
        .then(|| ServiceError::Invalid(vec![FieldError::new("query", message.to_owned())]))
}

impl SearchService {
    pub fn new(projects: Arc<ProjectService>, presence: Arc<Presence>) -> Self {
        Self { projects, presence }
    }

    async fn ask<T: DeserializeOwned>(
        &self,
        project: Uuid,
        method: &str,
        params: Value,
    ) -> Result<T, ServiceError> {
        let project = self.projects.get(project).await?;
        self.presence
            .ask(
                project.machine_id,
                method,
                params,
                TIMEOUT,
                search_error,
                "machine_request_failed",
            )
            .await
    }

    /// Searches the project's files' contents.
    pub async fn search(
        &self,
        project: Uuid,
        query: SearchQueryDto,
    ) -> Result<SearchResultsDto, ServiceError> {
        let mut params = serde_json::to_value(query)
            .map_err(|error| ServiceError::Internal(error.to_string()))?;
        params["project"] = json!(project);
        self.ask(project, "search.run", params).await
    }

    /// Searches the project's files' contents, the results coming on the
    /// channel as they are found ([`SearchFrameDto`](crate::modules::projects::search::api::SearchFrameDto)
    /// JSON, one per frame). Dropping the channel stops the search.
    pub async fn stream(
        &self,
        project: Uuid,
        query: SearchQueryDto,
    ) -> Result<Channel, ServiceError> {
        let machine = self.projects.get(project).await?.machine_id;
        let mut params = serde_json::to_value(query)
            .map_err(|error| ServiceError::Internal(error.to_string()))?;
        params["project"] = json!(project);
        match self
            .presence
            .open_channel(machine, "search.stream", params, TIMEOUT)
            .await?
        {
            Ok(channel) => Ok(channel),
            Err(error) => Err(search_error(&error.code, &error.message)
                .unwrap_or_else(|| shared_error(error, "machine_request_failed"))),
        }
    }

    /// The project's files, for quick open.
    pub async fn index(&self, project: Uuid) -> Result<FileIndexDto, ServiceError> {
        self.ask(project, "files.index", json!({ "project": project }))
            .await
    }
}
