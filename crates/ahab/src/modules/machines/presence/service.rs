//! Which machines are connected right now. Connections live in memory: a
//! restarted server knows none until the daemons reconnect.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, broadcast, mpsc, oneshot, watch};
use uuid::Uuid;

use crate::modules::machines::identity::model::{MachineKey, verify};
use crate::modules::machines::machine::model::{Machine, MachineState, PublicKey};
use crate::modules::machines::machine::service::MachineService;
use crate::modules::machines::presence::api::{
    BASELINE_V1, Envelope, MachineFacts, OLDEST_PROTOCOL, PROTOCOL, WireError, channel, close,
    connect_message, server_proof_message,
};
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::events::Events;
use crate::shared::sync::lock;

/// What the server sends down one connection.
#[derive(Debug)]
pub enum Outgoing {
    Envelope(Envelope),
    /// A channel's binary frame (`channel::encode`).
    Frame(Vec<u8>),
    /// Close the connection with this code and reason.
    Close(u16, String),
}

/// Why a daemon was not let in: the WebSocket close code and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: u16,
    pub reason: String,
}

/// A daemon's hello, as received.
#[derive(Debug, Clone)]
pub struct Hello {
    pub protocol: u32,
    pub public_key: String,
    pub signature: String,
    pub facts: MachineFacts,
    /// The daemon's challenge for the server to sign.
    pub challenge: String,
    /// The methods it handles; `None` from a daemon older than
    /// capabilities.
    pub capabilities: Option<Vec<String>>,
}

/// The methods a connected daemon handles: what its hello listed, or the
/// protocol-1 baseline for a daemon that listed none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities(BTreeSet<String>);

impl Capabilities {
    pub fn from_hello(listed: Option<&[String]>) -> Self {
        Self(match listed {
            Some(listed) => listed.iter().cloned().collect(),
            None => BASELINE_V1
                .iter()
                .map(|&method| method.to_owned())
                .collect(),
        })
    }

    pub fn supports(&self, method: &str) -> bool {
        self.0.contains(method)
    }

    /// Which of `methods` it lacks.
    pub fn missing(&self, methods: &[&str]) -> Vec<String> {
        methods
            .iter()
            .filter(|&&method| !self.supports(method))
            .map(|&method| method.to_owned())
            .collect()
    }
}

/// An accepted connection: its number, what to send down it, and the
/// protocol version both sides speak.
pub struct Registration {
    pub generation: u64,
    pub outgoing: mpsc::Receiver<Outgoing>,
    pub acknowledged: watch::Receiver<u64>,
    pub retry: Arc<Notify>,
    pub protocol: u32,
    /// The token the daemon's plugins call the API with, valid
    /// while this connection lasts.
    pub api_token: String,
}

type Pending = HashMap<String, oneshot::Sender<Result<Value, WireError>>>;

struct Connection {
    generation: u64,
    outgoing: mpsc::Sender<Outgoing>,
    acknowledged: watch::Sender<u64>,
    durable: VecDeque<(u64, bool)>,
    retry: Arc<Notify>,
    pending: Arc<Mutex<Pending>>,
    /// Open channels: where each one's data from the daemon goes.
    channels: Arc<Mutex<Channels>>,
    /// The methods the daemon handles; nothing else is sent to it.
    capabilities: Arc<Capabilities>,
    /// Its Ahab version, for the message that asks to update it.
    version: String,
    /// The SHA-256 of the token this connection's daemon calls the API
    /// with.
    api_token_hash: [u8; 32],
}

impl Connection {
    /// Fails when the daemon does not handle `method`, before asking it.
    fn check(&self, method: &str) -> Result<(), ServiceError> {
        if self.capabilities.supports(method) {
            Ok(())
        } else {
            Err(outdated(method, &self.version))
        }
    }
}

type Channels = HashMap<Uuid, mpsc::Sender<Vec<u8>>>;

/// How many frames from the daemon a channel holds for a slow reader;
/// past it the channel closes (its reader reattaches and starts afresh).
const CHANNEL_FRAMES: usize = 512;

/// A stream of bytes with a daemon: read what the daemon sends,
/// send it bytes; dropping it closes the channel on both sides.
pub struct Channel {
    pub id: Uuid,
    incoming: mpsc::Receiver<Vec<u8>>,
    outgoing: mpsc::Sender<Outgoing>,
    channels: Arc<Mutex<Channels>>,
}

impl Channel {
    /// The next bytes from the daemon; `None` once the channel closed.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.incoming.recv().await
    }

    /// Sends bytes to the daemon; false once the connection is gone.
    pub async fn send(&self, payload: &[u8]) -> bool {
        self.outgoing
            .send(Outgoing::Frame(channel::encode(
                channel::DATA,
                self.id,
                payload,
            )))
            .await
            .is_ok()
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        lock(&self.channels).remove(&self.id);
        let _ = self.outgoing.try_send(Outgoing::Frame(channel::encode(
            channel::CLOSE,
            self.id,
            &[],
        )));
    }
}

/// Machine API tokens start with this, so they are told apart from client
/// credentials at once.
const MACHINE_TOKEN_PREFIX: &str = "ahabm_";

pub struct Presence {
    machines: Arc<MachineService>,
    /// The methods this server may call: a daemon lacking one is outdated.
    methods: &'static [&'static str],
    server_key: Arc<MachineKey>,
    connections: Mutex<HashMap<Uuid, Connection>>,
    next: AtomicU64,
    /// Announces each machine that connects, so other features can bring it
    /// up to date (its projects, its agents).
    connected: broadcast::Sender<Uuid>,
    /// Events from daemons, in order per machine, for the one feature that
    /// stores them ([`Presence::take_events`]).
    event_bytes: Arc<tokio::sync::Semaphore>,
    events: mpsc::Sender<MachineEvent>,
    events_out: Mutex<Option<mpsc::Receiver<MachineEvent>>>,
    /// Events whose kind starts with a prefix, for the feature that owns it
    /// ([`Presence::take_events_starting`]).
    routes: Mutex<Vec<(String, mpsc::Sender<MachineEvent>)>>,
    /// Ahab's own events, which every feature reaches through
    /// presence.
    bus: Arc<Events>,
}

/// An event a daemon sent: `seq` 0 for live-only events, which are never
/// acknowledged.
#[derive(Debug)]
pub struct MachineEvent {
    pub machine: Uuid,
    pub seq: u64,
    pub generation: u64,
    pub kind: String,
    pub data: Value,
    _bytes: tokio::sync::OwnedSemaphorePermit,
}

impl Presence {
    /// `methods` are those this server may call (this build's daemon's), to
    /// tell which machines run an older Ahab.
    pub fn new(
        machines: Arc<MachineService>,
        server_key: Arc<MachineKey>,
        methods: &'static [&'static str],
    ) -> Self {
        let (sender, receiver) = mpsc::channel(256);
        Self {
            machines,
            methods,
            server_key,
            connections: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            connected: broadcast::channel(64).0,
            event_bytes: Arc::new(tokio::sync::Semaphore::new(32 * 1024 * 1024)),
            events: sender,
            events_out: Mutex::new(Some(receiver)),
            routes: Mutex::new(Vec::new()),
            bus: Arc::default(),
        }
    }

    /// The server's key fingerprint, which daemons pin when they join.
    pub fn server_fingerprint(&self) -> String {
        self.server_key.public_key().fingerprint().to_string()
    }

    /// The server's public key and its signature of `challenge` from a
    /// daemon that connected to `host`, both in base64.
    pub fn prove(&self, host: &str, challenge: &str) -> (String, String) {
        let signature = self.server_key.sign(&server_proof_message(host, challenge));
        (
            STANDARD.encode(self.server_key.public_key().as_bytes()),
            STANDARD.encode(signature),
        )
    }

    /// Ahab's own events: features publish on it, plugins and the web app
    /// hear them.
    pub fn bus(&self) -> &Arc<Events> {
        &self.bus
    }

    /// Every machine that connects from now on. Requests sent on hearing of
    /// one wait in its queue until its handshake has finished.
    pub fn subscribe(&self) -> broadcast::Receiver<Uuid> {
        self.connected.subscribe()
    }

    /// Events whose kind starts with `prefix` (`plugin.`), from now on;
    /// they no longer go to [`Presence::take_events`].
    pub fn take_events_starting(&self, prefix: &str) -> mpsc::Receiver<MachineEvent> {
        let (sender, receiver) = mpsc::channel(256);
        lock(&self.routes).push((prefix.to_owned(), sender));
        receiver
    }

    /// Every event daemons send, from now on; only one caller gets them.
    pub fn take_events(&self) -> Option<mpsc::Receiver<MachineEvent>> {
        lock(&self.events_out).take()
    }

    /// Acks the largest contiguous prefix across all event consumers.
    pub fn ack(&self, machine: Uuid, generation: u64, seq: u64) {
        let mut connections = lock(&self.connections);
        let Some(connection) = connections
            .get_mut(&machine)
            .filter(|connection| connection.generation == generation)
        else {
            return;
        };
        if let Some((_, done)) = connection
            .durable
            .iter_mut()
            .find(|(accepted, _)| *accepted == seq)
        {
            *done = true;
        }
        let mut latest = None;
        while connection.durable.front().is_some_and(|(_, done)| *done) {
            latest = connection.durable.pop_front().map(|(seq, _)| seq);
        }
        if let Some(seq) = latest {
            connection.acknowledged.send_replace(seq);
        }
    }

    /// A failed durable write needs a replay, even if the socket is otherwise healthy.
    pub fn retry_events(&self, machine: Uuid, generation: u64) {
        if let Some(connection) = lock(&self.connections)
            .get(&machine)
            .filter(|connection| connection.generation == generation)
        {
            connection.retry.notify_one();
        }
    }

    /// The machines connected right now.
    pub fn online(&self) -> Vec<Uuid> {
        lock(&self.connections).keys().copied().collect()
    }

    pub fn is_online(&self, machine: Uuid) -> bool {
        lock(&self.connections).contains_key(&machine)
    }

    /// What `machine`'s daemon handles; `None` while it is not connected.
    pub fn capabilities(&self, machine: Uuid) -> Option<Arc<Capabilities>> {
        lock(&self.connections)
            .get(&machine)
            .map(|connection| Arc::clone(&connection.capabilities))
    }

    /// Whether `machine`'s daemon is connected and handles `method`, for
    /// features that skip what an older daemon cannot do.
    pub fn supports(&self, machine: Uuid, method: &str) -> bool {
        self.capabilities(machine)
            .is_some_and(|capabilities| capabilities.supports(method))
    }

    /// The methods this server may call that `machine`'s daemon lacks: it
    /// runs an older Ahab when any. `None` while it is not connected.
    pub fn missing_methods(&self, machine: Uuid) -> Option<Vec<String>> {
        self.capabilities(machine)
            .map(|capabilities| capabilities.missing(self.methods))
    }

    /// Checks a hello: the protocol, the signature of `host` and `nonce`, and
    /// that the key belongs to an active machine.
    pub async fn authenticate(
        &self,
        hello: &Hello,
        host: &str,
        nonce: &str,
    ) -> Result<(Machine, u32), Rejection> {
        let reject = |code: u16, reason: &str| Rejection {
            code,
            reason: reason.to_owned(),
        };
        if hello.protocol < OLDEST_PROTOCOL {
            return Err(reject(
                close::OUTDATED,
                "This daemon is too old for this server: update Ahab.",
            ));
        }
        let public_key = STANDARD
            .decode(&hello.public_key)
            .ok()
            .and_then(|bytes| PublicKey::from_bytes(&bytes).ok())
            .ok_or_else(|| reject(close::UNKNOWN_KEY, "The public key is invalid."))?;
        let signature = STANDARD
            .decode(&hello.signature)
            .map_err(|_| reject(close::UNKNOWN_KEY, "The signature is invalid."))?;
        if !verify(&public_key, &connect_message(host, nonce), &signature) {
            return Err(reject(close::UNKNOWN_KEY, "The signature does not match."));
        }
        let machine = self
            .machines
            .find_by_key(&public_key)
            .await
            .map_err(|_| reject(1011, "The server cannot check this machine right now."))?
            .ok_or_else(|| {
                reject(
                    close::UNKNOWN_KEY,
                    "This server does not know this machine: join it with `ahab setup`.",
                )
            })?;
        if machine.state == MachineState::Pending {
            return Err(reject(close::PENDING, "This machine waits for approval."));
        }
        Ok((machine, hello.protocol.min(PROTOCOL)))
    }

    /// Records a connection for `machine`, replacing (and closing) an older
    /// one, and stores the facts the daemon reported. Only the methods in
    /// `capabilities` are sent to it.
    pub async fn connect(
        &self,
        machine: &Machine,
        facts: &MachineFacts,
        protocol: u32,
        capabilities: Capabilities,
    ) -> Result<Registration, ServiceError> {
        self.machines
            .record_connection(machine.id, &facts.os, &facts.arch, &facts.version)
            .await?;
        let (sender, receiver) = mpsc::channel(256);
        let (acknowledge, acknowledged) = watch::channel(0);
        let retry = Arc::new(Notify::new());
        let generation = self.next.fetch_add(1, Ordering::Relaxed);
        let mut random = [0u8; 32];
        getrandom::fill(&mut random)
            .map_err(|error| ServiceError::Internal(format!("no randomness: {error}")))?;
        let api_token = format!(
            "{MACHINE_TOKEN_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
        );
        let previous = lock(&self.connections).insert(
            machine.id,
            Connection {
                generation,
                outgoing: sender,
                acknowledged: acknowledge,
                durable: VecDeque::new(),
                retry: Arc::clone(&retry),
                pending: Arc::new(Mutex::new(HashMap::new())),
                channels: Arc::new(Mutex::new(HashMap::new())),
                capabilities: Arc::new(capabilities),
                version: facts.version.clone(),
                api_token_hash: Sha256::digest(api_token.as_bytes()).into(),
            },
        );
        if let Some(previous) = previous {
            fail_pending(&previous.pending, "machine_disconnected");
            let _ = previous.outgoing.try_send(Outgoing::Close(
                close::REPLACED,
                "A newer connection for this machine replaced this one.".to_owned(),
            ));
        }
        tracing::info!(machine = %machine.id, name = %machine.name, "daemon connected");
        let _ = self.connected.send(machine.id);
        self.bus.publish(
            "machine.online",
            serde_json::json!({ "machine": machine.id, "name": machine.name.as_str() }),
        );
        Ok(Registration {
            generation,
            outgoing: receiver,
            acknowledged,
            retry,
            protocol,
            api_token,
        })
    }

    /// The connected machine whose daemon holds this API token.
    pub fn machine_for_token(&self, token: &str) -> Option<Uuid> {
        if !token.starts_with(MACHINE_TOKEN_PREFIX) {
            return None;
        }
        let hash: [u8; 32] = Sha256::digest(token.trim().as_bytes()).into();
        lock(&self.connections)
            .iter()
            .find(|(_, connection)| connection.api_token_hash == hash)
            .map(|(machine, _)| *machine)
    }

    /// Closes `machine`'s connection because it was removed: its daemon is
    /// told its key is unknown now, and stops.
    pub fn close_removed(&self, machine: Uuid) {
        let connection = lock(&self.connections).remove(&machine);
        if let Some(connection) = connection {
            fail_pending(&connection.pending, "machine_disconnected");
            let _ = connection.outgoing.try_send(Outgoing::Close(
                close::UNKNOWN_KEY,
                "This machine was removed from the server: join it again with `ahab setup`."
                    .to_owned(),
            ));
            tracing::info!(%machine, "closed the connection of a removed machine");
        }
    }

    /// The connection `generation` of `machine` ended.
    pub async fn disconnect(&self, machine: Uuid, generation: u64) {
        let removed = {
            let mut connections = lock(&self.connections);
            match connections.get(&machine) {
                Some(connection) if connection.generation == generation => {
                    connections.remove(&machine)
                }
                _ => None,
            }
        };
        if let Some(connection) = removed {
            fail_pending(&connection.pending, "machine_disconnected");
            tracing::info!(%machine, "daemon disconnected");
            self.bus
                .publish("machine.offline", serde_json::json!({ "machine": machine }));
        }
        if let Err(error) = self.machines.touch(machine).await {
            tracing::warn!(%machine, %error, "cannot record when the machine was last seen");
        }
    }

    /// The daemon is still there.
    pub async fn heartbeat(&self, machine: Uuid) {
        if let Err(error) = self.machines.touch(machine).await {
            tracing::warn!(%machine, %error, "cannot record when the machine was last seen");
        }
    }

    /// A channel's frame from the daemon on connection `generation`.
    pub fn receive_frame(&self, machine: Uuid, generation: u64, frame: &[u8]) {
        let Some((kind, id, payload)) = channel::decode(frame) else {
            return;
        };
        let channels = {
            let connections = lock(&self.connections);
            match connections.get(&machine) {
                Some(connection) if connection.generation == generation => {
                    Arc::clone(&connection.channels)
                }
                _ => return,
            }
        };
        let mut channels = lock(&channels);
        if kind == channel::CLOSE {
            channels.remove(&id);
        } else if let Some(sender) = channels.get(&id)
            && sender.try_send(payload.to_vec()).is_err()
        {
            // Its reader is gone, or too slow: the channel closes.
            channels.remove(&id);
        }
    }

    /// Opens a channel with `machine`'s daemon through the request `method`,
    /// whose `params` gain the channel's ID as `channel`.
    pub async fn open_channel(
        &self,
        machine: Uuid,
        method: &str,
        mut params: Value,
        timeout: Duration,
    ) -> Result<Result<Channel, WireError>, ServiceError> {
        let id = Uuid::now_v7();
        let (sender, incoming) = mpsc::channel(CHANNEL_FRAMES);
        let (outgoing, channels) = {
            let connections = lock(&self.connections);
            let connection = connections.get(&machine).ok_or_else(offline)?;
            connection.check(method)?;
            (
                connection.outgoing.clone(),
                Arc::clone(&connection.channels),
            )
        };
        lock(&channels).insert(id, sender);
        if let Value::Object(map) = &mut params {
            map.insert("channel".to_owned(), Value::String(id.to_string()));
        }
        let channel = Channel {
            id,
            incoming,
            outgoing,
            channels,
        };
        Ok(
            match self.request_raw(machine, method, params, timeout).await? {
                Ok(_) => Ok(channel),
                Err(error) => Err(error),
            },
        )
    }

    /// A message from the daemon on connection `generation`.
    /// False closes an overloaded connection so its daemon replays durable events.
    pub fn receive(
        &self,
        machine: Uuid,
        generation: u64,
        envelope: Envelope,
        bytes: usize,
    ) -> bool {
        let (id, result, error) = match envelope {
            Envelope::Response { id, result, error } => (id, result, error),
            Envelope::Event { seq, kind, data } => {
                if kind == "buffer.overflow" {
                    let dropped = data.get("dropped").and_then(Value::as_u64).unwrap_or(0);
                    tracing::warn!(
                        %machine,
                        dropped,
                        "the machine dropped events it could not keep while disconnected"
                    );
                }
                let mut connections = lock(&self.connections);
                let Some(connection) = connections
                    .get_mut(&machine)
                    .filter(|connection| connection.generation == generation)
                else {
                    return false;
                };
                if seq > 0 && connection.durable.len() >= 512 {
                    return false;
                }
                let Ok(bytes) = u32::try_from(bytes) else {
                    return seq == 0;
                };
                let Ok(permit) = Arc::clone(&self.event_bytes).try_acquire_many_owned(bytes) else {
                    return seq == 0;
                };
                let event = MachineEvent {
                    machine,
                    seq,
                    generation,
                    kind,
                    data,
                    _bytes: permit,
                };
                let routed = lock(&self.routes)
                    .iter()
                    .find(|(prefix, _)| event.kind.starts_with(prefix.as_str()))
                    .map(|(_, sender)| sender.clone());
                let sender = routed.as_ref().unwrap_or(&self.events);
                if sender.try_send(event).is_err() {
                    return seq == 0;
                }
                if seq > 0 {
                    connection.durable.push_back((seq, false));
                }
                return true;
            }
            _ => return true,
        };
        let pending = {
            let connections = lock(&self.connections);
            match connections.get(&machine) {
                Some(connection) if connection.generation == generation => {
                    Arc::clone(&connection.pending)
                }
                _ => return true,
            }
        };
        let waiter = lock(&pending).remove(&id);
        if let Some(waiter) = waiter {
            let answer = match (result, error) {
                (_, Some(error)) => Err(error),
                (result, None) => Ok(result.unwrap_or(Value::Null)),
            };
            let _ = waiter.send(answer);
        }
        true
    }

    /// Asks `machine`'s daemon to run `method`, and waits at most `timeout`:
    /// the daemon's value, or the error it answered with; not reaching the
    /// machine, or a daemon too old for `method`, is the outer error.
    async fn request_raw(
        &self,
        machine: Uuid,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Result<Value, WireError>, ServiceError> {
        let id = Uuid::now_v7().to_string();
        let (sender, receiver) = oneshot::channel();
        let (outgoing, pending, version) = {
            let connections = lock(&self.connections);
            let connection = connections.get(&machine).ok_or_else(offline)?;
            connection.check(method)?;
            lock(&connection.pending).insert(id.clone(), sender);
            (
                connection.outgoing.clone(),
                Arc::clone(&connection.pending),
                connection.version.clone(),
            )
        };
        let request = Envelope::Request {
            id: id.clone(),
            method: method.to_owned(),
            params,
            timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        };
        // A caller that goes away (a closed HTTP request) cancels the work
        // on the machine too.
        let mut unanswered = Unanswered {
            id: Some(id.clone()),
            outgoing: outgoing.clone(),
            pending: Arc::clone(&pending),
        };
        let deadline = tokio::time::Instant::now() + timeout;
        match tokio::time::timeout_at(deadline, outgoing.send(Outgoing::Envelope(request))).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(disconnected()),
            Err(_) => return Err(machine_timeout(method)),
        }
        let answered = tokio::time::timeout_at(deadline, receiver).await;
        unanswered.id = None;
        match answered {
            Ok(Ok(Ok(value))) => Ok(Ok(value)),
            Ok(Ok(Err(error))) if error.code == "machine_disconnected" => Err(disconnected()),
            // Its capabilities said otherwise (they drifted): still older.
            Ok(Ok(Err(error))) if error.code == "unknown_method" => Err(outdated(method, &version)),
            Ok(Ok(Err(error))) => Ok(Err(error)),
            Ok(Err(_)) => Err(disconnected()),
            Err(_) => {
                lock(&pending).remove(&id);
                let _ = outgoing.try_send(Outgoing::Envelope(Envelope::Cancel { id }));
                Err(machine_timeout(method))
            }
        }
    }
}

fn machine_timeout(method: &str) -> ServiceError {
    ServiceError::Unavailable {
        code: "machine_timeout",
        message: format!("The machine did not answer {method} in time."),
    }
}

/// A request still waiting for its answer: dropped while waiting, it tells
/// the machine to stop.
struct Unanswered {
    id: Option<String>,
    outgoing: mpsc::Sender<Outgoing>,
    pending: Arc<Mutex<Pending>>,
}

impl Drop for Unanswered {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            lock(&self.pending).remove(&id);
            let _ = self
                .outgoing
                .try_send(Outgoing::Envelope(Envelope::Cancel { id }));
        }
    }
}

impl Presence {
    /// Sends a request; an error the daemon answered with is a conflict
    /// (`machine_request_failed`) carrying its message and code.
    pub async fn request(
        &self,
        machine: Uuid,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ServiceError> {
        self.request_raw(machine, method, params, timeout)
            .await?
            .map_err(|error| ServiceError::Conflict {
                code: "machine_request_failed",
                message: format!("{} ({})", error.message, error.code),
            })
    }

    /// Sends a request and reads its answer as `T`. The daemon's errors are
    /// mapped by `own` (a component's own codes), then by the codes every
    /// project request shares, else reported as `fallback`.
    pub async fn ask<T: DeserializeOwned>(
        &self,
        machine: Uuid,
        method: &str,
        params: Value,
        timeout: Duration,
        own: fn(&str, &str) -> Option<ServiceError>,
        fallback: &'static str,
    ) -> Result<T, ServiceError> {
        match self.request_raw(machine, method, params, timeout).await? {
            Ok(value) => serde_json::from_value(value).map_err(|error| {
                ServiceError::Internal(format!("unreadable {method} answer: {error}"))
            }),
            Err(error) => {
                Err(own(&error.code, &error.message)
                    .unwrap_or_else(|| shared_error(error, fallback)))
            }
        }
    }
}

/// Whether [`shared_error`] gives `code` a meaning of its own.
pub fn is_shared(code: &str) -> bool {
    matches!(
        code,
        "project_unknown"
            | "folder_missing"
            | "read_only"
            | "path_outside_folder"
            | "permission_denied"
            | "invalid_path"
    )
}

/// What the codes every project request may answer mean to the API.
pub fn shared_error(error: WireError, fallback: &'static str) -> ServiceError {
    let WireError { code, message } = error;
    match code.as_str() {
        "project_unknown" => ServiceError::Unavailable {
            code: "project_unknown",
            message: "The machine does not know this project yet; try again in a moment."
                .to_owned(),
        },
        "folder_missing" => ServiceError::NotFound {
            code: "folder_missing",
            message,
        },
        "read_only" => ServiceError::Forbidden {
            code: "read_only",
            message,
        },
        "path_outside_folder" => ServiceError::Forbidden {
            code: "path_outside_folder",
            message,
        },
        "permission_denied" => ServiceError::Forbidden {
            code: "permission_denied",
            message,
        },
        "invalid_path" => ServiceError::Invalid(vec![FieldError::new("path", message)]),
        _ => ServiceError::Conflict {
            code: fallback,
            message,
        },
    }
}

fn fail_pending(pending: &Mutex<Pending>, code: &str) {
    for (_, waiter) in lock(pending).drain() {
        let _ = waiter.send(Err(WireError::new(code, "The machine disconnected.")));
    }
}

/// The daemon does not handle `method`: its Ahab is older than the server's.
/// A conflict (409): retrying will not help until the machine is updated.
fn outdated(method: &str, version: &str) -> ServiceError {
    let runs = if version.is_empty() {
        "an older Ahab".to_owned()
    } else {
        format!("an older Ahab ({version})")
    };
    ServiceError::Conflict {
        code: "machine_outdated",
        message: format!("This machine runs {runs}: update it to use {method}."),
    }
}

fn offline() -> ServiceError {
    ServiceError::Unavailable {
        code: "machine_offline",
        message: "The machine is not connected.".to_owned(),
    }
}

fn disconnected() -> ServiceError {
    ServiceError::Unavailable {
        code: "machine_disconnected",
        message: "The machine disconnected before it answered.".to_owned(),
    }
}
