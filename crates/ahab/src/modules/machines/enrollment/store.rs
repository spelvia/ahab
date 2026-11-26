//! Storing one-time join codes.

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use crate::modules::machines::enrollment::model::JoinCode;
use crate::modules::machines::machine::model::MachineName;
use crate::shared::database::{Db, RowExt, assembled};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait JoinCodeRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<JoinCode>, RepoError>;
    async fn find_by_hash(&self, hash: &[u8; 32]) -> Result<Option<JoinCode>, RepoError>;
    async fn insert(&self, code: &JoinCode) -> Result<(), RepoError>;
    /// Marks a code used by `machine`; `false` when another use came first.
    async fn mark_used(&self, id: Uuid, machine: Uuid) -> Result<bool, RepoError>;
    async fn delete(&self, id: Uuid) -> Result<bool, RepoError>;
}

const COLUMNS: &str = "id, code_hash, name, expires_at, used_by, created_at";

pub struct DbJoinCodeRepository {
    db: Db,
}

impl DbJoinCodeRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_code(row: &AnyRow) -> Result<JoinCode, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("join code {id}: {what}"));
    Ok(JoinCode {
        id,
        code_hash: <[u8; 32]>::try_from(row.value::<Vec<u8>>("code_hash")?.as_slice())
            .map_err(|_| invalid("hash"))?,
        name: row
            .value::<Option<String>>("name")?
            .as_deref()
            .map(MachineName::parse)
            .transpose()
            .map_err(|_| invalid("name"))?,
        expires_at: row.time("expires_at")?,
        used_by: row.opt_uuid("used_by")?,
        created_at: row.time("created_at")?,
    })
}

#[async_trait]
impl JoinCodeRepository for DbJoinCodeRepository {
    async fn list(&self) -> Result<Vec<JoinCode>, RepoError> {
        sqlx::query(assembled(format!("SELECT {COLUMNS} FROM join_codes")))
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(to_code)
            .collect()
    }

    async fn find_by_hash(&self, hash: &[u8; 32]) -> Result<Option<JoinCode>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM join_codes WHERE code_hash = $1"
        )))
        .bind(hash.to_vec())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_code)
        .transpose()
    }

    async fn insert(&self, code: &JoinCode) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO join_codes (id, code_hash, name, expires_at, used_by, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(code.id.to_string())
        .bind(code.code_hash.to_vec())
        .bind(code.name.as_ref().map(ToString::to_string))
        .bind(to_unix_micros(code.expires_at))
        .bind(code.used_by.map(|machine| machine.to_string()))
        .bind(to_unix_micros(code.created_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn mark_used(&self, id: Uuid, machine: Uuid) -> Result<bool, RepoError> {
        // Only an unused code: two machines racing with one code cannot both
        // join.
        let result =
            sqlx::query("UPDATE join_codes SET used_by = $2 WHERE id = $1 AND used_by IS NULL")
                .bind(id.to_string())
                .bind(machine.to_string())
                .execute(&self.db)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepoError> {
        let result = sqlx::query("DELETE FROM join_codes WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
