//! Pairing: links and codes a paired client (or the server) offers, and
//! requests a new client makes and a paired one answers.

use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;
use time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::clients::client::model::{
    ClientKind, ClientName, PairedVia, Seen, browser_name, hash_secret,
};
use crate::modules::clients::client::service::{ClientService, NewClient, Paired};
use crate::modules::clients::pairing::model::{
    LINK_CODE_LENGTH, Limiter, MAX_OPEN_REQUESTS, Pairing, PairingKind, PairingState,
    REQUEST_CODE_LENGTH, code_from_random, first_run_valid_for, format_code, link_valid_for,
    normalize_code, request_valid_for,
};
use crate::modules::clients::pairing::store::PairingRepository;
use crate::shared::error::ServiceError;
use crate::shared::events::Events;
use crate::shared::sync::lock;
use crate::shared::time::Clock;

/// Who is asking, without a credential: what the approver sees, and what
/// the rate limits count by.
#[derive(Debug, Clone, Default)]
pub struct Asker {
    pub seen: Seen,
    pub user_agent: Option<String>,
}

impl Asker {
    fn limit_key(&self) -> &str {
        self.seen.address.as_deref().unwrap_or("unknown")
    }
}

/// A link (and code) just offered: the secrets are shown only now.
#[derive(Debug, Clone)]
pub struct Offered {
    pub pairing: Pairing,
    /// What goes after `/pair#` in the link.
    pub secret: String,
    /// The code to type instead, as shown (`K7QM-4RXT`); first-run links
    /// have none.
    pub code: Option<String>,
}

/// A request just made: its code to show, and the secret to wait with.
#[derive(Debug, Clone)]
pub struct Requested {
    pub pairing: Pairing,
    pub secret: String,
    pub code: String,
}

/// What a new client says about itself.
#[derive(Debug, Clone, Default)]
pub struct Introduction {
    pub name: Option<String>,
    pub kind: Option<String>,
}

/// How a wait for an answer ended.
#[derive(Debug)]
pub enum Answer {
    /// No answer yet.
    Waiting(Pairing),
    /// Approved: the new client and its credential.
    Paired(Paired),
    Denied,
}

pub struct PairingService {
    pairings: Arc<dyn PairingRepository>,
    clients: Arc<ClientService>,
    clock: Arc<dyn Clock>,
    events: Arc<Events>,
    limiter: Mutex<Limiter>,
    /// The pairings that changed, for those waiting on one.
    changed: broadcast::Sender<Uuid>,
}

fn invalid() -> ServiceError {
    // One error for unknown links and codes, so they cannot be probed.
    ServiceError::Forbidden {
        code: "pairing_invalid",
        message: "This pairing link or code does not work: it may be mistyped, used, withdrawn \
                  or expired. Ask for a new one."
            .to_owned(),
    }
}

fn expired() -> ServiceError {
    ServiceError::Forbidden {
        code: "pairing_expired",
        message: "This pairing ran out of time or was used already. Start again.".to_owned(),
    }
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "pairing_not_found",
        message: "There is no open pairing request with this code: it may have expired, or \
                  been answered already."
            .to_owned(),
    }
}

fn random<const N: usize>() -> Result<[u8; N], ServiceError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| ServiceError::Internal(format!("no randomness: {error}")))?;
    Ok(bytes)
}

fn new_secret() -> Result<String, ServiceError> {
    Ok(URL_SAFE_NO_PAD.encode(random::<32>()?))
}

impl PairingService {
    pub fn new(
        pairings: Arc<dyn PairingRepository>,
        clients: Arc<ClientService>,
        clock: Arc<dyn Clock>,
        events: Arc<Events>,
    ) -> Self {
        Self {
            pairings,
            clients,
            clock,
            events,
            limiter: Mutex::default(),
            changed: broadcast::channel(64).0,
        }
    }

    /// Counts a try at a secret from `asker`.
    fn limit(&self, asker: &Asker) -> Result<(), ServiceError> {
        lock(&self.limiter)
            .attempt(asker.limit_key(), self.clock.now())
            .map_err(|wait| ServiceError::TooManyAttempts {
                message: "Too many tries from this address. Wait a little, then try again."
                    .to_owned(),
                retry_after: u32::try_from(wait.whole_seconds()).unwrap_or(60).max(1),
            })
    }

    /// A link and code for a new client, offered by the paired client `by`.
    /// They work once, for 10 minutes.
    pub async fn offer(&self, by: Uuid) -> Result<Offered, ServiceError> {
        let code = code_from_random(&random::<LINK_CODE_LENGTH>()?);
        self.create_link(Some(by), Some(code), link_valid_for())
            .await
    }

    /// The first-run link, while no client but the server's own computer is
    /// paired: it works once, for 24 hours, and replaces earlier ones.
    /// `None` once a client is paired.
    pub async fn first_run(&self) -> Result<Option<Offered>, ServiceError> {
        if self.clients.has_paired_client().await? {
            return Ok(None);
        }
        let replaced = self.pairings.expire_first_run().await?;
        if replaced > 0 {
            tracing::info!(replaced, "earlier first-run links no longer work");
        }
        self.create_link(None, None, first_run_valid_for())
            .await
            .map(Some)
    }

    async fn create_link(
        &self,
        by: Option<Uuid>,
        code: Option<String>,
        valid_for: Duration,
    ) -> Result<Offered, ServiceError> {
        self.purge().await?;
        let secret = new_secret()?;
        let now = self.clock.now();
        let pairing = Pairing {
            id: Uuid::now_v7(),
            kind: PairingKind::Link,
            secret_hash: hash_secret(&secret),
            code_hash: code.as_deref().map(hash_secret),
            code: None,
            created_by: by,
            approved_by: None,
            client_name: None,
            client_kind: None,
            user_agent: None,
            address: None,
            state: PairingState::Open,
            client_id: None,
            expires_at: now + valid_for,
            created_at: now,
        };
        self.pairings.insert(&pairing).await?;
        Ok(Offered {
            pairing,
            secret,
            code: code.as_deref().map(format_code),
        })
    }

    async fn get_link(&self, id: Uuid) -> Result<Pairing, ServiceError> {
        self.pairings
            .get(id)
            .await?
            .filter(|pairing| pairing.kind == PairingKind::Link)
            .ok_or(ServiceError::NotFound {
                code: "pairing_not_found",
                message: "There is no such pairing link.".to_owned(),
            })
    }

    /// A link offered earlier, and where it stands now.
    pub async fn link(&self, id: Uuid) -> Result<(Pairing, PairingState), ServiceError> {
        let pairing = self.get_link(id).await?;
        let state = pairing.state_at(self.clock.now());
        Ok((pairing, state))
    }

    /// Withdraws an open link.
    pub async fn withdraw(&self, id: Uuid) -> Result<(), ServiceError> {
        let pairing = self.get_link(id).await?;
        self.pairings
            .transition(pairing.id, PairingState::Open, PairingState::Expired)
            .await?;
        Ok(())
    }

    /// Pairs the asking client with a link's secret or a code.
    pub async fn pair(
        &self,
        secret: Option<&str>,
        code: Option<&str>,
        introduction: &Introduction,
        asker: &Asker,
    ) -> Result<Paired, ServiceError> {
        self.limit(asker)?;
        let now = self.clock.now();
        let (pairing, via) = match (secret, code) {
            (Some(secret), _) => {
                let pairing = self
                    .pairings
                    .find_by_secret(&hash_secret(secret))
                    .await?
                    .filter(|pairing| pairing.kind == PairingKind::Link)
                    .ok_or_else(invalid)?;
                if !pairing.is_open(now) {
                    return Err(expired());
                }
                (pairing, PairedVia::Link)
            }
            (None, Some(code)) => {
                let found = match normalize_code(code) {
                    Some(code) if code.len() == LINK_CODE_LENGTH => self
                        .pairings
                        .find_open_by_code(PairingKind::Link, &hash_secret(&code))
                        .await?
                        .into_iter()
                        .find(|pairing| pairing.is_open(now)),
                    _ => None,
                };
                let Some(pairing) = found else {
                    self.wrong_code().await?;
                    return Err(invalid());
                };
                (pairing, PairedVia::Code)
            }
            (None, None) => return Err(invalid()),
        };
        // Claimed first: of two clients racing with one link, one wins.
        if !self
            .pairings
            .transition(pairing.id, PairingState::Open, PairingState::Used)
            .await?
        {
            return Err(expired());
        }
        let kind = ClientKind::claimed(introduction.kind.as_deref());
        let paired = self
            .clients
            .pair(NewClient {
                name: Self::name_for(kind, introduction.name.as_deref(), asker),
                kind,
                paired_via: via,
                paired_by: pairing.created_by,
                user_agent: asker.user_agent.clone(),
                seen: asker.seen.clone(),
            })
            .await?;
        self.pairings
            .mark_used(pairing.id, PairingState::Used, paired.client.id)
            .await?;
        self.paired(&pairing, &paired).await?;
        Ok(paired)
    }

    fn name_for(kind: ClientKind, name: Option<&str>, asker: &Asker) -> ClientName {
        let fallback = match kind {
            ClientKind::Browser => browser_name(asker.user_agent.as_deref()),
            ClientKind::Cli => "ahab CLI".to_owned(),
            _ => "An app".to_owned(),
        };
        ClientName::or(name, &fallback)
    }

    async fn wrong_code(&self) -> Result<(), ServiceError> {
        let too_many = lock(&self.limiter).wrong_code(self.clock.now());
        if too_many {
            let withdrawn = self.pairings.expire_codes().await?;
            tracing::warn!(
                withdrawn,
                "too many wrong pairing codes in an hour: every open code was withdrawn"
            );
        }
        Ok(())
    }

    /// After a client was paired: the link's maker hears of it, and the
    /// first-run link stops working.
    async fn paired(&self, pairing: &Pairing, paired: &Paired) -> Result<(), ServiceError> {
        self.close_first_run().await?;
        let _ = self.changed.send(pairing.id);
        self.events.publish(
            "pairing.used",
            json!({
                "pairing": pairing.id,
                "client": paired.client.id,
                "name": paired.client.name.as_str(),
            }),
        );
        Ok(())
    }

    /// The first-run link stops working once a client is paired.
    pub async fn close_first_run(&self) -> Result<(), ServiceError> {
        if self.clients.has_paired_client().await? {
            let closed = self.pairings.expire_first_run().await?;
            if closed > 0 {
                tracing::info!("a client is paired: the first-run link no longer works");
            }
        }
        Ok(())
    }

    /// A new client asks to be paired: it shows the code and waits with the
    /// secret.
    pub async fn request(
        &self,
        introduction: &Introduction,
        asker: &Asker,
    ) -> Result<Requested, ServiceError> {
        self.limit(asker)?;
        self.purge().await?;
        let now = self.clock.now();
        let open = self.pairings.list_open(PairingKind::Request, now).await?;
        if open.len() >= MAX_OPEN_REQUESTS {
            return Err(ServiceError::Conflict {
                code: "too_many_requests",
                message: "Too many clients wait for approval. Answer or let them expire first, \
                          or pair with a code from Add client."
                    .to_owned(),
            });
        }
        // A code no other open request shows.
        let mut code = code_from_random(&random::<REQUEST_CODE_LENGTH>()?);
        for _ in 0..16 {
            if !open
                .iter()
                .any(|other| other.code.as_deref() == Some(&code))
            {
                break;
            }
            code = code_from_random(&random::<REQUEST_CODE_LENGTH>()?);
        }
        let secret = new_secret()?;
        let kind = ClientKind::claimed(introduction.kind.as_deref());
        let pairing = Pairing {
            id: Uuid::now_v7(),
            kind: PairingKind::Request,
            secret_hash: hash_secret(&secret),
            code_hash: Some(hash_secret(&code)),
            code: Some(code.clone()),
            created_by: None,
            approved_by: None,
            client_name: Some(
                Self::name_for(kind, introduction.name.as_deref(), asker).to_string(),
            ),
            client_kind: Some(kind),
            user_agent: asker.user_agent.clone(),
            address: asker.seen.address.clone(),
            state: PairingState::Open,
            client_id: None,
            expires_at: now + request_valid_for(),
            created_at: now,
        };
        self.pairings.insert(&pairing).await?;
        tracing::info!(pairing = %pairing.id, code, address = ?pairing.address, "a client asks to be paired");
        self.events.publish(
            "pairing.requested",
            json!({
                "id": pairing.id,
                "code": code,
                "clientName": pairing.client_name,
                "clientKind": kind.as_str(),
                "address": pairing.address,
                "expiresAt": pairing.expires_at.unix_timestamp(),
            }),
        );
        Ok(Requested {
            pairing,
            secret,
            code,
        })
    }

    /// The requester waits, up to `wait`, for an answer. Once approved, the
    /// first wait after it pairs the client and hands over its credential.
    pub async fn wait(
        &self,
        id: Uuid,
        secret: &str,
        asker: &Asker,
        wait: StdDuration,
    ) -> Result<Answer, ServiceError> {
        self.limit(asker)?;
        let mut changes = self.changed.subscribe();
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let pairing = self
                .pairings
                .get(id)
                .await?
                .filter(|pairing| {
                    pairing.kind == PairingKind::Request
                        && pairing.secret_hash == hash_secret(secret)
                })
                .ok_or_else(invalid)?;
            match pairing.state_at(self.clock.now()) {
                PairingState::Approved => {
                    return self.hand_over(&pairing, asker).await.map(Answer::Paired);
                }
                PairingState::Denied => return Ok(Answer::Denied),
                PairingState::Used | PairingState::Expired => return Err(expired()),
                PairingState::Open => {}
            }
            // Until this one changes, or the time is up.
            loop {
                match tokio::time::timeout_at(deadline, changes.recv()).await {
                    Err(_) => return Ok(Answer::Waiting(pairing)),
                    Ok(Ok(changed)) if changed != id => {}
                    Ok(_) => break,
                }
            }
        }
    }

    /// Pairs the client that asked, now that it waits as `asker`.
    async fn hand_over(&self, pairing: &Pairing, asker: &Asker) -> Result<Paired, ServiceError> {
        if !self
            .pairings
            .transition(pairing.id, PairingState::Approved, PairingState::Used)
            .await?
        {
            return Err(expired());
        }
        let kind = pairing.client_kind.unwrap_or(ClientKind::App);
        let paired = self
            .clients
            .pair(NewClient {
                name: ClientName::or(pairing.client_name.as_deref(), "A client"),
                kind,
                paired_via: PairedVia::Request,
                paired_by: pairing.approved_by,
                user_agent: pairing.user_agent.clone(),
                seen: asker.seen.clone(),
            })
            .await?;
        self.pairings
            .mark_used(pairing.id, PairingState::Used, paired.client.id)
            .await?;
        self.paired(pairing, &paired).await?;
        Ok(paired)
    }

    /// The requests waiting for an answer, oldest first.
    pub async fn requests(&self) -> Result<Vec<Pairing>, ServiceError> {
        Ok(self
            .pairings
            .list_open(PairingKind::Request, self.clock.now())
            .await?)
    }

    /// An open request, by its ID or its code.
    async fn open_request(&self, key: &str) -> Result<Pairing, ServiceError> {
        let now = self.clock.now();
        let found = if let Ok(id) = Uuid::parse_str(key) {
            self.pairings.get(id).await?.into_iter().collect()
        } else {
            match normalize_code(key) {
                Some(code) if code.len() == REQUEST_CODE_LENGTH => {
                    self.pairings
                        .find_open_by_code(PairingKind::Request, &hash_secret(&code))
                        .await?
                }
                _ => Vec::new(),
            }
        };
        found
            .into_iter()
            .find(|pairing| pairing.kind == PairingKind::Request && pairing.is_open(now))
            .ok_or_else(not_found)
    }

    /// Approves a request: the waiting client is paired, by `approver`.
    pub async fn approve(&self, key: &str, approver: Uuid) -> Result<Pairing, ServiceError> {
        let mut pairing = self.open_request(key).await?;
        // Long enough for the client to take its credential.
        let until = self.clock.now() + request_valid_for();
        if !self.pairings.approve(pairing.id, approver, until).await? {
            return Err(not_found());
        }
        pairing.state = PairingState::Approved;
        pairing.approved_by = Some(approver);
        self.answered(&pairing);
        Ok(pairing)
    }

    /// Denies a request: the waiting client is told so.
    pub async fn deny(&self, key: &str) -> Result<Pairing, ServiceError> {
        let mut pairing = self.open_request(key).await?;
        if !self
            .pairings
            .transition(pairing.id, PairingState::Open, PairingState::Denied)
            .await?
        {
            return Err(not_found());
        }
        pairing.state = PairingState::Denied;
        self.answered(&pairing);
        Ok(pairing)
    }

    fn answered(&self, pairing: &Pairing) {
        tracing::info!(pairing = %pairing.id, state = pairing.state.as_str(), "a pairing request was answered");
        let _ = self.changed.send(pairing.id);
        self.events.publish(
            "pairing.answered",
            json!({ "id": pairing.id, "state": pairing.state.as_str() }),
        );
    }

    /// Deletes pairings that ended over a day ago.
    async fn purge(&self) -> Result<(), ServiceError> {
        self.pairings
            .purge(self.clock.now() - Duration::days(1))
            .await?;
        Ok(())
    }
}
