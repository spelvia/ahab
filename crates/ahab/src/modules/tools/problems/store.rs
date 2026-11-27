//! Storing Problems: the latest diagnostics per project, file and tool,
//! and what agents' turns changed. The service sees only
//! [`ProblemRepository`].

use std::collections::BTreeMap;

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::tools::problems::model::{Diagnostic, SUMMARY_FORMAT, Snapshot, TurnSummary};
use crate::shared::database::{Db, RowExt, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

/// The format of stored diagnostics (`project_diagnostics.diagnostics_format`).
pub const DIAGNOSTICS_FORMAT: i32 = 1;

#[async_trait]
pub trait ProblemRepository: Send + Sync {
    /// `source`'s diagnostics for `files` replace what it said of them;
    /// with `replace_all`, of every file (under `within`, a folder, when
    /// given). An empty list clears a file.
    async fn replace(
        &self,
        project: Uuid,
        source: &str,
        files: &BTreeMap<String, Vec<Diagnostic>>,
        replace_all: bool,
        within: Option<&str>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
    /// A project's problems, and when they last changed.
    async fn snapshot(
        &self,
        project: Uuid,
    ) -> Result<(Snapshot, Option<OffsetDateTime>), RepoError>;
    async fn put_turn(
        &self,
        chat: Uuid,
        turn: &str,
        summary: &TurnSummary,
        at: OffsetDateTime,
    ) -> Result<(), RepoError>;
    async fn turn(
        &self,
        chat: Uuid,
        turn: &str,
    ) -> Result<Option<(TurnSummary, OffsetDateTime)>, RepoError>;
}

pub struct DbProblemRepository {
    db: Db,
}

impl DbProblemRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ProblemRepository for DbProblemRepository {
    async fn replace(
        &self,
        project: Uuid,
        source: &str,
        files: &BTreeMap<String, Vec<Diagnostic>>,
        replace_all: bool,
        within: Option<&str>,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        let mut tx = self.db.begin().await?;
        let folder = within
            .map(|f| f.trim_matches('/'))
            .filter(|f| !f.is_empty());
        if replace_all && let Some(folder) = folder {
            let prefix = format!("{folder}/");
            sqlx::query(
                "DELETE FROM project_diagnostics WHERE project_id = $1 AND source = $2 \
                 AND (path = $3 OR substr(path, 1, length($4)) = $4)",
            )
            .bind(project.to_string())
            .bind(source.to_owned())
            .bind(folder.to_owned())
            .bind(prefix)
            .execute(&mut *tx)
            .await?;
        } else if replace_all {
            sqlx::query("DELETE FROM project_diagnostics WHERE project_id = $1 AND source = $2")
                .bind(project.to_string())
                .bind(source.to_owned())
                .execute(&mut *tx)
                .await?;
        }
        for (path, diagnostics) in files {
            if diagnostics.is_empty() {
                sqlx::query(
                    "DELETE FROM project_diagnostics \
                     WHERE project_id = $1 AND path = $2 AND source = $3",
                )
                .bind(project.to_string())
                .bind(path.clone())
                .bind(source.to_owned())
                .execute(&mut *tx)
                .await?;
                continue;
            }
            sqlx::query(
                "INSERT INTO project_diagnostics \
                 (project_id, path, source, diagnostics, diagnostics_format, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 ON CONFLICT (project_id, path, source) DO UPDATE SET \
                 diagnostics = excluded.diagnostics, \
                 diagnostics_format = excluded.diagnostics_format, \
                 updated_at = excluded.updated_at",
            )
            .bind(project.to_string())
            .bind(path.clone())
            .bind(source.to_owned())
            .bind(json_text(diagnostics)?)
            .bind(DIAGNOSTICS_FORMAT)
            .bind(to_unix_micros(at))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn snapshot(
        &self,
        project: Uuid,
    ) -> Result<(Snapshot, Option<OffsetDateTime>), RepoError> {
        let rows = sqlx::query(
            "SELECT path, source, diagnostics, diagnostics_format, updated_at \
             FROM project_diagnostics WHERE project_id = $1 ORDER BY path, source",
        )
        .bind(project.to_string())
        .fetch_all(&self.db)
        .await?;
        let mut snapshot = Snapshot::new();
        let mut latest: Option<OffsetDateTime> = None;
        for row in rows {
            // A newer Ahab's rows are left alone, never misread.
            if row.format("diagnostics")? != DIAGNOSTICS_FORMAT {
                continue;
            }
            let updated = row.time("updated_at")?;
            latest = Some(latest.map_or(updated, |known| known.max(updated)));
            snapshot.insert(
                (row.value::<String>("path")?, row.value::<String>("source")?),
                row.json("diagnostics")?,
            );
        }
        Ok((snapshot, latest))
    }

    async fn put_turn(
        &self,
        chat: Uuid,
        turn: &str,
        summary: &TurnSummary,
        at: OffsetDateTime,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO turn_diagnostics (chat_id, turn, summary, summary_format, created_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (chat_id, turn) DO UPDATE SET \
             summary = excluded.summary, summary_format = excluded.summary_format, \
             created_at = excluded.created_at",
        )
        .bind(chat.to_string())
        .bind(turn.to_owned())
        .bind(json_text(summary)?)
        .bind(SUMMARY_FORMAT)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn turn(
        &self,
        chat: Uuid,
        turn: &str,
    ) -> Result<Option<(TurnSummary, OffsetDateTime)>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT summary, summary_format, created_at FROM turn_diagnostics \
             WHERE chat_id = $1 AND turn = $2",
        )
        .bind(chat.to_string())
        .bind(turn.to_owned())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let format = row.format("summary")?;
        if format != SUMMARY_FORMAT {
            return Err(unknown_format("turn_diagnostics.summary", format));
        }
        Ok(Some((row.json("summary")?, row.time("created_at")?)))
    }
}
