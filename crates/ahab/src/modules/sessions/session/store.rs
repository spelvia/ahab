//! Storing sessions and their layouts. The service sees only
//! [`SessionRepository`].

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use crate::modules::sessions::session::model::{Folder, Session, SessionKind};
use crate::shared::database::{Db, RowExt, assembled, changed_one};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<Session>, RepoError>;
    async fn list_for_project(&self, project: Uuid) -> Result<Vec<Session>, RepoError>;
    async fn insert(&self, session: &Session) -> Result<(), RepoError>;
    async fn update(&self, session: &Session) -> Result<(), RepoError>;
    /// Notes that someone named the session, so an agent's title leaves it.
    async fn mark_named(&self, id: Uuid) -> Result<(), RepoError>;
    async fn is_named(&self, id: Uuid) -> Result<bool, RepoError>;
    /// Deletes a session and its layouts; its chats are deleted first.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    /// Moves a folder's sessions out of it.
    async fn unfile(&self, folder: Uuid) -> Result<(), RepoError>;
    /// Files a folder's sessions and folders in `to` (none: the top level).
    async fn move_contents(&self, folder: Uuid, to: Option<Uuid>) -> Result<(), RepoError>;
    async fn folder(&self, id: Uuid) -> Result<Option<Folder>, RepoError>;
    async fn folders(&self, project: Uuid) -> Result<Vec<Folder>, RepoError>;
    async fn insert_folder(&self, folder: &Folder) -> Result<(), RepoError>;
    async fn update_folder(&self, folder: &Folder) -> Result<(), RepoError>;
    async fn delete_folder(&self, id: Uuid) -> Result<(), RepoError>;
}

const COLUMNS: &str = "id, project_id, kind, title, folder_id, pinned_at, \
    archived_at, created_at, updated_at";

const FOLDER_COLUMNS: &str = "id, project_id, name, created_at, updated_at";

/// A folder with its place (`session_folder_places`), as [`to_folder`] reads it.
const FOLDER_SELECTED: &str = "session_folders.id AS id, project_id, name, created_at, updated_at, \
    parent_id, pinned_at FROM session_folders \
    LEFT JOIN session_folder_places ON session_folder_places.folder_id = session_folders.id";

pub struct DbSessionRepository {
    db: Db,
}

impl DbSessionRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_session(row: &AnyRow) -> Result<Session, RepoError> {
    Ok(Session {
        id: row.uuid("id")?,
        project_id: row.uuid("project_id")?,
        kind: SessionKind::parse(&row.value::<String>("kind")?),
        title: row.value("title")?,
        folder_id: row.opt_uuid("folder_id")?,
        pinned_at: row.opt_time("pinned_at")?,
        archived_at: row.opt_time("archived_at")?,
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
    })
}

fn to_folder(row: &AnyRow) -> Result<Folder, RepoError> {
    Ok(Folder {
        id: row.uuid("id")?,
        project_id: row.uuid("project_id")?,
        name: row.value("name")?,
        parent_id: row
            .value::<Option<String>>("parent_id")?
            .and_then(|id| id.parse().ok()),
        pinned_at: row.opt_time("pinned_at")?,
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
    })
}

impl DbSessionRepository {
    /// Writes where a folder sits and whether it is pinned.
    async fn place(&self, folder: &Folder) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM session_folder_places WHERE folder_id = $1")
            .bind(folder.id.to_string())
            .execute(&self.db)
            .await?;
        if folder.parent_id.is_some() || folder.pinned_at.is_some() {
            sqlx::query(
                "INSERT INTO session_folder_places (folder_id, parent_id, pinned_at) \
                 VALUES ($1, $2, $3)",
            )
            .bind(folder.id.to_string())
            .bind(folder.parent_id.map(|id| id.to_string()))
            .bind(folder.pinned_at.map(to_unix_micros))
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SessionRepository for DbSessionRepository {
    async fn get(&self, id: Uuid) -> Result<Option<Session>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM sessions WHERE id = $1"
        )))
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_session)
        .transpose()
    }

    async fn list_for_project(&self, project: Uuid) -> Result<Vec<Session>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM sessions WHERE project_id = $1 \
             ORDER BY updated_at DESC, id DESC"
        )))
        .bind(project.to_string())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_session)
        .collect()
    }

    async fn insert(&self, session: &Session) -> Result<(), RepoError> {
        sqlx::query(assembled(format!(
            "INSERT INTO sessions ({COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )))
        .bind(session.id.to_string())
        .bind(session.project_id.to_string())
        .bind(session.kind.as_str())
        .bind(session.title.clone())
        .bind(session.folder_id.map(|folder| folder.to_string()))
        .bind(session.pinned_at.map(to_unix_micros))
        .bind(session.archived_at.map(to_unix_micros))
        .bind(to_unix_micros(session.created_at))
        .bind(to_unix_micros(session.updated_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn update(&self, session: &Session) -> Result<(), RepoError> {
        let result = sqlx::query(
            "UPDATE sessions SET project_id = $2, kind = $3, title = $4, \
             folder_id = $5, pinned_at = $6, archived_at = $7, updated_at = $8 WHERE id = $1",
        )
        .bind(session.id.to_string())
        .bind(session.project_id.to_string())
        .bind(session.kind.as_str())
        .bind(session.title.clone())
        .bind(session.folder_id.map(|folder| folder.to_string()))
        .bind(session.pinned_at.map(to_unix_micros))
        .bind(session.archived_at.map(to_unix_micros))
        .bind(to_unix_micros(session.updated_at))
        .execute(&self.db)
        .await?;
        changed_one(
            result.rows_affected(),
            format_args!("session {}", session.id),
        )
    }

    async fn mark_named(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO named_sessions (session_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn is_named(&self, id: Uuid) -> Result<bool, RepoError> {
        Ok(
            sqlx::query("SELECT 1 FROM named_sessions WHERE session_id = $1")
                .bind(id.to_string())
                .fetch_optional(&self.db)
                .await?
                .is_some(),
        )
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn move_contents(&self, folder: Uuid, to: Option<Uuid>) -> Result<(), RepoError> {
        let to = to.map(|id| id.to_string());
        sqlx::query("UPDATE sessions SET folder_id = $2 WHERE folder_id = $1")
            .bind(folder.to_string())
            .bind(to.clone())
            .execute(&self.db)
            .await?;
        sqlx::query("UPDATE session_folder_places SET parent_id = $2 WHERE parent_id = $1")
            .bind(folder.to_string())
            .bind(to)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn unfile(&self, folder: Uuid) -> Result<(), RepoError> {
        sqlx::query("UPDATE sessions SET folder_id = NULL WHERE folder_id = $1")
            .bind(folder.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn folder(&self, id: Uuid) -> Result<Option<Folder>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {FOLDER_SELECTED} WHERE session_folders.id = $1"
        )))
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_folder)
        .transpose()
    }

    async fn folders(&self, project: Uuid) -> Result<Vec<Folder>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {FOLDER_SELECTED} WHERE project_id = $1 ORDER BY name"
        )))
        .bind(project.to_string())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_folder)
        .collect()
    }

    async fn insert_folder(&self, folder: &Folder) -> Result<(), RepoError> {
        sqlx::query(assembled(format!(
            "INSERT INTO session_folders ({FOLDER_COLUMNS}) VALUES ($1, $2, $3, $4, $5)"
        )))
        .bind(folder.id.to_string())
        .bind(folder.project_id.to_string())
        .bind(folder.name.clone())
        .bind(to_unix_micros(folder.created_at))
        .bind(to_unix_micros(folder.updated_at))
        .execute(&self.db)
        .await?;
        self.place(folder).await
    }

    async fn update_folder(&self, folder: &Folder) -> Result<(), RepoError> {
        let result = sqlx::query(
            "UPDATE session_folders SET project_id = $2, name = $3, updated_at = $4 WHERE id = $1",
        )
        .bind(folder.id.to_string())
        .bind(folder.project_id.to_string())
        .bind(folder.name.clone())
        .bind(to_unix_micros(folder.updated_at))
        .execute(&self.db)
        .await?;
        changed_one(result.rows_affected(), format_args!("folder {}", folder.id))?;
        self.place(folder).await
    }

    async fn delete_folder(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM session_folders WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }
}
