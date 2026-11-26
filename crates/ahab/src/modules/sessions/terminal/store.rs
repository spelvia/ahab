//! Storing terminals. The service sees only [`TerminalRepository`].

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use crate::modules::sessions::terminal::model::{Terminal, TerminalState};
use crate::shared::database::{Db, RowExt, assembled, changed_one, json_text};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait TerminalRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<Terminal>, RepoError>;
    /// A session's terminals, oldest first.
    async fn for_session(&self, session: Uuid) -> Result<Vec<Terminal>, RepoError>;
    /// Gives a terminal its name, or takes it away (`None`).
    async fn set_label(&self, id: Uuid, label: Option<&str>) -> Result<(), RepoError>;
    async fn for_project(&self, project: Uuid) -> Result<Vec<Terminal>, RepoError>;
    /// The terminals running on a machine, as the server knows them.
    async fn running_on(&self, machine: Uuid) -> Result<Vec<Terminal>, RepoError>;
    /// The terminals a plugin started.
    async fn by_plugin(&self, plugin: &str) -> Result<Vec<Terminal>, RepoError>;
    async fn insert(&self, terminal: &Terminal) -> Result<(), RepoError>;
    async fn update(&self, terminal: &Terminal) -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
}

const COLUMNS: &str = "id, session_id, project_id, machine_id, command, state, \
    exit_code, col_count, row_count, title, plugin, context, hooked_by, created_at, updated_at, context_format, hooked_by_format";

/// The format of a terminal's context this Ahab writes (`terminals.context_format`).
pub const CONTEXT_FORMAT: i32 = 1;
/// The format of the plugins that changed how a terminal started
/// (`terminals.hooked_by_format`): a JSON array of their IDs.
pub const HOOKED_BY_FORMAT: i32 = 1;

pub struct DbTerminalRepository {
    db: Db,
}

impl DbTerminalRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_terminal(row: &AnyRow) -> Result<Terminal, RepoError> {
    Ok(Terminal {
        id: row.uuid("id")?,
        session_id: row.uuid("session_id")?,
        project_id: row.uuid("project_id")?,
        machine_id: row.uuid("machine_id")?,
        command: row.value("command")?,
        state: TerminalState::parse(&row.value::<String>("state")?),
        exit_code: row.value("exit_code")?,
        cols: u16::try_from(row.value::<i32>("col_count")?).unwrap_or(80),
        rows: u16::try_from(row.value::<i32>("row_count")?).unwrap_or(24),
        title: row.value("title")?,
        label: row.value("label")?,
        plugin: row.value("plugin")?,
        // Both only tell what started the terminal: written by a newer Ahab,
        // they are left out.
        context: match row.format("context")? {
            CONTEXT_FORMAT => row
                .value::<Option<String>>("context")?
                .and_then(|text| serde_json::from_str(&text).ok()),
            _ => None,
        },
        hooked_by: match row.format("hooked_by")? {
            HOOKED_BY_FORMAT => row.json("hooked_by")?,
            _ => Vec::new(),
        },
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
    })
}

impl DbTerminalRepository {
    /// The terminals `filter` finds, in `order`; `filter` compares columns
    /// with `$1`, `$2`…, bound to `values` in order.
    async fn many(
        &self,
        filter: &str,
        order: &str,
        values: &[&str],
    ) -> Result<Vec<Terminal>, RepoError> {
        // `terminal_labels` has no column the filters name, so they stay plain.
        let sql = format!(
            "SELECT {COLUMNS}, label FROM terminals \
             LEFT JOIN terminal_labels ON terminal_labels.terminal_id = terminals.id \
             WHERE {filter} ORDER BY {order}"
        );
        let mut query = sqlx::query(assembled(sql));
        for value in values {
            query = query.bind((*value).to_owned());
        }
        query
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(to_terminal)
            .collect()
    }

    /// Binds `terminal`'s columns as `$1`… in [`COLUMNS`]' order and runs `sql`.
    async fn write(&self, sql: String, terminal: &Terminal) -> Result<u64, RepoError> {
        let result = sqlx::query(assembled(sql))
            .bind(terminal.id.to_string())
            .bind(terminal.session_id.to_string())
            .bind(terminal.project_id.to_string())
            .bind(terminal.machine_id.to_string())
            .bind(terminal.command.clone())
            .bind(terminal.state.as_str())
            .bind(terminal.exit_code)
            .bind(i32::from(terminal.cols))
            .bind(i32::from(terminal.rows))
            .bind(terminal.title.clone())
            .bind(terminal.plugin.clone())
            .bind(terminal.context.as_ref().map(serde_json::Value::to_string))
            .bind(json_text(&terminal.hooked_by)?)
            .bind(to_unix_micros(terminal.created_at))
            .bind(to_unix_micros(terminal.updated_at))
            .bind(CONTEXT_FORMAT)
            .bind(HOOKED_BY_FORMAT)
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl TerminalRepository for DbTerminalRepository {
    async fn get(&self, id: Uuid) -> Result<Option<Terminal>, RepoError> {
        Ok(self
            .many("id = $1", "id", &[&id.to_string()])
            .await?
            .into_iter()
            .next())
    }

    async fn for_session(&self, session: Uuid) -> Result<Vec<Terminal>, RepoError> {
        self.many("session_id = $1", "created_at, id", &[&session.to_string()])
            .await
    }

    async fn set_label(&self, id: Uuid, label: Option<&str>) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM terminal_labels WHERE terminal_id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        if let Some(label) = label {
            sqlx::query("INSERT INTO terminal_labels (terminal_id, label) VALUES ($1, $2)")
                .bind(id.to_string())
                .bind(label)
                .execute(&self.db)
                .await?;
        }
        Ok(())
    }

    async fn for_project(&self, project: Uuid) -> Result<Vec<Terminal>, RepoError> {
        self.many(
            "project_id = $1",
            "updated_at DESC, id DESC",
            &[&project.to_string()],
        )
        .await
    }

    async fn by_plugin(&self, plugin: &str) -> Result<Vec<Terminal>, RepoError> {
        self.many("plugin = $1", "created_at, id", &[plugin]).await
    }

    async fn running_on(&self, machine: Uuid) -> Result<Vec<Terminal>, RepoError> {
        self.many(
            "machine_id = $1 AND state = $2",
            "created_at, id",
            &[&machine.to_string(), TerminalState::Running.as_str()],
        )
        .await
    }

    async fn insert(&self, terminal: &Terminal) -> Result<(), RepoError> {
        self.write(
            format!(
                "INSERT INTO terminals ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 $10, $11, $12, $13, $14, $15, $16, $17)"
            ),
            terminal,
        )
        .await?;
        Ok(())
    }

    async fn update(&self, terminal: &Terminal) -> Result<(), RepoError> {
        let changed = self
            .write(
                "UPDATE terminals SET session_id = $2, project_id = $3, \
                 machine_id = $4, command = $5, state = $6, exit_code = $7, col_count = $8, \
                 row_count = $9, title = $10, plugin = $11, context = $12, hooked_by = $13, \
                 created_at = $14, updated_at = $15, context_format = $16, \
                 hooked_by_format = $17 WHERE id = $1"
                    .to_owned(),
                terminal,
            )
            .await?;
        changed_one(changed, format_args!("terminal {}", terminal.id))
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM terminals WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }
}
