//! A real server on a random port, for tests that need a network: daemons
//! connecting over WebSocket.

use std::time::Duration;

use ahab::modules::machines::machine::store::DbMachineRepository;
use ahab::modules::machines::{Machine, MachineKey};
use ahab::server::start::{build_state_with, listen};
use ahab::server::state::AppState;
use ahab::server::web::WebApp;
use ahab::shared::time::FixedClock;
use reqwest::{Method, StatusCode};
use serde_json::Value;
use time::macros::datetime;
use tokio::sync::watch;

use super::TestDatabase;

pub struct TestServer {
    /// Such as `http://127.0.0.1:41234`.
    pub url: String,
    /// This computer's credential, which [`Self::http`] and
    /// [`Self::call`] send.
    pub credential: String,
    pub state: AppState,
    pub clock: FixedClock,
    database: TestDatabase,
    /// Where plugins are installed, removed with the test.
    plugin_dir: tempfile::TempDir,
    /// Daemons' data folders, removed with the test: a terminal host started
    /// there sees its socket gone and stops, ending its shells.
    daemons: tempfile::TempDir,
    stop: watch::Sender<bool>,
}

impl TestServer {
    pub async fn start() -> Self {
        let database = TestDatabase::new().await;
        let clock = FixedClock::at(datetime!(2026-11-13 9:00 UTC));
        let plugin_dir = tempfile::tempdir().unwrap();
        let state = build_state_with(
            database.db.clone(),
            std::sync::Arc::new(clock.clone()),
            ahab::modules::machines::MachineKey::from_secret([200; 32]),
            ahab::modules::plugins::PluginPaths {
                store: plugin_dir.path().join("store"),
                node: None,
            },
        );
        let credential_file = plugin_dir.path().join("credential");
        state.clients.ensure_local(&credential_file).await.unwrap();
        let credential = std::fs::read_to_string(&credential_file)
            .unwrap()
            .trim()
            .to_owned();
        let started = listen(state.clone(), "127.0.0.1:0".parse().unwrap(), &WebApp::Off)
            .await
            .unwrap();
        let url = format!("http://{}", started.addr);
        let (stop, mut stopped) = watch::channel(false);
        tokio::spawn(started.run(async move {
            let _ = stopped.changed().await;
        }));
        Self {
            url,
            credential,
            state,
            clock,
            database,
            plugin_dir,
            daemons: tempfile::tempdir().unwrap(),
            stop,
        }
    }

    /// Restarts the server on the same database and with the same credential.
    pub async fn restart(&mut self) {
        self.stop.send(true).unwrap();
        let state = build_state_with(
            self.database.db.clone(),
            std::sync::Arc::new(self.clock.clone()),
            MachineKey::from_secret([200; 32]),
            ahab::modules::plugins::PluginPaths {
                store: self.plugin_dir.path().join("store"),
                node: None,
            },
        );
        let started = listen(state.clone(), "127.0.0.1:0".parse().unwrap(), &WebApp::Off)
            .await
            .unwrap();
        self.url = format!("http://{}", started.addr);
        self.state = state;
        let (stop, mut stopped) = watch::channel(false);
        self.stop = stop;
        tokio::spawn(started.run(async move {
            let _ = stopped.changed().await;
        }));
    }

    /// A data folder for a daemon of this test, removed with the server.
    pub fn daemon_dir(&self) -> std::path::PathBuf {
        self.daemons
            .path()
            .join(format!("daemon-{}", uuid::Uuid::now_v7()))
    }

    pub fn machines(&self) -> DbMachineRepository {
        DbMachineRepository::new(self.database.db.clone())
    }

    pub fn chats(&self) -> ahab::modules::sessions::chat::store::DbChatRepository {
        ahab::modules::sessions::chat::store::DbChatRepository::new(self.database.db.clone())
    }

    /// An HTTP client that sends this computer's credential.
    pub fn http(&self) -> reqwest::Client {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.credential).parse().unwrap(),
        );
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap()
    }

    /// Opens a WebSocket at `url` (`ws://…`) with this computer's
    /// credential.
    pub async fn ws(
        &self,
        url: &str,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Error,
    > {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", self.credential).parse().unwrap(),
        );
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    }

    /// Calls the API (`path` after `/api/v1`): the status and the JSON body
    /// (`null` when there is none).
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self
            .http()
            .request(method, format!("{}/api/v1{path}", self.url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> (StatusCode, Value) {
        self.call(Method::DELETE, path, None).await
    }

    /// Makes `key`'s machine the server's own, named `name`; a daemon with
    /// that key then connects as it.
    pub async fn local_machine(&self, key: &MachineKey, name: &str) -> Machine {
        self.state
            .machines
            .ensure_local(key.public_key(), name)
            .await
            .unwrap()
    }

    /// A project called `name` for the folder `/work/<name>` on an offline
    /// machine, stored without asking a daemon about the folder: for tests
    /// that never touch its files. Its ID.
    pub async fn stored_project(&self, name: &str) -> String {
        use ahab::modules::machines::machine::store::MachineRepository;
        use ahab::modules::projects::project::model::{
            Project, ProjectLook, ProjectMode, ProjectName, ProjectState,
        };
        use ahab::modules::projects::project::store::{DbProjectRepository, ProjectRepository};
        let at = datetime!(2026-11-17 09:00 UTC);
        let machine = Machine::joined(
            ahab::modules::machines::machine::model::MachineName::parse(&format!("box-{name}"))
                .unwrap(),
            {
                let mut secret = [0; 32];
                secret[..16].copy_from_slice(uuid::Uuid::now_v7().as_bytes());
                MachineKey::from_secret(secret).public_key()
            },
            at,
        );
        self.machines().insert(&machine).await.unwrap();
        let project = Project {
            id: uuid::Uuid::now_v7(),
            name: ProjectName::parse(name).unwrap(),
            description: None,
            state: ProjectState::Active,
            machine_id: machine.id,
            path: format!("/work/{name}"),
            mode: ProjectMode::ReadWrite,
            group: None,
            created_at: at,
            updated_at: at,
            look: ProjectLook::default(),
        };
        DbProjectRepository::new(self.database.db.clone())
            .insert(&project)
            .await
            .unwrap();
        project.id.to_string()
    }

    /// Waits until `machine`'s daemon is connected.
    pub async fn online(&self, machine: &Machine) {
        let id = machine.id;
        self.eventually("the daemon is online", |state| state.presence.is_online(id))
            .await;
    }

    /// Waits until `condition` holds, for at most five seconds.
    pub async fn eventually(&self, what: &str, condition: impl Fn(&AppState) -> bool) {
        for _ in 0..100 {
            if condition(&self.state) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("timed out waiting until {what}");
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
