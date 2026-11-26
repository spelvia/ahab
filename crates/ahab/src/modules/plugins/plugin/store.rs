//! Storing plugins. The service sees only [`PluginRepository`].

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use crate::modules::plugins::plugin::model::{Manifest, Plugin, Source};
use crate::shared::database::{Db, RowExt, assembled, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait PluginRepository: Send + Sync {
    /// Every plugin, in the interceptors' order.
    async fn list(&self) -> Result<Vec<Plugin>, RepoError>;
    async fn get(&self, id: &str) -> Result<Option<Plugin>, RepoError>;
    /// Inserts it, or replaces the plugin with the same ID.
    async fn save(&self, plugin: &Plugin) -> Result<(), RepoError>;
    async fn delete(&self, id: &str) -> Result<(), RepoError>;
}

const COLUMNS: &str = "id, name, version, description, source, location, digest, manifest, \
    enabled_at, run_order, created_at, updated_at, manifest_format";

/// The format of the manifests this Ahab writes (`plugins.manifest_format`).
pub const MANIFEST_FORMAT: i32 = 1;

pub struct DbPluginRepository {
    db: Db,
}

impl DbPluginRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_plugin(row: &AnyRow, machines: Vec<Uuid>) -> Result<Plugin, RepoError> {
    let id: String = row.value("id")?;
    let format = row.format("manifest")?;
    if format != MANIFEST_FORMAT {
        return Err(unknown_format(&format!("plugin {id}'s manifest"), format));
    }
    let manifest: Manifest = serde_json::from_str(&row.value::<String>("manifest")?)
        .map_err(|error| RepoError::InvalidData(format!("plugin {id}'s manifest: {error}")))?;
    Ok(Plugin {
        id,
        name: row.value("name")?,
        version: row.value("version")?,
        description: row.value("description")?,
        source: Source::parse(&row.value::<String>("source")?),
        location: row.value("location")?,
        digest: row.value("digest")?,
        manifest,
        enabled_at: row.opt_time("enabled_at")?,
        position: row.value("run_order")?,
        machines,
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
    })
}

/// The machines chosen for plugins, as (plugin, machine) pairs.
fn to_placement(row: &AnyRow) -> Result<(String, Uuid), RepoError> {
    Ok((row.value("plugin_id")?, row.uuid("machine_id")?))
}

#[async_trait]
impl PluginRepository for DbPluginRepository {
    async fn list(&self) -> Result<Vec<Plugin>, RepoError> {
        let rows = sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM plugins ORDER BY run_order, created_at, id"
        )))
        .fetch_all(&self.db)
        .await?;
        let chosen = sqlx::query(
            "SELECT plugin_id, machine_id FROM plugin_machines ORDER BY plugin_id, machine_id",
        )
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_placement)
        .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|row| {
                let id: String = row.value("id")?;
                let machines = chosen
                    .iter()
                    .filter(|(plugin, _)| *plugin == id)
                    .map(|(_, machine)| *machine)
                    .collect();
                to_plugin(row, machines)
            })
            .collect()
    }

    async fn get(&self, id: &str) -> Result<Option<Plugin>, RepoError> {
        let Some(row) = sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM plugins WHERE id = $1"
        )))
        .bind(id.to_owned())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let machines = sqlx::query(
            "SELECT plugin_id, machine_id FROM plugin_machines WHERE plugin_id = $1 \
             ORDER BY machine_id",
        )
        .bind(id.to_owned())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(|row| to_placement(row).map(|(_, machine)| machine))
        .collect::<Result<Vec<_>, _>>()?;
        to_plugin(&row, machines).map(Some)
    }

    async fn save(&self, plugin: &Plugin) -> Result<(), RepoError> {
        let manifest = json_text(&plugin.manifest)?;
        let mut txn = self.db.begin().await?;
        sqlx::query(assembled(format!(
            "INSERT INTO plugins ({COLUMNS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, version = excluded.version, \
             description = excluded.description, source = excluded.source, \
             location = excluded.location, digest = excluded.digest, \
             manifest = excluded.manifest, manifest_format = excluded.manifest_format, \
             enabled_at = excluded.enabled_at, \
             run_order = excluded.run_order, created_at = excluded.created_at, \
             updated_at = excluded.updated_at"
        )))
        .bind(plugin.id.clone())
        .bind(plugin.name.clone())
        .bind(plugin.version.clone())
        .bind(plugin.description.clone())
        .bind(plugin.source.as_str())
        .bind(plugin.location.clone())
        .bind(plugin.digest.clone())
        .bind(manifest)
        .bind(plugin.enabled_at.map(to_unix_micros))
        .bind(plugin.position)
        .bind(to_unix_micros(plugin.created_at))
        .bind(to_unix_micros(plugin.updated_at))
        .bind(MANIFEST_FORMAT)
        .execute(&mut *txn)
        .await?;
        sqlx::query("DELETE FROM plugin_machines WHERE plugin_id = $1")
            .bind(plugin.id.clone())
            .execute(&mut *txn)
            .await?;
        for machine in &plugin.machines {
            sqlx::query("INSERT INTO plugin_machines (plugin_id, machine_id) VALUES ($1, $2)")
                .bind(plugin.id.clone())
                .bind(machine.to_string())
                .execute(&mut *txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM plugins WHERE id = $1")
            .bind(id.to_owned())
            .execute(&self.db)
            .await?;
        Ok(())
    }
}
