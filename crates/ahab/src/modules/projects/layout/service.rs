//! Saving and restoring each project's windows: its layout per kind of
//! screen.

use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::modules::projects::layout::model::{ClientClass, Layout, MAX_BYTES};
use crate::modules::projects::layout::store::LayoutRepository;
use crate::modules::projects::project::service::ProjectService;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

pub struct LayoutService {
    repo: Arc<dyn LayoutRepository>,
    projects: Arc<ProjectService>,
    clock: Arc<dyn Clock>,
}

impl LayoutService {
    pub fn new(
        repo: Arc<dyn LayoutRepository>,
        projects: Arc<ProjectService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            projects,
            clock,
        }
    }

    /// The saved layout, or `None` before the first save.
    pub async fn get(
        &self,
        project: Uuid,
        class: ClientClass,
    ) -> Result<Option<Layout>, ServiceError> {
        self.projects.get(project).await?;
        Ok(self.repo.get(project, class).await?)
    }

    /// Saves `layout` over revision `expected` (0 when there is none yet).
    /// Another client's newer save is a conflict, so the caller can reload.
    pub async fn save(
        &self,
        project: Uuid,
        class: ClientClass,
        layout: Value,
        expected: i64,
    ) -> Result<Layout, ServiceError> {
        self.projects.get(project).await?;
        if serde_json::to_vec(&layout).map_or(0, |bytes| bytes.len()) > MAX_BYTES {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "layout",
                "The layout is too large.",
            )]));
        }
        let current = self.repo.get(project, class).await?;
        let revision = current.as_ref().map_or(0, |layout| layout.revision);
        if revision != expected {
            return Err(layout_changed());
        }
        let saved = Layout {
            project_id: project,
            client_class: class,
            previous: current.map(|layout| layout.layout),
            layout,
            revision: revision + 1,
            updated_at: self.clock.now(),
        };
        if !self.repo.save(&saved).await? {
            return Err(layout_changed());
        }
        Ok(saved)
    }
}

fn layout_changed() -> ServiceError {
    ServiceError::Conflict {
        code: "layout_changed",
        message: "Another window saved this project's layout; reload it.".to_owned(),
    }
}
