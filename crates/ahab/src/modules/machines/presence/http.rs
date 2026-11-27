//! `GET /api/v1/daemon/connect`: the WebSocket a daemon keeps open to its
//! server. Not in the `OpenAPI` document: its protocol is in `api.rs`.

use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::http::header::HOST;
use axum::response::Response;
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};

use crate::modules::machines::presence::api::{Envelope, Handshake, close};
use crate::modules::machines::presence::service::{Capabilities, Hello, Outgoing};
use crate::server::state::AppState;

/// How long a daemon has to answer the challenge.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the server pings, and records the machine as seen.
const PING_EVERY: Duration = Duration::from_secs(30);
/// A connection without a pong for this long is closed.
const DEAD_AFTER: Duration = Duration::from_secs(90);

pub async fn connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    // Large enough for an attachment (10 MiB, base64-encoded) in one answer.
    upgrade
        .max_message_size(24 * 1024 * 1024)
        .max_frame_size(24 * 1024 * 1024)
        .on_upgrade(move |socket| session(socket, state, host))
}

async fn session(mut socket: WebSocket, state: AppState, host: String) {
    let mut nonce = [0u8; 32];
    if getrandom::fill(&mut nonce).is_err() {
        close_with(&mut socket, 1011, "The server cannot create a challenge.").await;
        return;
    }
    let nonce = STANDARD.encode(nonce);
    if send_json(
        &mut socket,
        &Handshake::Challenge {
            nonce: nonce.clone(),
        },
    )
    .await
    .is_err()
    {
        return;
    }

    let hello = match tokio::time::timeout(HELLO_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<Handshake>(&text).ok(),
        _ => None,
    };
    let Some(Handshake::Hello {
        protocol,
        public_key,
        signature,
        facts,
        challenge,
        capabilities,
    }) = hello
    else {
        close_with(&mut socket, close::PROTOCOL_ERROR, "Expected a hello.").await;
        return;
    };
    let hello = Hello {
        protocol,
        public_key,
        signature,
        facts,
        challenge,
        capabilities,
    };

    let presence = &state.presence;
    let (machine, protocol) = match presence.authenticate(&hello, &host, &nonce).await {
        Ok(accepted) => accepted,
        Err(rejection) => {
            tracing::info!(code = rejection.code, reason = %rejection.reason, "daemon refused");
            close_with(&mut socket, rejection.code, &rejection.reason).await;
            return;
        }
    };
    let capabilities = Capabilities::from_hello(hello.capabilities.as_deref());
    let mut registration = match presence
        .connect(&machine, &hello.facts, protocol, capabilities)
        .await
    {
        Ok(registration) => registration,
        Err(error) => {
            tracing::error!(%error, "cannot record the daemon's connection");
            close_with(
                &mut socket,
                1011,
                "The server cannot record this connection.",
            )
            .await;
            return;
        }
    };
    let (server_key, server_signature) = presence.prove(&host, &hello.challenge);
    let welcome = Handshake::Welcome {
        machine_id: machine.id,
        name: machine.name.to_string(),
        protocol: registration.protocol,
        server_key,
        server_signature,
        api_token: Some(registration.api_token.clone()),
    };
    if send_json(&mut socket, &welcome).await.is_ok() {
        run(&state, socket, machine.id, &mut registration).await;
    }
    presence
        .disconnect(machine.id, registration.generation)
        .await;
}

/// Passes messages both ways until either side ends the connection.
async fn run(
    state: &AppState,
    socket: WebSocket,
    machine: uuid::Uuid,
    registration: &mut crate::modules::machines::presence::service::Registration,
) {
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let mut last_pong = Instant::now();
    let mut ack_open = true;
    loop {
        tokio::select! {
            changed = registration.acknowledged.changed(), if ack_open => {
                if changed.is_err() {
                    ack_open = false;
                    continue;
                }
                let seq = *registration.acknowledged.borrow_and_update();
                let Ok(text) = serde_json::to_string(&Envelope::Ack { seq }) else { continue };
                if !send(&mut sink, Message::Text(text.into())).await {
                    return;
                }
            }
            () = registration.retry.notified() => {
                retry_events(&mut sink).await;
                return;
            }
            outgoing = registration.outgoing.recv() => match outgoing {
                Some(Outgoing::Envelope(envelope)) => {
                    let Ok(text) = serde_json::to_string(&envelope) else { continue };
                    if !send(&mut sink, Message::Text(text.into())).await {
                        return;
                    }
                }
                Some(Outgoing::Frame(frame)) => {
                    if !send(&mut sink, Message::Binary(frame.into())).await {
                        return;
                    }
                }
                Some(Outgoing::Close(code, reason)) => {
                    let frame = CloseFrame { code, reason: Utf8Bytes::from(reason) };
                    let _ = send(&mut sink, Message::Close(Some(frame))).await;
                    return;
                }
                None => return,
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => match serde_json::from_str::<Envelope>(&text) {
                    Ok(envelope) => {
                        if !state.presence.receive(machine, registration.generation, envelope, text.len()) {
                            tracing::warn!(%machine, "daemon event backlog full or connection replaced; reconnecting for replay");
                            retry_events(&mut sink).await;
                            return;
                        }
                    }
                    Err(error) => tracing::warn!(%machine, %error, "unreadable message from a daemon"),
                },
                Some(Ok(Message::Binary(frame))) => {
                    state.presence.receive_frame(machine, registration.generation, &frame);
                }
                Some(Ok(Message::Pong(_))) => last_pong = Instant::now(),
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if last_pong.elapsed() > DEAD_AFTER {
                    tracing::info!(%machine, "daemon stopped answering");
                    return;
                }
                if !send(&mut sink, Message::Ping(Vec::new().into())).await {
                    return;
                }
                state.presence.heartbeat(machine).await;
            }
        }
    }
}

async fn retry_events(sink: &mut (impl futures_util::Sink<Message> + Unpin)) {
    let frame = CloseFrame {
        code: 1013,
        reason: Utf8Bytes::from_static("Try again for event replay"),
    };
    let _ = send(sink, Message::Close(Some(frame))).await;
}

async fn send(sink: &mut (impl futures_util::Sink<Message> + Unpin), message: Message) -> bool {
    matches!(
        tokio::time::timeout(WRITE_TIMEOUT, sink.send(message)).await,
        Ok(Ok(()))
    )
}

async fn send_json(socket: &mut WebSocket, message: &Handshake) -> Result<(), ()> {
    let text = serde_json::to_string(message).map_err(|_| ())?;
    send(socket, Message::Text(text.into()))
        .await
        .then_some(())
        .ok_or(())
}

async fn close_with(socket: &mut WebSocket, code: u16, reason: &str) {
    let frame = CloseFrame {
        code,
        reason: Utf8Bytes::from(reason),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

/// This component's route. A WebSocket, so a plain route: the `OpenAPI`
/// document describes request and response bodies, not this protocol.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/daemon/connect", get(connect))
}
