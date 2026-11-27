//! Which core features are on, and switching them.

use std::sync::Arc;

use crate::modules::platform::feature::model::{FEATURES, Feature, find};
use crate::modules::platform::feature::store::FeatureRepository;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

pub struct FeatureService {
    repo: Arc<dyn FeatureRepository>,
    clock: Arc<dyn Clock>,
}

impl FeatureService {
    pub fn new(repo: Arc<dyn FeatureRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    /// Every core feature and whether it is on (on unless switched off).
    pub async fn list(&self) -> Result<Vec<(&'static Feature, bool)>, ServiceError> {
        let stored = self.repo.all().await?;
        Ok(FEATURES
            .iter()
            .map(|feature| {
                let on = feature.required
                    || stored
                        .iter()
                        .find(|(id, _)| id == feature.id)
                        .is_none_or(|(_, on)| *on);
                (feature, on)
            })
            .collect())
    }

    /// Switches a feature on or off; Sessions stays on.
    pub async fn set(&self, id: &str, enabled: bool) -> Result<(), ServiceError> {
        let Some(feature) = find(id) else {
            return Err(ServiceError::NotFound {
                code: "feature_not_found",
                message: format!("There is no core feature {id}."),
            });
        };
        if feature.required && !enabled {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "enabled",
                format!("{} is always on.", feature.name),
            )]));
        }
        self.repo.set(id, enabled, self.clock.now()).await?;
        Ok(())
    }
}
