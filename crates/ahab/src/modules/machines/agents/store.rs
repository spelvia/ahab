//! Storing agent reports. The service sees only [`AgentRepository`].

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use crate::modules::machines::agents::model::{AddedAgent, AgentState, AgentStatus, CustomLaunch};
use crate::shared::database::{Db, RowExt, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait AgentRepository: Send + Sync {
    /// The latest report of every agent on `machine`, by agent.
    async fn list(&self, machine: Uuid) -> Result<Vec<AgentStatus>, RepoError>;
    /// Replaces `machine`'s report with `agents`.
    async fn replace(&self, machine: Uuid, agents: &[AgentStatus]) -> Result<(), RepoError>;
}

pub struct DbAgentRepository {
    db: Db,
}

impl DbAgentRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_status(row: &AnyRow) -> Result<AgentStatus, RepoError> {
    let machine_id = row.uuid("machine_id")?;
    let agent: String = row.value("agent")?;
    Ok(AgentStatus {
        state: AgentState::parse(&row.value::<String>("state")?).ok_or_else(|| {
            RepoError::InvalidData(format!("agent {agent} on {machine_id}: state"))
        })?,
        machine_id,
        agent,
        label: row.value("label")?,
        version: row.value("version")?,
        path: row.value("path")?,
        account: row.value("account")?,
        detail: row.value("detail")?,
        checked_at: row.time("checked_at")?,
    })
}

#[async_trait]
impl AgentRepository for DbAgentRepository {
    async fn list(&self, machine: Uuid) -> Result<Vec<AgentStatus>, RepoError> {
        sqlx::query(
            "SELECT machine_id, agent, label, state, version, path, account, detail, checked_at \
             FROM machine_agents WHERE machine_id = $1 ORDER BY agent",
        )
        .bind(machine.to_string())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_status)
        .collect()
    }

    async fn replace(&self, machine: Uuid, agents: &[AgentStatus]) -> Result<(), RepoError> {
        let mut txn = self.db.begin().await?;
        // One report at a time per machine: the machine's row is locked
        // first. Without it, on Postgres a second report deletes before the
        // first commits, misses its rows, and fails on their keys.
        sqlx::query("UPDATE machines SET id = id WHERE id = $1")
            .bind(machine.to_string())
            .execute(&mut *txn)
            .await?;
        sqlx::query("DELETE FROM machine_agents WHERE machine_id = $1")
            .bind(machine.to_string())
            .execute(&mut *txn)
            .await?;
        for status in agents {
            sqlx::query(
                "INSERT INTO machine_agents (machine_id, agent, label, state, version, path, \
                 account, detail, checked_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(machine.to_string())
            .bind(status.agent.clone())
            .bind(status.label.clone())
            .bind(status.state.as_str())
            .bind(status.version.clone())
            .bind(status.path.clone())
            .bind(status.account.clone())
            .bind(status.detail.clone())
            .bind(to_unix_micros(status.checked_at))
            .execute(&mut *txn)
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }
}

/// The agents added to each machine (ACP ones, from the registry or custom).
#[async_trait]
pub trait AddedAgentRepository: Send + Sync {
    /// `machine`'s added agents, by agent.
    async fn list(&self, machine: Uuid) -> Result<Vec<AddedAgent>, RepoError>;
    /// Adds `agent`, or replaces it when it is there.
    async fn put(&self, agent: &AddedAgent) -> Result<(), RepoError>;
    /// Removes `agent` from `machine`; whether it was there.
    async fn remove(&self, machine: Uuid, agent: &str) -> Result<bool, RepoError>;
}

pub struct DbAddedAgentRepository {
    db: Db,
}

impl DbAddedAgentRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

/// The format of a custom agent's launch this Ahab writes
/// (`machine_added_agents.launch_format`).
pub const LAUNCH_FORMAT: i32 = 1;

fn to_added(row: &AnyRow) -> Result<AddedAgent, RepoError> {
    let machine_id = row.uuid("machine_id")?;
    let agent: String = row.value("agent")?;
    let format = row.format("launch")?;
    if format != LAUNCH_FORMAT {
        return Err(unknown_format("launch", format));
    }
    let launch = row
        .value::<Option<String>>("launch")?
        .as_deref()
        .map(serde_json::from_str::<CustomLaunch>)
        .transpose()
        .map_err(|error| {
            RepoError::InvalidData(format!("agent {agent} on {machine_id}: launch: {error}"))
        })?;
    Ok(AddedAgent {
        machine_id,
        agent,
        name: row.value("name")?,
        launch,
        added_at: row.time("added_at")?,
    })
}

#[async_trait]
impl AddedAgentRepository for DbAddedAgentRepository {
    async fn list(&self, machine: Uuid) -> Result<Vec<AddedAgent>, RepoError> {
        sqlx::query(
            "SELECT machine_id, agent, name, launch, launch_format, added_at \
             FROM machine_added_agents WHERE machine_id = $1 ORDER BY agent",
        )
        .bind(machine.to_string())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_added)
        .collect()
    }

    async fn put(&self, agent: &AddedAgent) -> Result<(), RepoError> {
        let launch = agent.launch.as_ref().map(json_text).transpose()?;
        sqlx::query(
            "INSERT INTO machine_added_agents (machine_id, agent, name, launch, launch_format, \
             added_at) VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (machine_id, agent) DO UPDATE SET name = excluded.name, \
             launch = excluded.launch, launch_format = excluded.launch_format, \
             added_at = excluded.added_at",
        )
        .bind(agent.machine_id.to_string())
        .bind(agent.agent.clone())
        .bind(agent.name.clone())
        .bind(launch)
        .bind(LAUNCH_FORMAT)
        .bind(to_unix_micros(agent.added_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn remove(&self, machine: Uuid, agent: &str) -> Result<bool, RepoError> {
        let result =
            sqlx::query("DELETE FROM machine_added_agents WHERE machine_id = $1 AND agent = $2")
                .bind(machine.to_string())
                .bind(agent.to_owned())
                .execute(&self.db)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}
