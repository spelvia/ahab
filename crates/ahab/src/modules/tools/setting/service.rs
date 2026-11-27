//! Reading and changing tool settings per machine and project.

use std::sync::Arc;

use crate::modules::tools::setting::model::ToolSettings;
use crate::modules::tools::setting::store::{Owner, ToolSettingRepository};
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

pub struct ToolSettingService {
    repo: Arc<dyn ToolSettingRepository>,
    clock: Arc<dyn Clock>,
}

impl ToolSettingService {
    pub fn new(repo: Arc<dyn ToolSettingRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    pub async fn get(&self, owner: Owner) -> Result<ToolSettings, ServiceError> {
        Ok(self.repo.get(owner).await?)
    }

    /// Replaces `owner`'s settings, when they are valid.
    pub async fn put(
        &self,
        owner: Owner,
        settings: ToolSettings,
    ) -> Result<ToolSettings, ServiceError> {
        let problems = settings.problems();
        if !problems.is_empty() {
            return Err(ServiceError::Invalid(problems));
        }
        if matches!(owner, Owner::Project(_)) {
            let machine_only = [
                (
                    settings.max_servers.is_some(),
                    "maxServers",
                    "How many servers run is a machine's setting.",
                ),
                (
                    settings.max_checks.is_some(),
                    "maxChecks",
                    "How many checks run at once is a machine's setting.",
                ),
                (
                    settings.memory_budget.is_some(),
                    "memoryBudget",
                    "The memory budget is a machine's setting.",
                ),
            ];
            let problems: Vec<_> = machine_only
                .into_iter()
                .filter(|(set, _, _)| *set)
                .map(|(_, field, message)| crate::shared::error::FieldError::new(field, message))
                .collect();
            if !problems.is_empty() {
                return Err(ServiceError::Invalid(problems));
            }
        }
        self.repo.put(owner, &settings, self.clock.now()).await?;
        Ok(settings)
    }
}
