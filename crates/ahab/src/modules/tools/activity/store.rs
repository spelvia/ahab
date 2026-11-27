//! Storing when repositories were used, and pins. The service sees only
//! [`ActivityRepository`].

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::tools::activity::model::Activity;
use crate::shared::database::{Db, RowExt};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait ActivityRepository: Send + Sync {
    /// A project's repositories that were used or pinned.
    async fn list(&self, project: Uuid) -> Result<Vec<Activity>, RepoError>;
    /// One was used at `at`.
    async fn touch(
        &self,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
    async fn pin(
        &self,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
        pinned: bool,
    ) -> Result<(), RepoError>;
}

pub struct DbActivityRepository {
    db: Db,
}

impl DbActivityRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ActivityRepository for DbActivityRepository {
    async fn list(&self, project: Uuid) -> Result<Vec<Activity>, RepoError> {
        let rows = sqlx::query(
            "SELECT repository, worktree, last_active_at, CAST(pinned AS INTEGER) AS pinned \
             FROM repository_activity WHERE project_id = $1 \
             ORDER BY repository, worktree",
        )
        .bind(project.to_string())
        .fetch_all(&self.db)
        .await?;
        rows.iter()
            .map(|row| {
                let worktree: String = row.value("worktree")?;
                Ok(Activity {
                    repository: row.value("repository")?,
                    worktree: (!worktree.is_empty()).then_some(worktree),
                    last_active_at: row.opt_time("last_active_at")?,
                    pinned: row.flag("pinned")?,
                })
            })
            .collect()
    }

    async fn touch(
        &self,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO repository_activity (project_id, repository, worktree, last_active_at, pinned) \
             VALUES ($1, $2, $3, $4, FALSE) \
             ON CONFLICT (project_id, repository, worktree) DO UPDATE SET \
             last_active_at = excluded.last_active_at",
        )
        .bind(project.to_string())
        .bind(repository.to_owned())
        .bind(worktree.unwrap_or_default().to_owned())
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn pin(
        &self,
        project: Uuid,
        repository: &str,
        worktree: Option<&str>,
        pinned: bool,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO repository_activity (project_id, repository, worktree, last_active_at, pinned) \
             VALUES ($1, $2, $3, NULL, $4) \
             ON CONFLICT (project_id, repository, worktree) DO UPDATE SET \
             pinned = excluded.pinned",
        )
        .bind(project.to_string())
        .bind(repository.to_owned())
        .bind(worktree.unwrap_or_default().to_owned())
        .bind(pinned)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
