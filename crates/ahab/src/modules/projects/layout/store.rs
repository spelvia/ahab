//! Storing layouts. The service sees only [`LayoutRepository`].

use async_trait::async_trait;
use uuid::Uuid;

use crate::modules::projects::layout::model::{ClientClass, Layout};
use crate::shared::database::{Db, RowExt, json_text};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait LayoutRepository: Send + Sync {
    async fn get(&self, project: Uuid, class: ClientClass) -> Result<Option<Layout>, RepoError>;
    /// Inserts or replaces the layout if its preceding revision still matches.
    async fn save(&self, layout: &Layout) -> Result<bool, RepoError>;
}

/// The format of the layouts this Ahab writes (`project_layouts.layout_format`
/// and `previous_format`).
pub const LAYOUT_FORMAT: i32 = 1;

pub struct DbLayoutRepository {
    db: Db,
}

impl DbLayoutRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl LayoutRepository for DbLayoutRepository {
    async fn get(&self, project: Uuid, class: ClientClass) -> Result<Option<Layout>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT project_id, layout, layout_format, previous, previous_format, revision, \
             updated_at FROM project_layouts WHERE project_id = $1 AND client_class = $2",
        )
        .bind(project.to_string())
        .bind(class.as_str())
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        // Written by a newer Ahab: none, so the next save starts afresh.
        if row.format("layout")? != LAYOUT_FORMAT {
            return Ok(None);
        }
        let previous = match row.format("previous")? {
            LAYOUT_FORMAT => row
                .value::<Option<String>>("previous")?
                .map(|text| serde_json::from_str(&text))
                .transpose()
                .map_err(|error| RepoError::InvalidData(format!("previous: {error}")))?,
            _ => None,
        };
        Ok(Some(Layout {
            project_id: row.uuid("project_id")?,
            client_class: class,
            layout: row.json("layout")?,
            previous,
            revision: row.value("revision")?,
            updated_at: row.time("updated_at")?,
        }))
    }

    async fn save(&self, layout: &Layout) -> Result<bool, RepoError> {
        let text = json_text(&layout.layout)?;
        let previous = layout.previous.as_ref().map(json_text).transpose()?;
        // The first revision is new, or replaces a layout in a format this
        // Ahab does not read (which `get` reports as none).
        let sql = if layout.revision == 1 {
            "INSERT INTO project_layouts (project_id, client_class, layout, previous, \
             revision, updated_at, layout_format, previous_format) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7) \
             ON CONFLICT (project_id, client_class) DO UPDATE SET layout = excluded.layout, \
             previous = excluded.previous, revision = excluded.revision, \
             updated_at = excluded.updated_at, layout_format = excluded.layout_format, \
             previous_format = excluded.previous_format \
             WHERE project_layouts.layout_format <> $7"
        } else {
            "UPDATE project_layouts SET layout = $3, previous = $4, revision = $5, \
             updated_at = $6, layout_format = $7, previous_format = $7 \
             WHERE project_id = $1 AND client_class = $2 AND layout_format = $7 AND revision = $8"
        };
        let query = sqlx::query(sql)
            .bind(layout.project_id.to_string())
            .bind(layout.client_class.as_str())
            .bind(text)
            .bind(previous)
            .bind(layout.revision)
            .bind(to_unix_micros(layout.updated_at))
            .bind(LAYOUT_FORMAT);
        let query = if layout.revision == 1 {
            query
        } else {
            query.bind(layout.revision - 1)
        };
        Ok(query.execute(&self.db).await?.rows_affected() == 1)
    }
}
