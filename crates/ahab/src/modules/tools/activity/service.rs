//! Active and pinned repositories: written when a file is opened in one
//! (the daemon tells) or an agent's turn changes one, pinned by people; a
//! project's pins go to its machine, for projects whose servers start
//! only in pinned repositories.

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::modules::machines::Presence;
use crate::modules::projects::ProjectService;
use crate::modules::tools::activity::api::{
    PinRepositoryDto, RepositoryActivityDto, RepositoryActivityListDto,
};
use crate::modules::tools::activity::model::{Activity, valid_folder};
use crate::modules::tools::activity::store::ActivityRepository;
use crate::modules::tools::setting::service::ToolSettingService;
use crate::modules::tools::setting::store::Owner;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

pub struct RepositoryActivityService {
    repo: Arc<dyn ActivityRepository>,
    projects: Arc<ProjectService>,
    settings: Arc<ToolSettingService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
}

impl RepositoryActivityService {
    pub fn new(
        repo: Arc<dyn ActivityRepository>,
        projects: Arc<ProjectService>,
        settings: Arc<ToolSettingService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            projects,
            settings,
            presence,
            clock,
        }
    }

    /// A repository (or one of its worktrees) was used now.
    pub async fn touch(
        &self,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
    ) -> Result<(), ServiceError> {
        if !valid_folder(repository) || worktree.is_some_and(|w| !valid_folder(w)) {
            return Ok(());
        }
        self.repo
            .touch(project, repository, worktree, self.clock.now())
            .await?;
        self.presence.bus().publish(
            "repositories.activity",
            json!({ "project": project, "repository": repository, "worktree": worktree }),
        );
        Ok(())
    }

    /// A daemon's `tools.activity`, when the project is on that machine.
    pub async fn receive(
        &self,
        machine: Uuid,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
    ) -> Result<(), ServiceError> {
        if self.projects.get(project).await?.machine_id != machine {
            return Ok(());
        }
        self.touch(project, repository, worktree).await
    }

    /// A project's used and pinned repositories.
    pub async fn list(&self, project: Uuid) -> Result<RepositoryActivityListDto, ServiceError> {
        let found = self.projects.get(project).await?;
        let days = self
            .settings
            .get(Owner::Project(found.id))
            .await?
            .active_days();
        let now = self.clock.now();
        let rows = self.repo.list(project).await?;
        // A repository pinned as a whole pins its worktrees.
        let whole: Vec<String> = rows
            .iter()
            .filter(|row| row.pinned && row.worktree.is_none())
            .map(|row| row.repository.clone())
            .collect();
        Ok(RepositoryActivityListDto {
            items: rows
                .into_iter()
                .map(|row| RepositoryActivityDto {
                    active: row.is_active(now, days) || whole.contains(&row.repository),
                    repository: row.repository,
                    worktree: row.worktree,
                    last_active_at: row.last_active_at,
                    pinned: row.pinned,
                })
                .collect(),
            active_days: days,
        })
    }

    /// The folders pinned in a project (worktrees', or repositories' as
    /// a whole).
    pub async fn pinned(&self, project: Uuid) -> Result<Vec<String>, ServiceError> {
        Ok(self
            .repo
            .list(project)
            .await?
            .iter()
            .filter(|row| row.pinned)
            .map(|row| Activity::folder(row).to_owned())
            .collect())
    }

    /// Pins or unpins a repository (or one worktree), and tells the
    /// project's machine.
    pub async fn pin(
        &self,
        project: Uuid,
        pin: PinRepositoryDto,
    ) -> Result<RepositoryActivityListDto, ServiceError> {
        let found = self.projects.get(project).await?;
        let bad = !valid_folder(&pin.repository)
            || pin
                .worktree
                .as_deref()
                .is_some_and(|w| w.is_empty() || !valid_folder(w));
        if bad {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "repository",
                "Name a repository (and worktree) by its folder in the project.",
            )]));
        }
        self.repo
            .pin(
                project,
                &pin.repository,
                pin.worktree.as_deref(),
                pin.pinned,
            )
            .await?;
        if self.presence.supports(found.machine_id, "tools.pins") {
            let pinned = self.pinned(project).await?;
            let told = self
                .presence
                .ask::<serde_json::Value>(
                    found.machine_id,
                    "tools.pins",
                    json!({ "project": project, "pinned": pinned }),
                    std::time::Duration::from_secs(10),
                    |_, _| None,
                    "tools_failed",
                )
                .await;
            if let Err(error) = told {
                tracing::debug!(%project, %error, "the machine did not hear of the pins");
            }
        }
        self.presence.bus().publish(
            "repositories.activity",
            json!({ "project": project, "repository": pin.repository, "worktree": pin.worktree }),
        );
        self.list(project).await
    }
}
