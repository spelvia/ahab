//! Reading and changing Ahab's own settings.

use std::sync::Arc;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::modules::platform::setting::model::{self, Scope, Section};
use crate::modules::platform::setting::store::CoreSettingRepository;
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

/// A section's values: in effect, and as set for the server and the
/// project asked about.
pub struct Values {
    pub section: &'static Section,
    pub effective: Map<String, Value>,
    pub server: Map<String, Value>,
    pub project: Option<Map<String, Value>>,
}

pub struct CoreSettingService {
    repo: Arc<dyn CoreSettingRepository>,
    clock: Arc<dyn Clock>,
}

fn section(id: &str) -> Result<&'static Section, ServiceError> {
    model::find(id).ok_or_else(|| ServiceError::NotFound {
        code: "settings_not_found",
        message: format!("Ahab has no settings called {id}."),
    })
}

impl CoreSettingService {
    pub fn new(repo: Arc<dyn CoreSettingRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    /// A section's values, for `project` when given.
    pub async fn get(&self, id: &str, project: Option<Uuid>) -> Result<Values, ServiceError> {
        let section = section(id)?;
        let server = self.repo.get(section.id, Scope::Server, "").await?;
        let project = match project {
            Some(project) => Some(
                self.repo
                    .get(section.id, Scope::Project, &project.to_string())
                    .await?,
            ),
            None => None,
        };
        let mut layers = vec![&server];
        if let Some(project) = &project {
            layers.push(project);
        }
        Ok(Values {
            section,
            effective: model::effective(section, &layers),
            server,
            project,
        })
    }

    /// Changes a section's values for the server, or for `project`: named
    /// fields take their values, `null` unsets them there.
    pub async fn change(
        &self,
        id: &str,
        project: Option<Uuid>,
        change: &Map<String, Value>,
    ) -> Result<Values, ServiceError> {
        let section = section(id)?;
        let (scope, scope_id) = match project {
            Some(project) => (Scope::Project, project.to_string()),
            None => (Scope::Server, String::new()),
        };
        let current = self.repo.get(section.id, scope, &scope_id).await?;
        let values = model::apply(section, scope, &current, change)?;
        self.repo
            .put(section.id, scope, &scope_id, &values, self.clock.now())
            .await?;
        self.get(id, project).await
    }
}
