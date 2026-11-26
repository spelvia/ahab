//! A fresh database for each test: a SQLite file in a temporary folder by
//! default, or a new Postgres database when `AHAB_TEST_POSTGRES_URL` names a
//! server (CI and `make test-postgres`). The tests never read `DATABASE_URL`,
//! so they cannot reach the development database.
//!
//! Each Postgres database is dropped when its test ends, also when the test
//! panics: each is a full copy of `template1` (about 8 MB), so keeping them
//! until the end of the run filled the test server's memory. A database a
//! killed test leaves behind goes with the server, which
//! `make test-postgres` recreates for each run.
//!
//! `DROP DATABASE` waits for every connection to the server to acknowledge
//! it, also one still logging in. The test blocks its runtime while
//! dropping, so a connection its pool was opening could never answer a
//! password challenge: the test server trusts local connections instead
//! (`docker/compose.yaml`, and the CI service).

use tempfile::TempDir;

use ahab::modules::platform::database::store as database;
use ahab::shared::database::Db;

/// A fresh database, and whatever must live as long as it.
pub struct TestDatabase {
    pub db: Db,
    /// Its address, for code that connects by itself (`start`).
    pub url: String,
    _folder: Option<TempDir>,
    /// The Postgres server and the database's name, to drop it at the end.
    postgres: Option<(String, String)>,
}

impl TestDatabase {
    /// A fresh, migrated database.
    pub async fn new() -> Self {
        let database = Self::empty().await;
        database::migrate(&database.db).await.unwrap();
        database
    }

    /// A fresh database without even the migrations.
    pub async fn empty() -> Self {
        match std::env::var("AHAB_TEST_POSTGRES_URL") {
            Ok(server) if !server.is_empty() => Self::postgres(&server).await,
            _ => Self::sqlite().await,
        }
    }

    async fn sqlite() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}?mode=rwc",
            folder.path().join("ahab.db").display()
        );
        let db = database::connect(&url, 4).await.unwrap();
        Self {
            db,
            url,
            _folder: Some(folder),
            postgres: None,
        }
    }

    /// Creates a uniquely named database on the server, then connects to it.
    async fn postgres(server: &str) -> Self {
        let name = format!("ahab_test_{}", uuid::Uuid::new_v4().simple());
        database::execute_once(server, &format!("CREATE DATABASE \"{name}\""))
            .await
            .unwrap();
        let url = with_database(server, &name);
        let db = database::connect(&url, 4).await.unwrap();
        Self {
            db,
            url,
            _folder: None,
            postgres: Some((server.to_owned(), name)),
        }
    }
}

impl Drop for TestDatabase {
    /// Drops a Postgres database when its test ends, also when it panics.
    fn drop(&mut self) {
        let Some((server, name)) = self.postgres.take() else {
            return;
        };
        // Best effort, on a thread of its own: the test's runtime may be
        // the one running this, and may be shutting down.
        let dropped = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            let sql = format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)");
            runtime
                .block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        database::execute_once(&server, &sql),
                    )
                    .await
                })
                .ok()?
                .ok()
        });
        let _ = dropped.join();
    }
}

/// `server` with its database name replaced by `name`, keeping any query.
fn with_database(server: &str, name: &str) -> String {
    let (base, query) = server
        .split_once('?')
        .map_or((server, None), |(b, q)| (b, Some(q)));
    let authority_end = base.find("://").map_or(0, |i| i + 3);
    let base = match base[authority_end..].find('/') {
        Some(slash) => &base[..authority_end + slash],
        None => base,
    };
    match query {
        Some(query) => format!("{base}/{name}?{query}"),
        None => format!("{base}/{name}"),
    }
}

#[test]
fn replaces_the_database_name() {
    assert_eq!(
        with_database("postgres://ahab:ahab@127.0.0.1:55443/ahab", "t1"),
        "postgres://ahab:ahab@127.0.0.1:55443/t1"
    );
    assert_eq!(
        with_database("postgres://ahab@db:5432?sslmode=disable", "t1"),
        "postgres://ahab@db:5432/t1?sslmode=disable"
    );
}
