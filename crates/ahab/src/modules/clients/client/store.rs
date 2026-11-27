//! Storing clients, and the local credential file: `credential` in the data
//! folder, readable only by its owner.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sqlx::any::AnyRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::clients::client::model::{Client, ClientKind, ClientName, PairedVia, Seen};
use crate::shared::database::{Db, RowExt, assembled, changed_one};
use crate::shared::error::RepoError;
use crate::shared::time::to_unix_micros;

#[async_trait]
pub trait ClientRepository: Send + Sync {
    /// The clients that are not revoked, oldest first.
    async fn list_active(&self) -> Result<Vec<Client>, RepoError>;
    async fn get(&self, id: Uuid) -> Result<Option<Client>, RepoError>;
    async fn find_by_credential(&self, hash: &[u8; 32]) -> Result<Option<Client>, RepoError>;
    async fn insert(&self, client: &Client) -> Result<(), RepoError>;
    async fn rename(&self, id: Uuid, name: &ClientName) -> Result<(), RepoError>;
    /// Records a request: when, from where, and whether it was encrypted.
    async fn touch(&self, id: Uuid, at: OffsetDateTime, seen: &Seen) -> Result<(), RepoError>;
    /// Revokes a client; `false` when it was revoked already or is unknown.
    async fn revoke(&self, id: Uuid, at: OffsetDateTime) -> Result<bool, RepoError>;
    /// Deletes clients revoked before `before`.
    async fn purge_revoked(&self, before: OffsetDateTime) -> Result<u64, RepoError>;
}

const COLUMNS: &str = "id, name, kind, credential_hash, paired_by, paired_via, user_agent, \
                       created_at, last_used_at, last_address, \
                       CAST(encrypted AS INTEGER) AS encrypted, revoked_at";

pub struct DbClientRepository {
    db: Db,
}

impl DbClientRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn to_client(row: &AnyRow) -> Result<Client, RepoError> {
    let id = row.uuid("id")?;
    let invalid = |what: &str| RepoError::InvalidData(format!("client {id}: {what}"));
    let kind: String = row.value("kind")?;
    let paired_via: String = row.value("paired_via")?;
    let name: String = row.value("name")?;
    Ok(Client {
        id,
        name: ClientName::parse(&name).map_err(|_| invalid("name"))?,
        // A kind a newer Ahab added is an app here.
        kind: ClientKind::parse(&kind).unwrap_or(ClientKind::App),
        credential_hash: <[u8; 32]>::try_from(row.value::<Vec<u8>>("credential_hash")?.as_slice())
            .map_err(|_| invalid("credential hash"))?,
        paired_by: row.opt_uuid("paired_by")?,
        paired_via: PairedVia::parse(&paired_via).unwrap_or(PairedVia::Link),
        user_agent: row.value("user_agent")?,
        created_at: row.time("created_at")?,
        last_used_at: row.opt_time("last_used_at")?,
        last_address: row.value("last_address")?,
        encrypted: row.flag("encrypted")?,
        revoked_at: row.opt_time("revoked_at")?,
    })
}

#[async_trait]
impl ClientRepository for DbClientRepository {
    async fn list_active(&self) -> Result<Vec<Client>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM devices WHERE revoked_at IS NULL ORDER BY created_at, id"
        )))
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(to_client)
        .collect()
    }

    async fn get(&self, id: Uuid) -> Result<Option<Client>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM devices WHERE id = $1"
        )))
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_client)
        .transpose()
    }

    async fn find_by_credential(&self, hash: &[u8; 32]) -> Result<Option<Client>, RepoError> {
        sqlx::query(assembled(format!(
            "SELECT {COLUMNS} FROM devices WHERE credential_hash = $1"
        )))
        .bind(hash.to_vec())
        .fetch_optional(&self.db)
        .await?
        .as_ref()
        .map(to_client)
        .transpose()
    }

    async fn insert(&self, client: &Client) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO devices (id, name, kind, credential_hash, paired_by, paired_via, \
             user_agent, created_at, last_used_at, last_address, encrypted, revoked_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(client.id.to_string())
        .bind(client.name.as_str().to_owned())
        .bind(client.kind.as_str())
        .bind(client.credential_hash.to_vec())
        .bind(client.paired_by.map(|id| id.to_string()))
        .bind(client.paired_via.as_str())
        .bind(client.user_agent.clone())
        .bind(to_unix_micros(client.created_at))
        .bind(client.last_used_at.map(to_unix_micros))
        .bind(client.last_address.clone())
        .bind(client.encrypted)
        .bind(client.revoked_at.map(to_unix_micros))
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn rename(&self, id: Uuid, name: &ClientName) -> Result<(), RepoError> {
        let result = sqlx::query("UPDATE devices SET name = $2 WHERE id = $1")
            .bind(id.to_string())
            .bind(name.as_str().to_owned())
            .execute(&self.db)
            .await?;
        changed_one(result.rows_affected(), format_args!("client {id}"))
    }

    async fn touch(&self, id: Uuid, at: OffsetDateTime, seen: &Seen) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE devices SET last_used_at = $2, last_address = $3, encrypted = $4 \
             WHERE id = $1",
        )
        .bind(id.to_string())
        .bind(to_unix_micros(at))
        .bind(seen.address.clone())
        .bind(seen.encrypted)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn revoke(&self, id: Uuid, at: OffsetDateTime) -> Result<bool, RepoError> {
        let result =
            sqlx::query("UPDATE devices SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL")
                .bind(id.to_string())
                .bind(to_unix_micros(at))
                .execute(&self.db)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn purge_revoked(&self, before: OffsetDateTime) -> Result<u64, RepoError> {
        let result =
            sqlx::query("DELETE FROM devices WHERE revoked_at IS NOT NULL AND revoked_at < $1")
                .bind(to_unix_micros(before))
                .execute(&self.db)
                .await?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialFileError {
    #[error("cannot read the credential {}: {reason}", path.display())]
    Unreadable { path: PathBuf, reason: String },
    #[error("cannot write the credential {}: {reason}", path.display())]
    Unwritable { path: PathBuf, reason: String },
}

/// The credential in `path`; `None` when there is no such file.
pub fn read_credential(path: &Path) -> Result<Option<String>, CredentialFileError> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let credential = text.trim();
            Ok((!credential.is_empty()).then(|| credential.to_owned()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CredentialFileError::Unreadable {
            path: path.to_owned(),
            reason: error.to_string(),
        }),
    }
}

/// Writes `contents` to `path`, readable and writable only by its owner,
/// through a temporary file so a failed write never leaves half a file.
pub fn write_private(path: &Path, contents: &str) -> Result<(), CredentialFileError> {
    let unwritable = |error: &dyn std::fmt::Display| CredentialFileError::Unwritable {
        path: path.to_owned(),
        reason: error.to_string(),
    };
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|error| unwritable(&error))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::now_v7().simple()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = written.and_then(|()| fs::rename(&temporary, path)) {
        let _ = fs::remove_file(&temporary);
        return Err(unwritable(&error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_file_only_its_owner_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/credential");
        assert_eq!(read_credential(&path).unwrap(), None);

        write_private(&path, "ahab_one\n").unwrap();
        assert_eq!(read_credential(&path).unwrap().as_deref(), Some("ahab_one"));
        write_private(&path, "ahab_two\n").unwrap();
        assert_eq!(read_credential(&path).unwrap().as_deref(), Some("ahab_two"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(fs::read_dir(dir.path().join("data")).unwrap().count(), 1);
    }
}
