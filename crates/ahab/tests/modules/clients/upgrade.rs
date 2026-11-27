//! Upgrading a server from before clients: its database gains the new tables,
//! and at its first start it
//! writes the local credential and prints a first-run link, as at every
//! start until a client is paired.

use std::path::{Path, PathBuf};

use ahab::config::{LogFormat, Paths, Settings};
use ahab::modules::machines::machine::model::{Machine, MachineName, PublicKey};
use ahab::modules::machines::machine::store::{DbMachineRepository, MachineRepository};
use ahab::server::start::start;
use ahab::shared::database::Db;
use ahab::shared::net::LOOPBACK;
use reqwest::StatusCode;
use serde_json::json;

use crate::support::TestDatabase;

/// The migrations a server before clients had.
fn old_migrations(into: &Path) {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.as_str() < "20261117000006" {
            std::fs::copy(&path, into.join(&name)).unwrap();
        }
    }
}

fn settings(dir: &Path, database_url: &str) -> Settings {
    Settings {
        paths: Paths {
            config_dir: dir.join("config"),
            data_dir: dir.join("data"),
        },
        daemon: false,
        web: false,
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        allowed_hosts: Vec::new(),
        public_url: Some("https://ahab.example".to_owned()),
        allowed_origins: Vec::new(),
        trusted_proxies: LOOPBACK.to_vec(),
        allow_insecure_http: false,
        database_url: database_url.to_owned(),
        ui_dir: None,
        log_format: LogFormat::Text,
        joined: None,
        plugin_node: None,
        // Tests never ask GitHub.
        update: ahab::config::UpdateCheck {
            enabled: false,
            ..ahab::config::UpdateCheck::default()
        },
    }
}

async fn open_first_run_links(db: &Db) -> (i64, i64) {
    let row = sqlx::query(
        "SELECT \
         SUM(CASE WHEN state = 'open' THEN 1 ELSE 0 END) AS open, COUNT(*) AS total \
         FROM pairings WHERE created_by IS NULL",
    )
    .fetch_one(db)
    .await
    .unwrap();
    (
        sqlx::Row::try_get::<i64, _>(&row, "open").unwrap_or(0),
        sqlx::Row::get::<i64, _>(&row, "total"),
    )
}

#[tokio::test]
async fn an_older_server_gains_clients_a_local_credential_and_a_first_run_link() {
    let database = TestDatabase::empty().await;
    let old = tempfile::tempdir().unwrap();
    old_migrations(old.path());
    sqlx::migrate::Migrator::new(old.path())
        .await
        .unwrap()
        .run(&database.db)
        .await
        .unwrap();
    // What the old server kept.
    let machine = Machine::joined(
        MachineName::parse("box").unwrap(),
        PublicKey::from_bytes(&[7; 32]).unwrap(),
        time::OffsetDateTime::now_utc(),
    );
    DbMachineRepository::new(database.db.clone())
        .insert(&machine)
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path(), &database.url);
    let started = start(&settings).await.unwrap();
    let addr = started.addr;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(started.run(async move {
        let _ = stopped.await;
    }));

    // The local credential, readable only by its owner.
    let file = dir.path().join("data/credential");
    let credential = std::fs::read_to_string(&file).unwrap().trim().to_owned();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(open_first_run_links(&database.db).await, (1, 1));

    // The command line on this computer keeps working; the old data is
    // there; a browser is not paired.
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();
    let machines = http
        .get(format!("{base}/api/v1/machines"))
        .bearer_auth(&credential)
        .send()
        .await
        .unwrap();
    assert_eq!(machines.status(), StatusCode::OK);
    let body: serde_json::Value = machines.json().await.unwrap();
    assert_eq!(body["items"][0]["name"], "box");
    let browser = http
        .get(format!("{base}/api/v1/machines"))
        .send()
        .await
        .unwrap();
    assert_eq!(browser.status(), StatusCode::UNAUTHORIZED);

    // public_url's host is answered.
    let named = http
        .get(format!("{base}/api/v1/health"))
        .header("host", "ahab.example")
        .send()
        .await
        .unwrap();
    assert_eq!(named.status(), StatusCode::OK);
    // Links point there.
    let link = http
        .post(format!("{base}/api/v1/pairings"))
        .bearer_auth(&credential)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert!(
        link["url"]
            .as_str()
            .unwrap()
            .starts_with("https://ahab.example/pair#")
    );
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();

    // The next start keeps the credential and prints a fresh first-run
    // link; the earlier one stops working.
    let again = start(&settings).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap().trim(),
        credential.as_str()
    );
    assert_eq!(open_first_run_links(&database.db).await, (1, 2));
    drop(again);
}
