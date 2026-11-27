//! Clients: the local credential, checking credentials, and listing,
//! renaming and revoking clients.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio::sync::watch;
use uuid::Uuid;

use crate::modules::clients::client::model::{
    Client, ClientKind, ClientName, PairedVia, Seen, credential_from_random, hash_secret,
    kept_after_revoking,
};
use crate::modules::clients::client::store::{
    ClientRepository, CredentialFileError, read_credential, write_private,
};
use crate::shared::error::ServiceError;
use crate::shared::events::Events;
use crate::shared::sync::lock;
use crate::shared::time::Clock;

/// The local client's name.
pub const LOCAL_NAME: &str = "This computer";

/// A client that was just paired, with its credential: shown only now.
#[derive(Debug, Clone)]
pub struct Paired {
    pub client: Client,
    pub credential: String,
}

/// Who listens for which client's revocation, and which were revoked while
/// this server runs (a stream that checks in late still ends).
#[derive(Default)]
struct Revocations {
    senders: HashMap<Uuid, watch::Sender<bool>>,
    revoked: HashSet<Uuid>,
}

/// What a new client is.
#[derive(Debug, Clone)]
pub struct NewClient {
    pub name: ClientName,
    pub kind: ClientKind,
    pub paired_via: PairedVia,
    pub paired_by: Option<Uuid>,
    pub user_agent: Option<String>,
    pub seen: Seen,
}

pub struct ClientService {
    clients: Arc<dyn ClientRepository>,
    clock: Arc<dyn Clock>,
    events: Arc<Events>,
    /// Each client's open streams and sockets listen here; revoking it
    /// closes them.
    revocations: Mutex<Revocations>,
    /// Where the local credential is kept, once [`Self::ensure_local`] ran.
    local_file: Mutex<Option<PathBuf>>,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "client_not_found",
        message: "There is no such client, or it was revoked.".to_owned(),
    }
}

fn file_error(error: CredentialFileError) -> ServiceError {
    ServiceError::Internal(error.to_string())
}

/// A new credential and its hash.
pub fn new_credential() -> Result<(String, [u8; 32]), ServiceError> {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| ServiceError::Internal(format!("no randomness: {error}")))?;
    let credential = credential_from_random(&random);
    let hash = hash_secret(&credential);
    Ok((credential, hash))
}

impl ClientService {
    pub fn new(
        clients: Arc<dyn ClientRepository>,
        clock: Arc<dyn Clock>,
        events: Arc<Events>,
    ) -> Self {
        Self {
            clients,
            clock,
            events,
            revocations: Mutex::default(),
            local_file: Mutex::new(None),
        }
    }

    /// Makes sure the local credential in `path` works: keeps the one there
    /// when it is the local client's, else writes a new one (at the first
    /// start, after it was revoked or deleted) and revokes the old.
    pub async fn ensure_local(&self, path: &Path) -> Result<Client, ServiceError> {
        *lock(&self.local_file) = Some(path.to_owned());
        if let Some(credential) = read_credential(path).map_err(file_error)?
            && let Some(client) = self
                .clients
                .find_by_credential(&hash_secret(&credential))
                .await?
            && client.kind == ClientKind::Local
            && !client.is_revoked()
        {
            return Ok(client);
        }
        self.replace_local(path).await
    }

    async fn replace_local(&self, path: &Path) -> Result<Client, ServiceError> {
        let now = self.clock.now();
        for old in self.clients.list_active().await? {
            if old.kind == ClientKind::Local && self.clients.revoke(old.id, now).await? {
                self.signal_revoked(old.id);
            }
        }
        let (credential, hash) = new_credential()?;
        let client = Client {
            encrypted: true,
            ..Client::new(
                ClientName::or(Some(LOCAL_NAME), LOCAL_NAME),
                ClientKind::Local,
                hash,
                PairedVia::Local,
                now,
            )
        };
        self.clients.insert(&client).await?;
        write_private(path, &format!("{credential}\n")).map_err(file_error)?;
        tracing::info!(path = %path.display(), "wrote the local credential");
        Ok(client)
    }

    /// The client whose credential this is, when it is paired; records the
    /// request at most once a minute.
    pub async fn authenticate(
        &self,
        credential: &str,
        seen: &Seen,
    ) -> Result<Option<Client>, ServiceError> {
        let Some(mut client) = self
            .clients
            .find_by_credential(&hash_secret(credential))
            .await?
        else {
            return Ok(None);
        };
        if client.is_revoked() {
            return Ok(None);
        }
        let now = self.clock.now();
        if client.needs_touch(now) {
            self.clients.touch(client.id, now, seen).await?;
            client.last_used_at = Some(now);
            client.last_address.clone_from(&seen.address);
            client.encrypted = seen.encrypted;
        }
        Ok(Some(client))
    }

    /// Pairs a new client: its record, and its credential.
    pub async fn pair(&self, new: NewClient) -> Result<Paired, ServiceError> {
        let (credential, hash) = new_credential()?;
        let now = self.clock.now();
        let client = Client {
            paired_by: new.paired_by,
            user_agent: new.user_agent,
            last_used_at: Some(now),
            last_address: new.seen.address,
            encrypted: new.seen.encrypted,
            ..Client::new(new.name, new.kind, hash, new.paired_via, now)
        };
        self.clients.insert(&client).await?;
        tracing::info!(client = %client.id, name = %client.name, kind = client.kind.as_str(), "paired a client");
        self.events.publish(
            "client.paired",
            json!({ "client": client.id, "name": client.name.as_str(), "kind": client.kind.as_str() }),
        );
        Ok(Paired { client, credential })
    }

    /// A token for a script: a client of kind `token`, made by `by`.
    pub async fn create_token(&self, name: &str, by: Option<Uuid>) -> Result<Paired, ServiceError> {
        let name = ClientName::parse(name)?;
        self.pair(NewClient {
            name,
            kind: ClientKind::Token,
            paired_via: PairedVia::Token,
            paired_by: by,
            user_agent: None,
            seen: Seen::default(),
        })
        .await
        .map(|mut paired| {
            // Not used yet.
            paired.client.last_used_at = None;
            paired
        })
    }

    /// The clients that are not revoked, oldest first. Clients revoked
    /// long ago are deleted on the way.
    pub async fn list(&self) -> Result<Vec<Client>, ServiceError> {
        let before = self.clock.now() - kept_after_revoking();
        let purged = self.clients.purge_revoked(before).await?;
        if purged > 0 {
            tracing::info!(purged, "deleted clients revoked over 30 days ago");
        }
        Ok(self.clients.list_active().await?)
    }

    /// A client that is not revoked.
    pub async fn get(&self, id: Uuid) -> Result<Client, ServiceError> {
        self.clients
            .get(id)
            .await?
            .filter(|client| !client.is_revoked())
            .ok_or_else(not_found)
    }

    /// Whether a client has gone unused long enough for the list to say so.
    pub fn is_unused(&self, client: &Client) -> bool {
        client.is_unused(self.clock.now())
    }

    /// Whether any client other than the server's own computer is paired.
    pub async fn has_paired_client(&self) -> Result<bool, ServiceError> {
        Ok(self
            .clients
            .list_active()
            .await?
            .iter()
            .any(|client| client.kind != ClientKind::Local))
    }

    pub async fn rename(&self, id: Uuid, name: &str) -> Result<Client, ServiceError> {
        let name = ClientName::parse(name)?;
        let mut client = self.get(id).await?;
        self.clients.rename(id, &name).await?;
        client.name = name;
        self.events.publish(
            "client.renamed",
            json!({ "client": id, "name": client.name.as_str() }),
        );
        Ok(client)
    }

    /// Revokes a client: its next request is refused, and its open streams
    /// and sockets close now. Revoking this computer's own client writes it
    /// a new credential at once, so the owner is never locked out.
    pub async fn revoke(&self, id: Uuid) -> Result<Client, ServiceError> {
        let client = self.get(id).await?;
        if !self.clients.revoke(id, self.clock.now()).await? {
            return Err(not_found());
        }
        self.signal_revoked(id);
        tracing::info!(client = %id, name = %client.name, "revoked a client");
        self.events
            .publish("client.revoked", json!({ "client": id }));
        if client.kind == ClientKind::Local {
            let path = lock(&self.local_file).clone();
            if let Some(path) = path {
                self.replace_local(&path).await?;
            }
        }
        Ok(client)
    }

    /// Turns true when `client` is revoked: its streams end then.
    pub fn revocation(&self, client: Uuid) -> watch::Receiver<bool> {
        let mut revocations = lock(&self.revocations);
        if revocations.revoked.contains(&client) {
            return watch::channel(true).1;
        }
        revocations
            .senders
            .entry(client)
            .or_insert_with(|| watch::channel(false).0)
            .subscribe()
    }

    fn signal_revoked(&self, client: Uuid) {
        let mut revocations = lock(&self.revocations);
        revocations.revoked.insert(client);
        if let Some(sender) = revocations.senders.remove(&client) {
            let _ = sender.send(true);
        }
    }
}
