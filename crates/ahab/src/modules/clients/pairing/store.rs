//! Storing pairing links and requests.

use async_trait::async_trait;
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::clients::client::model::ClientKind;
use crate::modules::clients::pairing::model::{Pairing, PairingKind, PairingState};
use crate::shared::database::{Db, RowExt, assembled};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait PairingRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<Pairing>, RepoError>;
    async fn find_by_secret(&self, hash: &[u8; 32]) -> Result<Option<Pairing>, RepoError>;
    /// The open pairings of `kind` with this code's hash.
    async fn find_open_by_code(
        &self,
        kind: PairingKind,
        hash: &[u8; 32],
    ) -> Result<Vec<Pairing>, RepoError>;
    /// The pairings of `kind` still open at `now`, oldest first.
    async fn list_open(
        &self,
        kind: PairingKind,
        now: OffsetDateTime,
    ) -> Result<Vec<Pairing>, RepoError>;
    async fn insert(&self, pairing: &Pairing) -> Result<(), RepoError>;
    /// Moves a pairing from `from` to `to`; `false` when it was not in
    /// `from` (another request came first).
    async fn transition(
        &self,
        id: Uuid,
        from: PairingState,
        to: PairingState,
    ) -> Result<bool, RepoError>;
    /// Marks an approved request, by `approver`, until `expires_at`.
    async fn approve(
        &self,
        id: Uuid,
        approver: Uuid,
        expires_at: OffsetDateTime,
    ) -> Result<bool, RepoError>;
    /// Records the client a pairing created; it is used then.
    async fn mark_used(
        &self,
        id: Uuid,
        from: PairingState,
        client: Uuid,
    ) -> Result<bool, RepoError>;
    /// Expires open links the server printed (first-run links).
    async fn expire_first_run(&self) -> Result<u64, RepoError>;
    /// Expires every open link that has a code.
    async fn expire_codes(&self) -> Result<u64, RepoError>;
    /// Deletes pairings that ended before `before`.
    async fn purge(&self, before: OffsetDateTime) -> Result<u64, RepoError>;
}

const COLUMNS: &str = "id, kind, secret_hash, code_hash, code, created_by, approved_by, \
                       device_name, device_kind, user_agent, address, state, device_id, \
                       expires_at, created_at";

pub struct DbPairingRepository {
    db: Db,
}

impl DbPairingRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn hash(row: &AnyRow, column: &str) -> Result<Option<[u8; 32]>, RepoError> {
    row.value::<Option<Vec<u8>>>(column)?
        .map(|bytes| {
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| RepoError::InvalidData(format!("{column}: not a hash")))
        })
        .transpose()
}

fn to_pairing(row: &AnyRow) -> Result<Pairing, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("pairing {id}: {what}"));
    let kind: String = row.value("kind")?;
    let state: String = row.value("state")?;
    Ok(Pairing {
        id,
        kind: PairingKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
        secret_hash: hash(row, "secret_hash")?.ok_or_else(|| invalid("secret"))?,
        code_hash: hash(row, "code_hash")?,
        code: row.value("code")?,
        created_by: row.opt_uuid("created_by")?,
        approved_by: row.opt_uuid("approved_by")?,
        client_name: row.value("device_name")?,
        client_kind: row
            .value::<Option<String>>("device_kind")?
            .map(|kind| ClientKind::parse(&kind).unwrap_or(ClientKind::App)),
        user_agent: row.value("user_agent")?,
        address: row.value("address")?,
        // A state a newer Ahab added has ended, as far as this one knows.
        state: PairingState::parse(&state).unwrap_or(PairingState::Expired),
        client_id: row.opt_uuid("device_id")?,
        expires_at: row.time("expires_at")?,
        created_at: row.time("created_at")?,
    })
}

#[async_trait]
impl PairingRepository for DbPairingRepository {
    async fn get(&self, id: Uuid) -> Result<Option<Pairing>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM pairings WHERE id = $1"
        )))
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_pairing)
        .transpose()
    }

    async fn find_by_secret(&self, hash: &[u8; 32]) -> Result<Option<Pairing>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM pairings WHERE secret_hash = $1"
        )))
        .bind(hash.to_vec())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_pairing)
        .transpose()
    }

    async fn find_open_by_code(
        &self,
        kind: PairingKind,
        hash: &[u8; 32],
    ) -> Result<Vec<Pairing>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM pairings WHERE code_hash = $1 AND kind = $2 AND state = 'open'"
        )))
        .bind(hash.to_vec())
        .bind(kind.as_str())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_pairing)
        .collect()
    }

    async fn list_open(
        &self,
        kind: PairingKind,
        now: OffsetDateTime,
    ) -> Result<Vec<Pairing>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM pairings WHERE kind = $1 AND state = 'open' \
             AND expires_at > $2 ORDER BY created_at, id"
        )))
        .bind(kind.as_str())
        .bind(to_unix_micros(now))
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_pairing)
        .collect()
    }

    async fn insert(&self, pairing: &Pairing) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO pairings (id, kind, secret_hash, code_hash, code, created_by, \
             approved_by, device_name, device_kind, user_agent, address, state, device_id, \
             expires_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(pairing.id.to_string())
        .bind(pairing.kind.as_str())
        .bind(pairing.secret_hash.to_vec())
        .bind(pairing.code_hash.map(|hash| hash.to_vec()))
        .bind(pairing.code.clone())
        .bind(pairing.created_by.map(|id| id.to_string()))
        .bind(pairing.approved_by.map(|id| id.to_string()))
        .bind(pairing.client_name.clone())
        .bind(pairing.client_kind.map(ClientKind::as_str))
        .bind(pairing.user_agent.clone())
        .bind(pairing.address.clone())
        .bind(pairing.state.as_str())
        .bind(pairing.client_id.map(|id| id.to_string()))
        .bind(to_unix_micros(pairing.expires_at))
        .bind(to_unix_micros(pairing.created_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn transition(
        &self,
        id: Uuid,
        from: PairingState,
        to: PairingState,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query("UPDATE pairings SET state = $3 WHERE id = $1 AND state = $2")
            .bind(id.to_string())
            .bind(from.as_str())
            .bind(to.as_str())
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn approve(
        &self,
        id: Uuid,
        approver: Uuid,
        expires_at: OffsetDateTime,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE pairings SET state = 'approved', approved_by = $2, expires_at = $3 \
             WHERE id = $1 AND state = 'open'",
        )
        .bind(id.to_string())
        .bind(approver.to_string())
        .bind(to_unix_micros(expires_at))
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn mark_used(
        &self,
        id: Uuid,
        from: PairingState,
        client: Uuid,
    ) -> Result<bool, RepoError> {
        let result = sqlx::query(
            "UPDATE pairings SET state = 'used', device_id = $3 WHERE id = $1 AND state = $2",
        )
        .bind(id.to_string())
        .bind(from.as_str())
        .bind(client.to_string())
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn expire_first_run(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE pairings SET state = 'expired' \
             WHERE kind = 'link' AND created_by IS NULL AND state = 'open'",
        )
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected())
    }

    async fn expire_codes(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE pairings SET state = 'expired' \
             WHERE kind = 'link' AND code_hash IS NOT NULL AND state = 'open'",
        )
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected())
    }

    async fn purge(&self, before: OffsetDateTime) -> Result<u64, RepoError> {
        let result = sqlx::query("DELETE FROM pairings WHERE expires_at < $1")
            .bind(to_unix_micros(before))
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected())
    }
}
