//! The database, through sqlx's `Any` driver: SQLite by default, Postgres
//! when `DATABASE_URL` says so. The backend is chosen at runtime from the URL,
//! so switching is a configuration change. Repositories keep sqlx behind their
//! traits, and the schema and queries stay in the SQL both backends share
//! (see [`crate::shared::database`]).
//!
//! The schema is in `crates/ahab/migrations/`, one SQL file per migration,
//! oldest first, each valid on SQLite and Postgres. The baseline has a file
//! per module, in the order the modules use each other. Every statement is
//! `IF NOT EXISTS`, so a migration that stopped half-way can run again. A
//! migration never changes once it is released; later changes are new
//! migrations.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::{ConnectOptions, Connection, Executor};

use crate::shared::database::Db;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Connects to the database, with at most `connections` at once. A SQLite
/// file and its folder are created when missing.
pub async fn connect(database_url: &str, connections: u32) -> Result<Db, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let sqlite = sqlite_path(database_url);
    if let Some(path) = sqlite
        && let Some(folder) = Path::new(path).parent()
        && !folder.as_os_str().is_empty()
    {
        std::fs::create_dir_all(folder)?;
    }
    let options = AnyConnectOptions::from_str(database_url)?.disable_statement_logging();
    let is_sqlite = database_url.starts_with("sqlite:");
    let db = AnyPoolOptions::new()
        .max_connections(connections)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |connection, _| {
            Box::pin(async move {
                if is_sqlite {
                    // With WAL, NORMAL is still safe from corruption and skips
                    // a flush to disk on every commit (agents' events arrive
                    // many at a time). sqlx already enables foreign keys and a
                    // busy timeout on every connection.
                    connection
                        .execute(sqlx::raw_sql("PRAGMA synchronous = NORMAL"))
                        .await?;
                }
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    if is_sqlite {
        // Readers do not block the writer. The mode is stored in the file, so
        // setting it once is enough.
        db.execute(sqlx::raw_sql("PRAGMA journal_mode = WAL"))
            .await?;
    }
    Ok(db)
}

/// Applies the migrations that have not run yet.
pub async fn migrate(db: &Db) -> Result<(), MigrateError> {
    MIGRATOR.run(db).await
}

/// The backend `database_url` names, for logs: `sqlite`, `postgres`.
pub fn backend(database_url: &str) -> &str {
    database_url.split(':').next().unwrap_or_default()
}

/// Runs `sql`, such as `CREATE DATABASE`, on a connection of its own.
pub async fn execute_once(database_url: &str, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::any::install_default_drivers();
    let mut connection = sqlx::AnyConnection::connect(database_url).await?;
    connection
        .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned())))
        .await?;
    connection.close().await
}

/// The file a `sqlite:` URL names, or `None` for other databases and for an
/// in-memory SQLite.
fn sqlite_path(database_url: &str) -> Option<&str> {
    let rest = database_url.strip_prefix("sqlite:")?;
    let path = rest.trim_start_matches("//");
    let path = path.split('?').next().unwrap_or_default();
    (!path.is_empty() && path != ":memory:").then_some(path)
}

#[cfg(test)]
mod tests {
    use super::{backend, sqlite_path};

    #[test]
    fn finds_the_file_of_a_sqlite_url() {
        assert_eq!(
            sqlite_path("sqlite://data/ahab.db?mode=rwc"),
            Some("data/ahab.db")
        );
        assert_eq!(
            sqlite_path("sqlite:///var/lib/ahab/ahab.db"),
            Some("/var/lib/ahab/ahab.db")
        );
        assert_eq!(sqlite_path("sqlite::memory:"), None);
        assert_eq!(sqlite_path("postgres://ahab@localhost/ahab"), None);
    }

    #[test]
    fn names_the_backend() {
        assert_eq!(backend("sqlite://data/ahab.db?mode=rwc"), "sqlite");
        assert_eq!(backend("postgres://ahab@localhost/ahab"), "postgres");
    }
}
