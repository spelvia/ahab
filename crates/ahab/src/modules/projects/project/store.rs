//! Storing projects. The service sees only [`ProjectRepository`].

use async_trait::async_trait;
use sqlx::any::AnyRow;
use uuid::Uuid;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::modules::projects::project::model::{
    Project, ProjectIcon, ProjectLook, ProjectMode, ProjectName, ProjectState,
};
use crate::shared::database::{Db, RowExt, assembled, changed_one};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    /// Projects in `state`, or every project, in the order chosen by hand,
    /// then by name.
    async fn list(&self, state: Option<ProjectState>) -> Result<Vec<Project>, RepoError>;
    /// Every project on a machine.
    async fn list_for_machine(&self, machine: Uuid) -> Result<Vec<Project>, RepoError>;
    async fn get(&self, id: Uuid) -> Result<Option<Project>, RepoError>;
    /// The active projects called `name`, oldest first.
    async fn find_active(&self, name: &ProjectName) -> Result<Vec<Project>, RepoError>;
    /// Every project of this folder on this machine, oldest first.
    async fn find_folder(&self, machine: Uuid, path: &str) -> Result<Vec<Project>, RepoError>;
    async fn insert(&self, project: &Project) -> Result<(), RepoError>;
    async fn update(&self, project: &Project) -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<bool, RepoError>;
    /// Orders projects by hand: each of `ids` at its place in it.
    async fn set_order(&self, ids: &[Uuid]) -> Result<(), RepoError>;
    /// Sets a project's own mark, or (none) takes it away.
    async fn set_icon(
        &self,
        id: Uuid,
        icon: Option<&ProjectIcon>,
        at: time::OffsetDateTime,
    ) -> Result<(), RepoError>;
    /// A project's image, as its type and bytes.
    async fn icon_image(&self, id: Uuid) -> Result<Option<(String, Vec<u8>)>, RepoError>;
}

const COLUMNS: &str = "p.id, p.name, p.description, p.state, p.machine_id, p.path, p.mode, \
     p.project_group, p.created_at, p.updated_at, l.position, l.emoji, l.image_type, l.icon_at";
/// Projects with how they look (no row: as they come).
const FROM: &str = "projects p LEFT JOIN project_looks l ON l.project_id = p.id";

pub struct DbProjectRepository {
    db: Db,
}

impl DbProjectRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_project(row: &AnyRow) -> Result<Project, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("project {id}: {what}"));
    Ok(Project {
        id,
        name: ProjectName::parse(&row.value::<String>("name")?).map_err(|_| invalid("name"))?,
        description: row.value("description")?,
        state: ProjectState::parse(&row.value::<String>("state")?)
            .ok_or_else(|| invalid("state"))?,
        machine_id: row.uuid("machine_id")?,
        path: row.value("path")?,
        mode: ProjectMode::parse(&row.value::<String>("mode")?).ok_or_else(|| invalid("mode"))?,
        group: row.value("project_group")?,
        created_at: row.time("created_at")?,
        updated_at: row.time("updated_at")?,
        look: ProjectLook {
            position: row.value("position")?,
            emoji: row.value("emoji")?,
            image: row.value::<Option<String>>("image_type")?.is_some(),
            icon_at: row.opt_time("icon_at")?,
        },
    })
}

#[async_trait]
impl ProjectRepository for DbProjectRepository {
    async fn list(&self, state: Option<ProjectState>) -> Result<Vec<Project>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE $1 IS NULL OR p.state = $1 \
             ORDER BY CASE WHEN l.position IS NULL THEN 1 ELSE 0 END, l.position, p.name"
        )))
        .bind(state.map(ProjectState::as_str))
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_project)
        .collect()
    }

    async fn list_for_machine(&self, machine: Uuid) -> Result<Vec<Project>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE p.machine_id = $1 ORDER BY p.created_at, p.id"
        )))
        .bind(machine.to_string())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_project)
        .collect()
    }

    async fn get(&self, id: Uuid) -> Result<Option<Project>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE p.id = $1"
        )))
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_project)
        .transpose()
    }

    async fn find_active(&self, name: &ProjectName) -> Result<Vec<Project>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE p.name = $1 AND p.state = $2 \
             ORDER BY p.created_at, p.id"
        )))
        .bind(name.as_str().to_owned())
        .bind(ProjectState::Active.as_str())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_project)
        .collect()
    }

    async fn find_folder(&self, machine: Uuid, path: &str) -> Result<Vec<Project>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE p.machine_id = $1 AND p.path = $2 \
             ORDER BY p.created_at, p.id"
        )))
        .bind(machine.to_string())
        .bind(path.to_owned())
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_project)
        .collect()
    }

    async fn insert(&self, project: &Project) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO projects (id, name, description, state, machine_id, path, mode, \
             project_group, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(project.id.to_string())
        .bind(project.name.as_str().to_owned())
        .bind(project.description.clone())
        .bind(project.state.as_str())
        .bind(project.machine_id.to_string())
        .bind(project.path.clone())
        .bind(project.mode.as_str())
        .bind(project.group.clone())
        .bind(to_unix_micros(project.created_at))
        .bind(to_unix_micros(project.updated_at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn update(&self, project: &Project) -> Result<(), RepoError> {
        let result = sqlx::query(
            "UPDATE projects SET name = $2, description = $3, state = $4, mode = $5, \
             project_group = $6, updated_at = $7 WHERE id = $1",
        )
        .bind(project.id.to_string())
        .bind(project.name.as_str().to_owned())
        .bind(project.description.clone())
        .bind(project.state.as_str())
        .bind(project.mode.as_str())
        .bind(project.group.clone())
        .bind(to_unix_micros(project.updated_at))
        .execute(&self.db)
        .await?;
        changed_one(
            result.rows_affected(),
            format_args!("project {}", project.id),
        )
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepoError> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn set_order(&self, ids: &[Uuid]) -> Result<(), RepoError> {
        let mut transaction = self.db.begin().await?;
        // Those not named fall back to after the ordered ones, by name.
        sqlx::query("UPDATE project_looks SET position = NULL")
            .execute(&mut *transaction)
            .await?;
        for (position, id) in ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO project_looks (project_id, position) VALUES ($1, $2) \
                 ON CONFLICT (project_id) DO UPDATE SET position = excluded.position",
            )
            .bind(id.to_string())
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn set_icon(
        &self,
        id: Uuid,
        icon: Option<&ProjectIcon>,
        at: time::OffsetDateTime,
    ) -> Result<(), RepoError> {
        let (emoji, image_type, image) = match icon {
            None => (None, None, None),
            Some(ProjectIcon::Emoji(emoji)) => (Some(emoji.clone()), None, None),
            Some(ProjectIcon::Image {
                content_type,
                bytes,
            }) => (None, Some(content_type.clone()), Some(BASE64.encode(bytes))),
        };
        sqlx::query(
            "INSERT INTO project_looks (project_id, emoji, image_type, image, icon_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (project_id) DO UPDATE SET emoji = excluded.emoji, \
             image_type = excluded.image_type, image = excluded.image, icon_at = excluded.icon_at",
        )
        .bind(id.to_string())
        .bind(emoji)
        .bind(image_type)
        .bind(image)
        .bind(to_unix_micros(at))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn icon_image(&self, id: Uuid) -> Result<Option<(String, Vec<u8>)>, RepoError> {
        let row = sqlx::query(
            "SELECT image_type, image FROM project_looks \
             WHERE project_id = $1 AND image IS NOT NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let content_type: String = row.value("image_type")?;
        let bytes = BASE64
            .decode(row.value::<String>("image")?)
            .map_err(|_| RepoError::InvalidData(format!("project {id}: its image")))?;
        Ok(Some((content_type, bytes)))
    }
}
