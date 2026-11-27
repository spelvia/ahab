//! Storing browsers' push subscriptions.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::notifications::push::model::{Subscription, VapidKey, encode};
use crate::shared::database::{Db, RowExt, json_text, unknown_format};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

/// The format of `push_subscriptions.keys`.
const KEYS_FORMAT: i32 = 1;

const COLUMNS: &str = "id, device_id, endpoint, keys, keys_format, label, created_at, last_used_at";

#[derive(Serialize, Deserialize)]
struct Keys {
    p256dh: String,
    auth: String,
}

#[async_trait]
pub trait PushRepository: Send + Sync {
    /// Keeps `subscription`; one with the same endpoint is replaced (a
    /// browser subscribing again, or for another client).
    async fn add(&self, subscription: &Subscription) -> Result<(), RepoError>;
    async fn get(&self, id: Uuid) -> Result<Option<Subscription>, RepoError>;
    async fn list(&self) -> Result<Vec<Subscription>, RepoError>;
    async fn remove(&self, id: Uuid) -> Result<bool, RepoError>;
    async fn used(&self, id: Uuid, at: OffsetDateTime) -> Result<(), RepoError>;
}

pub struct DbPushRepository {
    db: Db,
}

impl DbPushRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn read(row: &AnyRow) -> Result<Subscription, RepoError> {
    let format = row.format("keys")?;
    if format > KEYS_FORMAT {
        return Err(unknown_format("keys", format));
    }
    let keys: Keys = row.json("keys")?;
    let bytes = |text: &str| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(text)
            .map_err(|error| RepoError::InvalidData(format!("push subscription keys: {error}")))
    };
    Ok(Subscription {
        id: row.uuid("id")?,
        client_id: row.uuid("device_id")?,
        endpoint: row.value("endpoint")?,
        p256dh: bytes(&keys.p256dh)?,
        auth: bytes(&keys.auth)?,
        label: row.value("label")?,
        created_at: row.time("created_at")?,
        last_used_at: row.opt_time("last_used_at")?,
    })
}

#[async_trait]
impl PushRepository for DbPushRepository {
    async fn add(&self, subscription: &Subscription) -> Result<(), RepoError> {
        let keys = Keys {
            p256dh: encode(&subscription.p256dh),
            auth: encode(&subscription.auth),
        };
        let mut tx = self.db.begin().await?;
        sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = $1")
            .bind(subscription.endpoint.clone())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO push_subscriptions (id, device_id, endpoint, keys, keys_format, label, \
             created_at, last_used_at) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)",
        )
        .bind(subscription.id.to_string())
        .bind(subscription.client_id.to_string())
        .bind(subscription.endpoint.clone())
        .bind(json_text(&keys)?)
        .bind(KEYS_FORMAT)
        .bind(subscription.label.clone())
        .bind(to_unix_micros(subscription.created_at))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Subscription>, RepoError> {
        let sql = format!("SELECT {COLUMNS} FROM push_subscriptions WHERE id = $1");
        sqlx::query(crate::shared::database::assembled(sql))
            .bind(id.to_string())
            .fetch_optional(&self.db)
            .await?
            .as_ref()
            .map(read)
            .transpose()
    }

    async fn list(&self) -> Result<Vec<Subscription>, RepoError> {
        let sql = format!("SELECT {COLUMNS} FROM push_subscriptions ORDER BY created_at, id");
        sqlx::query(crate::shared::database::assembled(sql))
            .fetch_all(&self.db)
            .await?
            .iter()
            .map(read)
            .collect()
    }

    async fn remove(&self, id: Uuid) -> Result<bool, RepoError> {
        let done = sqlx::query("DELETE FROM push_subscriptions WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn used(&self, id: Uuid, at: OffsetDateTime) -> Result<(), RepoError> {
        sqlx::query("UPDATE push_subscriptions SET last_used_at = $1 WHERE id = $2")
            .bind(to_unix_micros(at))
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        Ok(())
    }
}

/// The first word of the key file.
const KEY_HEADER: &str = "ahab-push-key-pkcs8";

/// The push key file could not be read or written.
#[derive(Debug, thiserror::Error)]
#[error("the push key {path} cannot be used: {reason}")]
pub struct KeyFileError {
    pub path: String,
    pub reason: String,
}

/// The server's VAPID key at `path`, made on first use (owner-only, beside
/// `server.key`): browsers' subscriptions are tied to it, so it is kept.
pub fn load_or_create_key(path: &Path) -> Result<VapidKey, KeyFileError> {
    let failed = |reason: String| KeyFileError {
        path: path.display().to_string(),
        reason,
    };
    match fs::read_to_string(path) {
        Ok(text) => {
            let encoded = text
                .trim()
                .strip_prefix(KEY_HEADER)
                .ok_or_else(|| failed("not an Ahab push key".to_owned()))?;
            let pkcs8 = STANDARD
                .decode(encoded.trim())
                .map_err(|error| failed(error.to_string()))?;
            VapidKey::from_pkcs8(&pkcs8).map_err(|error| failed(error.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = VapidKey::generate().map_err(|error| failed(error.to_string()))?;
            if let Some(folder) = path.parent() {
                fs::create_dir_all(folder).map_err(|error| failed(error.to_string()))?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(path)
                .map_err(|error| failed(error.to_string()))?;
            writeln!(file, "{KEY_HEADER} {}", STANDARD.encode(key.pkcs8()))
                .and_then(|()| file.sync_all())
                .map_err(|error| failed(error.to_string()))?;
            Ok(key)
        }
        Err(error) => Err(failed(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_key_is_made_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/push.key");
        let made = load_or_create_key(&path).unwrap();
        assert_eq!(load_or_create_key(&path).unwrap().public(), made.public());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(&path, "nonsense").unwrap();
        assert!(load_or_create_key(&path).is_err());
    }
}
