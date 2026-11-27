//! Storing which core features are switched off.

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::shared::database::{Db, RowExt};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait FeatureRepository: Send + Sync {
    /// Every feature with a stored server-wide state, and whether it is on.
    async fn all(&self) -> Result<Vec<(String, bool)>, RepoError>;
    async fn set(&self, id: &str, enabled: bool, at: OffsetDateTime) -> Result<(), RepoError>;
}

pub struct DbFeatureRepository {
    db: Db,
}

impl DbFeatureRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FeatureRepository for DbFeatureRepository {
    async fn all(&self) -> Result<Vec<(String, bool)>, RepoError> {
        sqlx::query(
            "SELECT feature, CAST(enabled AS INTEGER) AS enabled FROM features \
             WHERE user_id IS NULL",
        )
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(|row| Ok((row.value("feature")?, row.flag("enabled")?)))
        .collect()
    }

    async fn set(&self, id: &str, enabled: bool, at: OffsetDateTime) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO features (feature, user_id, enabled, updated_at) \
             VALUES ($1, NULL, $2, $3) \
             ON CONFLICT (feature) WHERE user_id IS NULL DO UPDATE SET enabled = excluded.enabled, \
             updated_at = excluded.updated_at",
        )
        .bind(id.to_owned())
        .bind(enabled)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
