//! Storing notifications and the server's settings for them.

use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::notifications::notification::model::{Level, Notification, Settings};
use crate::shared::database::{Db, RowExt, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

/// The format of `notifications.target`.
const TARGET_FORMAT: i32 = 1;
/// The format of `notification_settings.kinds`.
const KINDS_FORMAT: i32 = 1;

const COLUMNS: &str = "id, kind, source, level, title, body, project_id, target, target_format, \
                       group_key, created_at, read_at";

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Keeps `notification`; an unread one of its group is replaced.
    async fn add(&self, notification: &Notification) -> Result<(), RepoError>;
    async fn get(&self, id: Uuid) -> Result<Option<Notification>, RepoError>;
    /// Newest first: those created before `before` (all when `None`), unread
    /// only when asked, at most `limit`.
    async fn list(
        &self,
        unread_only: bool,
        before: Option<OffsetDateTime>,
        limit: u32,
    ) -> Result<Vec<Notification>, RepoError>;
    async fn unread(&self) -> Result<u64, RepoError>;
    /// The newest of a group, read or not.
    async fn latest_in_group(&self, group: &str) -> Result<Option<Notification>, RepoError>;
    /// Marks a group's unread ones read (what they told is over); how many.
    async fn read_group(&self, group: &str, at: OffsetDateTime) -> Result<u64, RepoError>;
    /// Marks one read; whether it was unread.
    async fn mark_read(&self, id: Uuid, at: OffsetDateTime) -> Result<bool, RepoError>;
    /// Marks every unread one read, of a project when given; how many.
    async fn mark_all_read(
        &self,
        project: Option<Uuid>,
        at: OffsetDateTime,
    ) -> Result<u64, RepoError>;
    /// Deletes read notifications read before `before` (all read ones when
    /// `None`); how many.
    async fn delete_read(&self, before: Option<OffsetDateTime>) -> Result<u64, RepoError>;
    /// The stored settings; `None` until changed.
    async fn settings(&self) -> Result<Option<Settings>, RepoError>;
    async fn set_settings(&self, settings: &Settings, at: OffsetDateTime) -> Result<(), RepoError>;
}

pub struct DbNotificationRepository {
    db: Db,
}

impl DbNotificationRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn read(row: &AnyRow) -> Result<Notification, RepoError> {
    let format = row.format("target")?;
    if format > TARGET_FORMAT {
        return Err(unknown_format("target", format));
    }
    let level: String = row.value("level")?;
    Ok(Notification {
        id: row.uuid("id")?,
        kind: row.value("kind")?,
        source: row.value("source")?,
        level: Level::parse(&level),
        title: row.value("title")?,
        body: row.value("body")?,
        project_id: row.opt_uuid("project_id")?,
        target: row.json("target")?,
        group_key: row.value("group_key")?,
        created_at: row.time("created_at")?,
        read_at: row.opt_time("read_at")?,
    })
}

#[async_trait]
impl NotificationRepository for DbNotificationRepository {
    async fn add(&self, notification: &Notification) -> Result<(), RepoError> {
        let mut tx = self.db.begin().await?;
        if let Some(group) = &notification.group_key {
            sqlx::query("DELETE FROM notifications WHERE group_key = $1 AND read_at IS NULL")
                .bind(group.clone())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO notifications (id, kind, source, level, title, body, project_id, \
             target, target_format, group_key, created_at, read_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NULL)",
        )
        .bind(notification.id.to_string())
        .bind(notification.kind.clone())
        .bind(notification.source.clone())
        .bind(notification.level.as_str())
        .bind(notification.title.clone())
        .bind(notification.body.clone())
        .bind(notification.project_id.map(|id| id.to_string()))
        .bind(json_text(&notification.target)?)
        .bind(TARGET_FORMAT)
        .bind(notification.group_key.clone())
        .bind(to_unix_micros(notification.created_at))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Notification>, RepoError> {
        let sql = format!("SELECT {COLUMNS} FROM notifications WHERE id = $1");
        sqlx::query(crate::shared::database::assembled(sql))
            .bind(id.to_string())
            .fetch_optional(&self.db)
            .await?
            .as_ref()
            .map(read)
            .transpose()
    }

    async fn list(
        &self,
        unread_only: bool,
        before: Option<OffsetDateTime>,
        limit: u32,
    ) -> Result<Vec<Notification>, RepoError> {
        let mut sql = format!("SELECT {COLUMNS} FROM notifications WHERE created_at < $1");
        if unread_only {
            sql.push_str(" AND read_at IS NULL");
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT $2");
        sqlx::query(crate::shared::database::assembled(sql))
            .bind(before.map_or(i64::MAX, to_unix_micros))
            .bind(i64::from(limit))
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(read)
            .collect()
    }

    async fn unread(&self) -> Result<u64, RepoError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE read_at IS NULL")
                .fetch_one(&self.db)
                .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    async fn latest_in_group(&self, group: &str) -> Result<Option<Notification>, RepoError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM notifications WHERE group_key = $1 \
             ORDER BY created_at DESC, id DESC LIMIT 1"
        );
        sqlx::query(crate::shared::database::assembled(sql))
            .bind(group.to_owned())
            .fetch_optional(&self.db)
            .await?
            .as_ref()
            .map(read)
            .transpose()
    }

    async fn read_group(&self, group: &str, at: OffsetDateTime) -> Result<u64, RepoError> {
        let done = sqlx::query(
            "UPDATE notifications SET read_at = $1 WHERE group_key = $2 AND read_at IS NULL",
        )
        .bind(to_unix_micros(at))
        .bind(group.to_owned())
        .execute(&self.db)
        .await?;
        Ok(done.rows_affected())
    }

    async fn mark_read(&self, id: Uuid, at: OffsetDateTime) -> Result<bool, RepoError> {
        let done =
            sqlx::query("UPDATE notifications SET read_at = $1 WHERE id = $2 AND read_at IS NULL")
                .bind(to_unix_micros(at))
                .bind(id.to_string())
                .execute(&self.db)
                .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn mark_all_read(
        &self,
        project: Option<Uuid>,
        at: OffsetDateTime,
    ) -> Result<u64, RepoError> {
        let done = match project {
            Some(project) => sqlx::query(
                "UPDATE notifications SET read_at = $1 WHERE read_at IS NULL AND project_id = $2",
            )
            .bind(to_unix_micros(at))
            .bind(project.to_string())
            .execute(&self.db)
            .await?,
            None => {
                sqlx::query("UPDATE notifications SET read_at = $1 WHERE read_at IS NULL")
                    .bind(to_unix_micros(at))
                    .execute(&self.db)
                    .await?
            }
        };
        Ok(done.rows_affected())
    }

    async fn delete_read(&self, before: Option<OffsetDateTime>) -> Result<u64, RepoError> {
        let done =
            sqlx::query("DELETE FROM notifications WHERE read_at IS NOT NULL AND read_at < $1")
                .bind(before.map_or(i64::MAX, to_unix_micros))
                .execute(&self.db)
                .await?;
        Ok(done.rows_affected())
    }

    async fn settings(&self) -> Result<Option<Settings>, RepoError> {
        let Some(row) = sqlx::query(
            "SELECT CAST(enabled AS INTEGER) AS enabled, keep_days, kinds, kinds_format, \
             CAST(outside AS INTEGER) AS outside FROM notification_settings WHERE id = 1",
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let format = row.format("kinds")?;
        // Kinds a newer Ahab wrote are left alone: every kind stays on.
        let kinds: BTreeMap<String, bool> = if format > KINDS_FORMAT {
            BTreeMap::new()
        } else {
            row.json("kinds")?
        };
        let keep_days: Option<i64> = row.value("keep_days")?;
        let inside_only = sqlx::query("SELECT kind FROM notification_kinds_inside")
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(|row| row.value::<String>("kind"))
            .collect::<Result<_, _>>()?;
        Ok(Some(Settings {
            enabled: row.flag("enabled")?,
            keep_days: keep_days.and_then(|days| u32::try_from(days).ok()),
            kinds,
            outside: row.flag("outside")?,
            inside_only,
        }))
    }

    async fn set_settings(&self, settings: &Settings, at: OffsetDateTime) -> Result<(), RepoError> {
        let mut transaction = self.db.begin().await?;
        sqlx::query("DELETE FROM notification_kinds_inside")
            .execute(&mut *transaction)
            .await?;
        for kind in &settings.inside_only {
            sqlx::query("INSERT INTO notification_kinds_inside (kind) VALUES ($1)")
                .bind(kind.clone())
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            "INSERT INTO notification_settings (id, enabled, keep_days, kinds, kinds_format, \
             outside, updated_at) VALUES (1, $1, $2, $3, $4, $5, $6) \
             ON CONFLICT (id) DO UPDATE SET enabled = excluded.enabled, \
             keep_days = excluded.keep_days, kinds = excluded.kinds, \
             kinds_format = excluded.kinds_format, outside = excluded.outside, \
             updated_at = excluded.updated_at",
        )
        .bind(settings.enabled)
        .bind(settings.keep_days.map(i64::from))
        .bind(json_text(&settings.kinds)?)
        .bind(KINDS_FORMAT)
        .bind(settings.outside)
        .bind(to_unix_micros(at))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}
