//! The daemon's connection to its server: connect, prove which machine it is,
//! answer requests, and reconnect with backoff when the connection drops.

use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::daemon::dispatch;
use crate::modules::machines::identity::model::verify;
use crate::modules::machines::presence::api::{
    Envelope, Handshake, MachineFacts, PROTOCOL, close, connect_message, server_proof_message,
};
use crate::modules::machines::{MachineKey, PublicKey};

/// What the daemon needs to connect.
pub struct DaemonConfig {
    /// The server, such as `http://127.0.0.1:8180`.
    pub server: String,
    pub key: MachineKey,
    pub facts: MachineFacts,
    /// The server key fingerprint this machine pinned when it joined; the
    /// daemon refuses a server with another key. `None` for the local daemon.
    pub server_fingerprint: Option<String>,
    /// Where the daemon keeps what it stores, such as attachments.
    pub data_dir: std::path::PathBuf,
    /// The `ahab` that runs the terminal host; this executable by default
    /// (tests run the daemon inside another).
    pub executable: Option<std::path::PathBuf>,
    /// The Node that runs plugins (`[plugins] node`); `node` on `PATH` when
    /// absent.
    pub plugin_node: Option<std::path::PathBuf>,
    /// How language tools are found and installed.
    pub tools: crate::modules::tools::registry::daemon::ToolOptions,
}

/// Why the daemon stopped trying.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("the server address {0} is invalid")]
    InvalidServer(String),
    #[error("the server does not know this machine: join it with `ahab setup`")]
    UnknownMachine,
    #[error("this Ahab is too old for the server: update it")]
    Outdated,
    #[error(
        "the server's key is not the one this machine joined ({expected}, now {found}): \
         if the server was reinstalled, join it again with `ahab setup`"
    )]
    ServerKeyChanged { expected: String, found: String },
    #[error("the server did not prove its key")]
    ServerProofInvalid,
}

/// How one connection ended.
enum Ended {
    /// Asked to stop.
    Stopped,
    /// Try again after this long; `connected` says the connection had been
    /// accepted, so the backoff starts over.
    Retry {
        after: Option<Duration>,
        connected: bool,
    },
    Fatal(DaemonError),
}

const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_IDLE_TIMEOUT: Duration = Duration::from_secs(100);
const REPLAY_BATCH: usize = 16;
/// A pending machine checks back this often.
const PENDING_RETRY: Duration = Duration::from_secs(30);

/// Keeps the daemon connected until `stop` turns true or the server refuses
/// the machine for good.
pub async fn run(config: DaemonConfig, mut stop: watch::Receiver<bool>) -> Result<(), DaemonError> {
    let target = Target::parse(&config.server)?;
    let mut backoff = BACKOFF_MIN;
    // The machine's resources are sampled while the daemon runs, so their
    // charts have a history when someone looks.
    crate::modules::machines::metrics::daemon::start();
    // Kept across connections: live agents outlive a dropped connection.
    let daemon = std::sync::Arc::new(dispatch::DaemonState::new(
        &config.data_dir,
        config.executable.clone(),
        &config.server,
        config.plugin_node.clone(),
        &config.tools,
    ));
    // Terminals' ends go to the server like other lasting events.
    let watching = std::sync::Arc::clone(&daemon);
    let watch = tokio::spawn(async move {
        let outbox = std::sync::Arc::clone(&watching.outbox);
        watching
            .terminals
            .watch(move |event| outbox.emit("terminal.event", event))
            .await;
    });
    let idle = std::sync::Arc::downgrade(&daemon);
    let retiring = tokio::spawn(async move {
        let mut sweep = tokio::time::interval(Duration::from_mins(1));
        loop {
            sweep.tick().await;
            let Some(daemon) = idle.upgrade() else { return };
            daemon.chats.retire_unused_providers().await;
        }
    });
    let result = run_connections(&config, &mut stop, &daemon, target, &mut backoff).await;
    watch.abort();
    retiring.abort();
    result
}

async fn run_connections(
    config: &DaemonConfig,
    stop: &mut watch::Receiver<bool>,
    daemon: &std::sync::Arc<dispatch::DaemonState>,
    target: Target,
    backoff: &mut Duration,
) -> Result<(), DaemonError> {
    loop {
        let ended = connect_once(config, &target, stop, daemon).await;
        daemon.outbox.disconnected();
        let wait = match ended {
            Ended::Stopped => {
                daemon.shutdown().await;
                return Ok(());
            }
            Ended::Fatal(error) => {
                daemon.shutdown().await;
                return Err(error);
            }
            Ended::Retry { after, connected } => {
                if connected {
                    *backoff = BACKOFF_MIN;
                }
                let wait = after.unwrap_or_else(|| with_jitter(*backoff));
                *backoff = (*backoff * 2).min(BACKOFF_MAX);
                wait
            }
        };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            _ = stop.changed() => {
                daemon.shutdown().await;
                return Ok(());
            }
        }
    }
}

/// The server's WebSocket address and the host the daemon signs.
struct Target {
    url: String,
    host: String,
}

impl Target {
    fn parse(server: &str) -> Result<Self, DaemonError> {
        let invalid = || DaemonError::InvalidServer(server.to_owned());
        let mut url = Url::parse(server).map_err(|_| invalid())?;
        let scheme = match url.scheme() {
            "http" | "ws" => "ws",
            "https" | "wss" => "wss",
            _ => return Err(invalid()),
        };
        url.set_scheme(scheme).map_err(|()| invalid())?;
        url.set_path("/api/v1/daemon/connect");
        let host = host_of(&url).ok_or_else(invalid)?;
        Ok(Self {
            url: url.to_string(),
            host,
        })
    }
}

/// The host as an HTTP client sends it in its `Host` header: the host, and
/// the port when it is not the scheme's default. Joining and connecting sign
/// it, and the server checks it against the header it received.
pub fn host_of(url: &Url) -> Option<String> {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => Some(format!("{host}:{port}")),
        (Some(host), None) => Some(host.to_owned()),
        (None, _) => None,
    }
}

/// `duration`, give or take a quarter, so many daemons do not reconnect at
/// once.
fn with_jitter(duration: Duration) -> Duration {
    let random = getrandom::u32().unwrap_or(0) % 1000;
    let factor = 0.75 + (f64::from(random) / 1000.0) * 0.5;
    duration.mul_f64(factor)
}

async fn connect_once(
    config: &DaemonConfig,
    target: &Target,
    stop: &mut watch::Receiver<bool>,
    daemon: &std::sync::Arc<dispatch::DaemonState>,
) -> Ended {
    let retry = Ended::Retry {
        after: None,
        connected: false,
    };
    let socket = tokio::select! {
        connected = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(target.url.as_str())) => match connected {
            Ok(Ok((socket, _))) => socket,
            Ok(Err(error)) => {
                tracing::debug!(%error, server = %config.server, "cannot reach the server");
                return retry;
            }
            Err(_) => {
                tracing::debug!(server = %config.server, "server connection timed out");
                return retry;
            }
        },
        _ = stop.changed() => return Ended::Stopped,
    };
    let (mut sink, mut stream) = socket.split();

    // The challenge.
    let nonce = match next_handshake(&mut stream).await {
        Ok(Handshake::Challenge { nonce }) => nonce,
        Ok(_) | Err(None) => return retry,
        Err(Some(code)) => return refused(code),
    };
    let signature = config.key.sign(&connect_message(&target.host, &nonce));
    let mut challenge = [0u8; 32];
    if getrandom::fill(&mut challenge).is_err() {
        return retry;
    }
    let challenge = STANDARD.encode(challenge);
    let hello = Handshake::Hello {
        protocol: PROTOCOL,
        public_key: STANDARD.encode(config.key.public_key().as_bytes()),
        signature: STANDARD.encode(signature),
        facts: config.facts.clone(),
        challenge: challenge.clone(),
        capabilities: Some(dispatch::capabilities()),
    };
    let Ok(text) = serde_json::to_string(&hello) else {
        return retry;
    };
    if !send(&mut sink, Message::Text(text.into())).await {
        return retry;
    }
    match next_handshake(&mut stream).await {
        Ok(Handshake::Welcome {
            name,
            protocol,
            server_key,
            server_signature,
            api_token,
            ..
        }) => {
            if let Err(error) = check_server(
                config,
                &target.host,
                &challenge,
                &server_key,
                &server_signature,
            ) {
                return Ended::Fatal(error);
            }
            tracing::info!(machine = %name, protocol, server = %config.server, "connected to the server");
            // Plugins' calls to the API carry it; an older server sends none.
            daemon.plugins.set_api_token(api_token);
        }
        Ok(_) | Err(None) => return retry,
        Err(Some(code)) => return refused(code),
    }

    // Requests run concurrently; their answers come back through `answers`.
    let (answers, mut answered) = mpsc::channel::<Envelope>(64);
    daemon.outbox.connected();
    let mut running: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut frames = daemon.channels.frames.lock().await;
    let mut last_heard = tokio::time::Instant::now();
    let ended = loop {
        tokio::select! {
            Some(frame) = frames.recv() => {
                if !send(&mut sink, Message::Binary(frame.into())).await {
                    break Ended::Retry { after: None, connected: true };
                }
            }
            _ = stop.changed() => {
                let _ = send(&mut sink, Message::Close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "The daemon is stopping.".into(),
                }))).await;
                break Ended::Stopped;
            }
            () = tokio::time::sleep_until(last_heard + SERVER_IDLE_TIMEOUT) => {
                tracing::debug!(server = %config.server, "server stopped answering");
                break Ended::Retry { after: None, connected: true };
            }
            () = daemon.outbox.ready.notified() => {
                let batch = daemon.outbox.take(REPLAY_BATCH);
                let written = tokio::time::timeout(WRITE_TIMEOUT, async {
                    for event in batch {
                        let Ok(text) = serde_json::to_string(&event) else { continue };
                        sink.feed(Message::Text(text.into())).await?;
                    }
                    sink.flush().await
                }).await;
                if !matches!(written, Ok(Ok(()))) {
                    break Ended::Retry { after: None, connected: true };
                }
            }
            Some(answer) = answered.recv() => {
                if let Envelope::Response { id, .. } = &answer {
                    running.remove(id);
                }
                let Ok(text) = serde_json::to_string(&answer) else { continue };
                if !send(&mut sink, Message::Text(text.into())).await {
                    break Ended::Retry { after: None, connected: true };
                }
            }
            incoming = stream.next() => {
              last_heard = tokio::time::Instant::now();
              match incoming {
                Some(Ok(Message::Text(text))) => match serde_json::from_str::<Envelope>(&text) {
                    Ok(Envelope::Request { id, method, params, .. }) => {
                        let answers = answers.clone();
                        let request_id = id.clone();
                        let daemon = std::sync::Arc::clone(daemon);
                        let task = tokio::spawn(async move {
                            let answer = match Box::pin(dispatch::handle(&daemon, &method, params)).await {
                                Ok(result) => Envelope::Response { id: request_id, result: Some(result), error: None },
                                Err(error) => Envelope::Response { id: request_id, result: None, error: Some(error) },
                            };
                            let _ = answers.send(answer).await;
                        });
                        running.insert(id, task);
                    }
                    Ok(Envelope::Ack { seq }) => daemon.outbox.ack(seq),
                    Ok(Envelope::Cancel { id }) => {
                        if let Some(task) = running.remove(&id) {
                            task.abort();
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "unreadable message from the server"),
                },
                Some(Ok(Message::Binary(frame))) => daemon.channels.receive(&frame),
                Some(Ok(Message::Close(frame))) => {
                    break match frame.map(|frame| u16::from(frame.code)) {
                        Some(code) if code == close::REPLACED => {
                            tracing::warn!("another connection for this machine replaced this one");
                            Ended::Retry { after: Some(BACKOFF_MAX), connected: true }
                        }
                        Some(1013) => Ended::Retry { after: Some(with_jitter(Duration::from_secs(3))), connected: true },
                        Some(code) if is_refusal(code) => refused(code),
                        _ => Ended::Retry { after: None, connected: true },
                    };
                }
                Some(Ok(Message::Ping(_))) => {
                    // Tungstenite queued the matching pong on receipt.
                    if !matches!(tokio::time::timeout(WRITE_TIMEOUT, sink.flush()).await, Ok(Ok(()))) {
                        break Ended::Retry { after: None, connected: true };
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break Ended::Retry { after: None, connected: true },
              }
            },
        }
    };
    drop(frames);
    daemon.channels.close_all();
    for (_, task) in running {
        task.abort();
    }
    ended
}

async fn send(sink: &mut (impl futures_util::Sink<Message> + Unpin), message: Message) -> bool {
    matches!(
        tokio::time::timeout(WRITE_TIMEOUT, sink.send(message)).await,
        Ok(Ok(()))
    )
}

/// Checks the server's proof: a valid signature of `challenge` by the key in
/// the welcome, and, when this machine pinned one, the same key it joined.
fn check_server(
    config: &DaemonConfig,
    host: &str,
    challenge: &str,
    server_key: &str,
    server_signature: &str,
) -> Result<(), DaemonError> {
    let key = STANDARD
        .decode(server_key)
        .ok()
        .and_then(|bytes| PublicKey::from_bytes(&bytes).ok())
        .ok_or(DaemonError::ServerProofInvalid)?;
    let signature = STANDARD
        .decode(server_signature)
        .map_err(|_| DaemonError::ServerProofInvalid)?;
    if !verify(&key, &server_proof_message(host, challenge), &signature) {
        return Err(DaemonError::ServerProofInvalid);
    }
    let found = key.fingerprint().to_string();
    match &config.server_fingerprint {
        Some(expected) if *expected != found => Err(DaemonError::ServerKeyChanged {
            expected: expected.clone(),
            found,
        }),
        _ => Ok(()),
    }
}

/// The next handshake message: `Err(Some(code))` when the server closed the
/// connection with a code, `Err(None)` for anything else.
async fn next_handshake<S>(stream: &mut S) -> Result<Handshake, Option<u16>>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match tokio::time::timeout(Duration::from_secs(10), stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                return serde_json::from_str(&text).map_err(|_| None);
            }
            Ok(Some(Ok(Message::Close(frame)))) => {
                return Err(frame.map(|frame| u16::from(frame.code)));
            }
            Ok(Some(Ok(_))) => {}
            _ => return Err(None),
        }
    }
}

fn is_refusal(code: u16) -> bool {
    matches!(code, close::UNKNOWN_KEY | close::PENDING | close::OUTDATED)
}

fn refused(code: u16) -> Ended {
    match code {
        close::UNKNOWN_KEY => Ended::Fatal(DaemonError::UnknownMachine),
        close::OUTDATED => Ended::Fatal(DaemonError::Outdated),
        close::PENDING => {
            tracing::info!("this machine waits for approval on the server");
            Ended::Retry {
                after: Some(PENDING_RETRY),
                connected: false,
            }
        }
        _ => Ended::Retry {
            after: None,
            connected: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_the_server_address_into_the_socket_address() {
        let local = Target::parse("http://127.0.0.1:8180").unwrap();
        assert_eq!(local.url, "ws://127.0.0.1:8180/api/v1/daemon/connect");
        assert_eq!(local.host, "127.0.0.1:8180");

        let remote = Target::parse("https://ahab.example.internal/").unwrap();
        assert_eq!(
            remote.url,
            "wss://ahab.example.internal/api/v1/daemon/connect"
        );
        assert_eq!(remote.host, "ahab.example.internal");

        assert!(Target::parse("ftp://example").is_err());
        assert!(Target::parse("not a url").is_err());
    }

    #[test]
    fn spreads_retries_around_the_backoff() {
        for _ in 0..100 {
            let wait = with_jitter(Duration::from_secs(4));
            assert!(wait >= Duration::from_secs(3) && wait <= Duration::from_secs(5));
        }
    }
}
