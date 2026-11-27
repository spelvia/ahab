//! Storing plugins' settings. The service sees only [`SettingRepository`].

use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;

use crate::modules::plugins::setting::model::Scope;
use crate::shared::database::{Db, RowExt, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait SettingRepository: Send + Sync {
    /// The values a plugin has at a scope (`scope_id` is empty for the
    /// server); `None` when none were set.
    async fn get(
        &self,
        plugin: &str,
        scope: Scope,
        scope_id: &str,
    ) -> Result<Option<Value>, RepoError>;
    async fn put(
        &self,
        plugin: &str,
        scope: Scope,
        scope_id: &str,
        values: &Value,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
}

/// The format of the values this Ahab writes
/// (`plugin_settings.setting_values_format`).
pub const VALUES_FORMAT: i32 = 1;

pub struct DbSettingRepository {
    db: Db,
}

impl DbSettingRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SettingRepository for DbSettingRepository {
    async fn get(
        &self,
        plugin: &str,
        scope: Scope,
        scope_id: &str,
    ) -> Result<Option<Value>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT setting_values, setting_values_format FROM plugin_settings \
             WHERE plugin_id = $1 AND scope = $2 AND scope_id = $3",
        )
        .bind(plugin.to_owned())
        .bind(scope.as_str())
        .bind(scope_id.to_owned())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let format = row.format("setting_values")?;
        if format != VALUES_FORMAT {
            return Err(unknown_format("plugin settings", format));
        }
        serde_json::from_str(&row.value::<String>("setting_values")?)
            .map(Some)
            .map_err(|error| RepoError::InvalidData(format!("plugin settings: {error}")))
    }

    async fn put(
        &self,
        plugin: &str,
        scope: Scope,
        scope_id: &str,
        values: &Value,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO plugin_settings (plugin_id, scope, scope_id, setting_values, \
             setting_values_format, updated_at) VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (plugin_id, scope, scope_id) DO UPDATE SET \
             setting_values = excluded.setting_values, \
             setting_values_format = excluded.setting_values_format, \
             updated_at = excluded.updated_at",
        )
        .bind(plugin.to_owned())
        .bind(scope.as_str())
        .bind(scope_id.to_owned())
        .bind(values.to_string())
        .bind(VALUES_FORMAT)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
