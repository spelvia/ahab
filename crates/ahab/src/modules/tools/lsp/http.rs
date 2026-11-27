//! `/api/v1/machines/{id}/language-servers`: what a machine runs, its
//! logs and restarting; and `/api/v1/projects/{id}/lsp`, a window's
//! channel to its project's language servers (a WebSocket).

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use futures_util::{Sink, SinkExt, StreamExt};
use tokio::time::Instant;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::Channel;
use crate::modules::tools::lsp::api::{LanguageServerListDto, ServerLogDto};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, PathParams};
use crate::server::state::AppState;

/// The language servers a machine runs: their root, state and memory.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/language-servers",
    tag = "tools.lsp",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "Its servers", body = LanguageServerListDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_language_servers(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<LanguageServerListDto>, ApiError> {
    Ok(Json(state.language_servers.servers(id).await?))
}

/// Restarts a language server, with the files open in it.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/language-servers/{server}/restart",
    tag = "tools.lsp",
    params(("id" = Uuid, Path, description = "The machine's ID"), ("server" = Uuid, Path, description = "The server's ID")),
    responses(
        (status = 204, description = "Restarted"),
        (status = 404, description = "`machine_not_found`, `language_server_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn restart_language_server(
    State(state): State<AppState>,
    PathParams((id, server)): PathParams<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state.language_servers.restart(id, server).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A language server's log: what it printed, and its LSP traffic when the
/// machine keeps it.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/language-servers/{server}/log",
    tag = "tools.lsp",
    params(("id" = Uuid, Path, description = "The machine's ID"), ("server" = Uuid, Path, description = "The server's ID")),
    responses(
        (status = 200, description = "Its log", body = ServerLogDto),
        (status = 404, description = "`machine_not_found`, `language_server_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn language_server_log(
    State(state): State<AppState>,
    PathParams((id, server)): PathParams<(Uuid, Uuid)>,
) -> Result<Json<ServerLogDto>, ApiError> {
    Ok(Json(state.language_servers.log(id, server).await?))
}

/// A window's channel to a project's language servers (a WebSocket of
/// JSON text frames both ways; see `lsp::api`). Revoking the client closes
/// it at once.
async fn connect(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    if caller.machine().is_some() {
        caller.may_reach(state.language_servers.machine_of(id).await?)?;
    }
    let (channel, _) = state.language_servers.connect(id).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        caller.until_revoked(bridge(socket, channel)).await;
    }))
}

/// Carries a window's frames to its daemon's channel and back.
async fn bridge(socket: WebSocket, mut channel: Channel) {
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    ping.tick().await;
    let mut last_seen = Instant::now();
    loop {
        tokio::select! {
            from_machine = channel.recv() => {
                let Some(message) = from_machine else { break };
                let text = String::from_utf8_lossy(&message).into_owned();
                if !send(&mut sink, Message::Text(text.into())).await {
                    break;
                }
            }
            from_browser = stream.next() => {
                last_seen = Instant::now();
                let bytes = match from_browser {
                    Some(Ok(Message::Text(text))) => text.as_bytes().to_vec(),
                    Some(Ok(Message::Binary(bytes))) => bytes.to_vec(),
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => continue,
                };
                if !matches!(tokio::time::timeout(Duration::from_secs(10), channel.send(&bytes)).await, Ok(true)) {
                    break;
                }
            }
            _ = ping.tick() => {
                if last_seen.elapsed() >= Duration::from_secs(90)
                    || !send(&mut sink, Message::Ping(Vec::new().into())).await
                {
                    break;
                }
            }
        }
    }
    drop(channel);
    let _ = send(&mut sink, Message::Close(None)).await;
}

async fn send(sink: &mut (impl Sink<Message> + Unpin), message: Message) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(10), sink.send(message)).await,
        Ok(Ok(()))
    )
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_language_servers))
        .routes(routes!(restart_language_server))
        .routes(routes!(language_server_log))
}

/// The window channel's WebSocket, outside the `OpenAPI` document.
pub fn socket_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/v1/projects/{id}/lsp", get(connect))
}
