//! Storing tool settings per machine and project. The service sees only
//! [`ToolSettingRepository`].

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::modules::tools::setting::model::{SETTINGS_FORMAT, ToolSettings};
use crate::shared::database::{Db, RowExt, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

/// Whose settings: a machine's or a project's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Machine(uuid::Uuid),
    Project(uuid::Uuid),
}

impl Owner {
    fn parts(self) -> (&'static str, String) {
        match self {
            Self::Machine(id) => ("machine", id.to_string()),
            Self::Project(id) => ("project", id.to_string()),
        }
    }
}

#[async_trait]
pub trait ToolSettingRepository: Send + Sync {
    /// The settings stored for `owner`; empty when none were.
    async fn get(&self, owner: Owner) -> Result<ToolSettings, RepoError>;
    async fn put(
        &self,
        owner: Owner,
        settings: &ToolSettings,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
}

pub struct DbToolSettingRepository {
    db: Db,
}

impl DbToolSettingRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolSettingRepository for DbToolSettingRepository {
    async fn get(&self, owner: Owner) -> Result<ToolSettings, RepoError> {
        let (scope, scope_id) = owner.parts();
        let Some(row) = sqlx::query(
            "SELECT settings, settings_format FROM tool_settings \
             WHERE scope = $1 AND scope_id = $2",
        )
        .bind(scope)
        .bind(scope_id)
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(ToolSettings::default());
        };
        let format = row.format("settings")?;
        if format != SETTINGS_FORMAT {
            return Err(unknown_format("tool_settings.settings", format));
        }
        row.json("settings")
    }

    async fn put(
        &self,
        owner: Owner,
        settings: &ToolSettings,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        let (scope, scope_id) = owner.parts();
        sqlx::query(
            "INSERT INTO tool_settings (scope, scope_id, settings, settings_format, updated_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (scope, scope_id) DO UPDATE SET \
             settings = excluded.settings, settings_format = excluded.settings_format, \
             updated_at = excluded.updated_at",
        )
        .bind(scope)
        .bind(scope_id)
        .bind(json_text(settings)?)
        .bind(SETTINGS_FORMAT)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
