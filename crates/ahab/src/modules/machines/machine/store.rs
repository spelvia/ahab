//! Storing machines. The service sees only [`MachineRepository`]; the SQL
//! stays here.

use async_trait::async_trait;
use sqlx::Any;
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::machines::machine::model::{Machine, MachineName, MachineState, PublicKey};
use crate::shared::database::{Db, RowExt, assembled, changed_one};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait MachineRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Machine>, RepoError>;
    async fn get(&self, id: Uuid) -> Result<Option<Machine>, RepoError>;
    async fn find_by_name(&self, name: &MachineName) -> Result<Option<Machine>, RepoError>;
    async fn find_by_key(&self, key: &PublicKey) -> Result<Option<Machine>, RepoError>;
    async fn find_local(&self) -> Result<Option<Machine>, RepoError>;
    async fn insert(&self, machine: &Machine) -> Result<(), RepoError>;
    /// Saves every field but the ID and creation time.
    async fn update(&self, machine: &Machine) -> Result<(), RepoError>;
    /// Whether a machine was deleted.
    async fn delete(&self, id: Uuid) -> Result<bool, RepoError>;
    /// Deletes pending machines created before `cutoff`; how many.
    async fn delete_pending_before(&self, cutoff: OffsetDateTime) -> Result<u64, RepoError>;
}

/// The columns a machine is read from, in [`to_machine`]'s terms.
const COLUMNS: &str = "id, name, state, CAST(is_local AS INTEGER) AS is_local, public_key, \
    os, arch, version, created_at, approved_at, last_seen_at";

/// Machines in the SQL database.
pub struct DbMachineRepository {
    db: Db,
}

impl DbMachineRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_machine(row: &AnyRow) -> Result<Machine, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("machine {id}: {what}"));
    Ok(Machine {
        id,
        name: MachineName::parse(&row.value::<String>("name")?).map_err(|_| invalid("name"))?,
        state: MachineState::parse(&row.value::<String>("state")?)
            .ok_or_else(|| invalid("state"))?,
        is_local: row.flag("is_local")?,
        public_key: PublicKey::from_bytes(&row.value::<Vec<u8>>("public_key")?)
            .map_err(|_| invalid("public key"))?,
        os: row.value("os")?,
        arch: row.value("arch")?,
        version: row.value("version")?,
        created_at: row.time("created_at")?,
        approved_at: row.opt_time("approved_at")?,
        last_seen_at: row.opt_time("last_seen_at")?,
    })
}

impl DbMachineRepository {
    /// The machine `filter` finds; `filter` compares one column with `$1`.
    async fn one<'q, T>(&self, filter: &str, value: T) -> Result<Option<Machine>, RepoError>
    where
        T: 'q + sqlx::Encode<'q, Any> + sqlx::Type<Any> + Send,
    {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM machines WHERE {filter}"
        )))
        .bind(value)
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_machine)
        .transpose()
    }
}

#[async_trait]
impl MachineRepository for DbMachineRepository {
    async fn list(&self) -> Result<Vec<Machine>, RepoError> {
        sqlx::query(assembled(format!("SELECT {COLUMNS} FROM machines")))
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(to_machine)
            .collect()
    }

    async fn get(&self, id: Uuid) -> Result<Option<Machine>, RepoError> {
        self.one("id = $1", id.to_string()).await
    }

    async fn find_by_name(&self, name: &MachineName) -> Result<Option<Machine>, RepoError> {
        self.one("name = $1", name.as_str().to_owned()).await
    }

    async fn find_by_key(&self, key: &PublicKey) -> Result<Option<Machine>, RepoError> {
        self.one("public_key = $1", key.as_bytes().to_vec()).await
    }

    async fn find_local(&self) -> Result<Option<Machine>, RepoError> {
        self.one("is_local = $1", true).await
    }

    async fn insert(&self, machine: &Machine) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO machines (id, name, state, is_local, public_key, os, arch, version, \
             created_at, approved_at, last_seen_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(machine.id.to_string())
        .bind(machine.name.as_str().to_owned())
        .bind(machine.state.as_str())
        .bind(machine.is_local)
        .bind(machine.public_key.as_bytes().to_vec())
        .bind(machine.os.clone())
        .bind(machine.arch.clone())
        .bind(machine.version.clone())
        .bind(to_unix_micros(machine.created_at))
        .bind(machine.approved_at.map(to_unix_micros))
        .bind(machine.last_seen_at.map(to_unix_micros))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn update(&self, machine: &Machine) -> Result<(), RepoError> {
        let result = sqlx::query(
            "UPDATE machines SET name = $2, state = $3, is_local = $4, public_key = $5, os = $6, \
             arch = $7, version = $8, approved_at = $9, last_seen_at = $10 WHERE id = $1",
        )
        .bind(machine.id.to_string())
        .bind(machine.name.as_str().to_owned())
        .bind(machine.state.as_str())
        .bind(machine.is_local)
        .bind(machine.public_key.as_bytes().to_vec())
        .bind(machine.os.clone())
        .bind(machine.arch.clone())
        .bind(machine.version.clone())
        .bind(machine.approved_at.map(to_unix_micros))
        .bind(machine.last_seen_at.map(to_unix_micros))
        .execute(&self.db)
        .await?;
        changed_one(
            result.rows_affected(),
            format_args!("machine {}", machine.id),
        )
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepoError> {
        let result = sqlx::query("DELETE FROM machines WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_pending_before(&self, cutoff: OffsetDateTime) -> Result<u64, RepoError> {
        let result = sqlx::query("DELETE FROM machines WHERE state = $1 AND created_at < $2")
            .bind(MachineState::Pending.as_str())
            .bind(to_unix_micros(cutoff))
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected())
    }
}
