//! What the app does, apart from its windows: pairing with servers, the
//! server list, and Ahab on this computer. The Tauri commands
//! (`commands.rs`, `bridge.rs`) are thin layers over it, so it is tested
//! without a window.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::computer::{Computer, ComputerError, Role, ThisComputer};
use crate::links::normalize_server;
use crate::secrets::{SecretStore, StoreKind};
use crate::server_api::{
    Answer, ApiError, Health, LocalAgents, PairingRequest, ServerApi, client_name,
};
use crate::servers::{Server, ServerList, Servers, ServersError, default_name};
use crate::settings::Files;

/// What the built-in screens get when something fails: a stable code and
/// what to tell the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct Failure {
    pub code: String,
    pub message: String,
}

impl Failure {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl From<ApiError> for Failure {
    fn from(error: ApiError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<ComputerError> for Failure {
    fn from(error: ComputerError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<ServersError> for Failure {
    fn from(error: ServersError) -> Self {
        let code = match error {
            ServersError::NotFound => "not_found",
            ServersError::Secret(_) => "keychain",
            _ => "storage",
        };
        Self::new(code, error.to_string())
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::new("storage", error.to_string())
    }
}

/// A server reached for the first time: its address and version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reached {
    pub url: String,
    pub version: String,
    /// Already in the list.
    pub known: Option<Server>,
}

/// A request to be paired, as the connect screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    pub id: String,
    pub code: String,
}

/// Where a request stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Waited {
    Waiting,
    Denied,
    Expired,
    Paired { server: Server },
}

/// The servers and the store their credentials are in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerOverview {
    #[serde(flatten)]
    pub list: ServerList,
    pub store: StoreKind,
    /// Why credentials are not in the keychain, when they are not.
    pub store_note: Option<String>,
}

pub struct AppState {
    pub servers: Servers,
    pub computer: Computer,
    pub files: Files,
    pub store_note: Option<String>,
    waiting: Mutex<HashMap<String, (String, PairingRequest)>>,
}

impl AppState {
    pub fn new(
        dir: &Path,
        secrets: Box<dyn SecretStore>,
        store_note: Option<String>,
        computer: Computer,
    ) -> Arc<Self> {
        Arc::new(Self {
            servers: Servers::new(dir, secrets),
            computer,
            files: Files::new(dir),
            store_note,
            waiting: Mutex::new(HashMap::new()),
        })
    }

    pub fn overview(&self) -> Result<ServerOverview, Failure> {
        Ok(ServerOverview {
            list: self.servers.load()?,
            store: self.servers.secrets().kind(),
            store_note: self.store_note.clone(),
        })
    }

    /// Checks that `input` is an Ahab server.
    pub async fn reach(&self, input: &str) -> Result<Reached, Failure> {
        let url = normalize_server(input).ok_or_else(|| {
            Failure::new(
                "bad_address",
                format!("{input} is not a server address, such as https://ahab.example."),
            )
        })?;
        let Health { version } = ServerApi::new(&url).health().await?;
        Ok(Reached {
            known: self.servers.find_by_url(&url)?,
            url,
            version,
        })
    }

    /// Pairs with a link's secret or a code from Add client.
    pub async fn pair(
        &self,
        url: &str,
        secret: Option<&str>,
        code: Option<&str>,
    ) -> Result<Server, Failure> {
        let url = normalize_server(url).ok_or_else(|| Failure::new("bad_address", url))?;
        let api = ServerApi::new(&url);
        let paired = api.pair(secret, code, &client_name()).await?;
        Ok(self.servers.add(
            &url,
            &default_name(&url, false),
            false,
            paired.client_id,
            &paired.credential,
        )?)
    }

    /// Asks `url` to pair; a paired client approves the code shown.
    pub async fn ask(&self, url: &str) -> Result<Asked, Failure> {
        let url = normalize_server(url).ok_or_else(|| Failure::new("bad_address", url))?;
        let request = ServerApi::new(&url).request_pairing(&client_name()).await?;
        let asked = Asked {
            id: request.id.clone(),
            code: request.code.clone(),
        };
        self.waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(request.id.clone(), (url, request));
        Ok(asked)
    }

    /// Waits a while for the answer to a request made with [`Self::ask`].
    pub async fn wait(&self, id: &str) -> Result<Waited, Failure> {
        let found = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned();
        let (url, request) = found.ok_or_else(|| Failure::new("not_found", "No such request."))?;
        let answer = ServerApi::new(&url).wait_for_answer(&request).await?;
        let done = |state| {
            self.waiting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(id);
            state
        };
        Ok(match answer {
            Answer::Waiting => Waited::Waiting,
            Answer::Denied => done(Waited::Denied),
            Answer::Expired => done(Waited::Expired),
            Answer::Paired(paired) => {
                let server = self.servers.add(
                    &url,
                    &default_name(&url, false),
                    false,
                    paired.client_id,
                    &paired.credential,
                )?;
                done(Waited::Paired { server })
            }
        })
    }

    /// Forgets a server: revokes the app's client there when it answers,
    /// then forgets the credential.
    pub async fn remove(&self, id: &str) -> Result<Server, Failure> {
        let server = self.servers.get(id)?;
        if let (Some(credential), Some(client)) =
            (self.servers.credential(id)?, server.client_id.as_deref())
        {
            let revoked = ServerApi::new(&server.url)
                .revoke(&credential, client)
                .await;
            if let Err(error) = revoked {
                tracing::info!(%error, server = %server.url, "could not revoke the app's client; forgetting it anyway");
            }
        }
        Ok(self.servers.remove(id)?)
    }

    /// Whether a server's credential still works (it may have been
    /// revoked); `Ok(false)` when it was refused.
    /// The machine of a server this app runs here, and the agents it found.
    pub async fn local_agents(&self, id: &str) -> Result<LocalAgents, Failure> {
        let server = self.servers.get(id)?;
        let credential = self
            .servers
            .credential(id)?
            .ok_or_else(|| Failure::new("not_paired", "This app is not paired with it."))?;
        Ok(ServerApi::new(&server.url)
            .local_agents(&credential)
            .await?)
    }

    pub async fn still_paired(&self, id: &str) -> Result<bool, Failure> {
        let server = self.servers.get(id)?;
        let Some(credential) = self.servers.credential(id)? else {
            return Ok(false);
        };
        match ServerApi::new(&server.url)
            .current_client(&credential)
            .await
        {
            Ok(_) => Ok(true),
            Err(ApiError::Problem { code, .. }) if code == "not_paired" => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn this_computer(&self) -> ThisComputer {
        let computer = self.computer.clone();
        tokio::task::spawn_blocking(move || computer.detect())
            .await
            .unwrap_or_else(|_| crate::computer::interpret(&serde_json::Value::Null, None, None))
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(Computer) -> Result<T, ComputerError> + Send + 'static,
    ) -> Result<T, Failure> {
        let computer = self.computer.clone();
        tokio::task::spawn_blocking(move || work(computer))
            .await
            .map_err(|error| Failure::new("cannot_run", error.to_string()))?
            .map_err(Failure::from)
    }

    /// "Run Ahab here": installs the bundled `ahab` unless one is
    /// installed, starts the user service, and pairs the app through the
    /// local credential.
    pub async fn run_here(&self) -> Result<Server, Failure> {
        let computer = self.blocking(|computer| computer.start()).await?;
        self.pair_locally(&computer).await
    }

    /// "Use the Ahab already running on this computer": starts it if it is
    /// set up but stopped, then pairs through the local credential. A
    /// computer that is a machine of another server has no server here:
    /// `joined_elsewhere` with that server's address, to connect to it.
    pub async fn use_existing(&self) -> Result<Server, Failure> {
        let mut computer = self.this_computer().await;
        match computer.role {
            Role::Machine => {
                return Err(Failure::new(
                    "joined_elsewhere",
                    computer.joined.clone().unwrap_or_default(),
                ));
            }
            Role::None => {
                return Err(Failure::new(
                    "no_ahab",
                    "Ahab is not set up on this computer.",
                ));
            }
            Role::Server => {}
        }
        if computer.server_version.is_none() {
            computer = self.blocking(|computer| computer.start()).await?;
        }
        self.pair_locally(&computer).await
    }

    /// Pairs with the server this computer runs, through its local
    /// credential; reuses a credential that still works.
    async fn pair_locally(&self, computer: &ThisComputer) -> Result<Server, Failure> {
        let url = computer
            .url
            .clone()
            .ok_or_else(|| Failure::new("not_running", "Ahab does not run on this computer."))?;
        let api = ServerApi::new(&url);
        api.health().await?;
        if let Some(known) = self.servers.find_by_url(&url)?
            && self.still_paired(&known.id).await.unwrap_or(false)
        {
            return Ok(known);
        }
        let local = self.computer.local_credential()?;
        let secret = api.create_link(&local).await?;
        let paired = api.pair(Some(&secret), None, &client_name()).await?;
        Ok(self.servers.add(
            &url,
            &default_name(&url, true),
            true,
            paired.client_id,
            &paired.credential,
        )?)
    }

    /// Joins this computer to a paired server as a machine: a join code
    /// and the server's fingerprint from the server, through the app's
    /// credential, then `ahab setup` with that fingerprint pinned.
    pub async fn join_server(&self, id: &str) -> Result<ThisComputer, Failure> {
        let server = self.servers.get(id)?;
        let credential = self
            .servers
            .credential(id)?
            .ok_or_else(|| Failure::new("not_paired", "This server has no credential here."))?;
        let host = gethostname::gethostname().to_string_lossy().into_owned();
        let code = ServerApi::new(&server.url)
            .create_join_code(
                &credential,
                Some(&host)
                    .filter(|host| !host.is_empty())
                    .map(String::as_str),
            )
            .await?;
        let url = server.url.clone();
        self.blocking(move |computer| {
            computer.join(&url, &code.code, code.fingerprint.as_deref(), false)
        })
        .await
    }

    /// Joins with a code from a link or typed: the person saw the server
    /// and its fingerprint (when the link had one) and confirmed.
    pub async fn join_with_code(
        &self,
        server: &str,
        code: &str,
        fingerprint: Option<&str>,
    ) -> Result<ThisComputer, Failure> {
        let url = normalize_server(server).ok_or_else(|| Failure::new("bad_address", server))?;
        ServerApi::new(&url).health().await?;
        let code = code.to_owned();
        let fingerprint = fingerprint.map(str::to_owned);
        self.blocking(move |computer| {
            computer.join(&url, &code, fingerprint.as_deref(), fingerprint.is_none())
        })
        .await
    }

    pub async fn start_here(&self) -> Result<ThisComputer, Failure> {
        self.blocking(|computer| computer.start()).await
    }

    pub async fn stop_here(&self) -> Result<ThisComputer, Failure> {
        self.blocking(|computer| computer.stop()).await
    }

    pub async fn leave(&self) -> Result<ThisComputer, Failure> {
        self.blocking(|computer| computer.leave()).await
    }
}
