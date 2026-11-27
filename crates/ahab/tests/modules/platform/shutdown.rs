//! Stopping: open event streams never end on their own, so a stop must not
//! wait for them.

use std::time::{Duration, Instant};

use ahab::server::start::{build_state_with, listen};
use ahab::server::web::WebApp;
use ahab::shared::time::FixedClock;
use time::macros::datetime;
use tokio::sync::oneshot;

use crate::support::TestDatabase;

#[tokio::test]
async fn a_stop_does_not_wait_for_open_event_streams() {
    let database = TestDatabase::new().await;
    let plugins = tempfile::tempdir().unwrap();
    let state = build_state_with(
        database.db.clone(),
        std::sync::Arc::new(FixedClock::at(datetime!(2026-11-13 9:00 UTC))),
        ahab::modules::machines::MachineKey::from_secret([200; 32]),
        ahab::modules::plugins::PluginPaths {
            store: plugins.path().join("store"),
            node: None,
        },
    );
    state
        .clients
        .ensure_local(&plugins.path().join("credential"))
        .await
        .unwrap();
    let credential = std::fs::read_to_string(plugins.path().join("credential")).unwrap();
    let started = listen(state, "127.0.0.1:0".parse().unwrap(), &WebApp::Off)
        .await
        .unwrap();
    let url = format!("http://{}/api/v1/plugin-events", started.addr);
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(started.run(async move {
        let _ = stopped.await;
    }));

    let stream = reqwest::Client::new()
        .get(url)
        .bearer_auth(credential.trim())
        .send()
        .await
        .unwrap();
    assert!(stream.status().is_success());

    let asked = Instant::now();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(15), server)
        .await
        .expect("the server stops while a stream is open")
        .unwrap()
        .unwrap();
    assert!(asked.elapsed() < Duration::from_secs(10));
    drop(stream);
}
