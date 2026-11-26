//! Storing Ahab's own settings. The service sees only [`CoreSettingRepository`].

use async_trait::async_trait;
use serde_json::{Map, Value};
use time::OffsetDateTime;

use crate::modules::platform::setting::model::Scope;
use crate::shared::database::{Db, RowExt, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait CoreSettingRepository: Send + Sync {
    /// A section's values at a scope (`scope_id` is empty for the server);
    /// empty when none were set.
    async fn get(
        &self,
        section: &str,
        scope: Scope,
        scope_id: &str,
    ) -> Result<Map<String, Value>, RepoError>;
    async fn put(
        &self,
        section: &str,
        scope: Scope,
        scope_id: &str,
        values: &Map<String, Value>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
}

/// The format of the values this Ahab writes (`settings.setting_values_format`).
pub const VALUES_FORMAT: i32 = 1;

pub struct DbCoreSettingRepository {
    db: Db,
}

impl DbCoreSettingRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CoreSettingRepository for DbCoreSettingRepository {
    async fn get(
        &self,
        section: &str,
        scope: Scope,
        scope_id: &str,
    ) -> Result<Map<String, Value>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT setting_values, setting_values_format FROM settings \
             WHERE section = $1 AND scope = $2 AND scope_id = $3",
        )
        .bind(section.to_owned())
        .bind(scope.as_str())
        .bind(scope_id.to_owned())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(Map::new());
        };
        let format = row.format("setting_values")?;
        if format != VALUES_FORMAT {
            return Err(unknown_format("settings", format));
        }
        serde_json::from_str(&row.value::<String>("setting_values")?)
            .map_err(|error| RepoError::InvalidData(format!("settings: {error}")))
    }

    async fn put(
        &self,
        section: &str,
        scope: Scope,
        scope_id: &str,
        values: &Map<String, Value>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO settings (section, scope, scope_id, setting_values, \
             setting_values_format, updated_at) VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (section, scope, scope_id) DO UPDATE SET \
             setting_values = excluded.setting_values, \
             setting_values_format = excluded.setting_values_format, \
             updated_at = excluded.updated_at",
        )
        .bind(section.to_owned())
        .bind(scope.as_str())
        .bind(scope_id.to_owned())
        .bind(Value::Object(values.clone()).to_string())
        .bind(VALUES_FORMAT)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
