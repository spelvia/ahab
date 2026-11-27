//! Reading and writing plugins' settings, and the values in effect where
//! a plugin runs.

use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::modules::plugins::plugin::service::PluginService;
use crate::modules::plugins::setting::model::{self, Scope};
use crate::modules::plugins::setting::store::SettingRepository;
use crate::modules::projects::ProjectService;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

/// Where values apply: a scope and what in it (none for the server).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub scope: Scope,
    pub id: Option<Uuid>,
}

pub struct SettingService {
    repo: Arc<dyn SettingRepository>,
    plugins: Arc<PluginService>,
    projects: Arc<ProjectService>,
    clock: Arc<dyn Clock>,
}

impl SettingService {
    pub fn new(
        repo: Arc<dyn SettingRepository>,
        plugins: Arc<PluginService>,
        projects: Arc<ProjectService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            plugins,
            projects,
            clock,
        }
    }

    fn key(at: At) -> Result<String, ServiceError> {
        match (at.scope, at.id) {
            (Scope::Server, _) => Ok(String::new()),
            (_, Some(id)) => Ok(id.to_string()),
            (scope, None) => Err(ServiceError::Invalid(vec![FieldError::new(
                "id",
                format!("which {}?", scope.as_str()),
            )])),
        }
    }

    /// A plugin's schema, and the values set at `at` (an empty object when
    /// none were).
    pub async fn get(&self, plugin: &str, at: At) -> Result<(Option<Value>, Value), ServiceError> {
        let found = self.plugins.get(plugin).await?;
        let values = self
            .repo
            .get(plugin, at.scope, &Self::key(at)?)
            .await?
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        Ok((found.manifest.settings, values))
    }

    /// Sets a plugin's values at `at`; its entries hear of it.
    pub async fn put(&self, plugin: &str, at: At, values: Value) -> Result<(), ServiceError> {
        let found = self.plugins.get(plugin).await?;
        let Some(fields) = values.as_object() else {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "values",
                "must be an object of fields",
            )]));
        };
        let problems = model::problems(found.manifest.settings.as_ref(), fields);
        if !problems.is_empty() {
            return Err(ServiceError::Invalid(
                problems
                    .into_iter()
                    .map(|(field, message)| FieldError::new(format!("values.{field}"), message))
                    .collect(),
            ));
        }
        self.repo
            .put(plugin, at.scope, &Self::key(at)?, &values, self.clock.now())
            .await?;
        self.plugins.touch(plugin);
        Ok(())
    }

    /// The values in effect on `machine`, for `project` when given (its
    /// project's and its own over the machine's and the server's).
    pub async fn effective(
        &self,
        plugin: &str,
        machine: Option<Uuid>,
        project: Option<Uuid>,
    ) -> Result<Value, ServiceError> {
        let found = self.plugins.get(plugin).await?;
        let mut layers = Vec::new();
        let mut add = |value: Option<Value>| layers.push(value.unwrap_or(Value::Null));
        add(self.repo.get(plugin, Scope::Server, "").await?);
        let (machine, project) = match project {
            Some(id) => {
                let project = self.projects.get(id).await?;
                (Some(project.machine_id), Some(project.id))
            }
            None => (machine, None),
        };
        if let Some(machine) = machine {
            add(self
                .repo
                .get(plugin, Scope::Machine, &machine.to_string())
                .await?);
        }
        if let Some(project) = project {
            add(self
                .repo
                .get(plugin, Scope::Project, &project.to_string())
                .await?);
        }
        Ok(model::effective(found.manifest.settings.as_ref(), &layers))
    }
}
