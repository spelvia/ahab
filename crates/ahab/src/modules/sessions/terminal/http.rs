//! `/api/v1/terminals`: starting a terminal in a project, its state,
//! killing and restarting it, and attaching to it (a WebSocket).

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::Channel;
use crate::modules::sessions::session::api::SessionDto;
use crate::modules::sessions::terminal::api::{
    StartTerminalDto, TerminalDto, TerminalListDto, UpdateTerminalDto,
};
use crate::modules::sessions::terminal::daemon::kind;
use crate::modules::sessions::terminal::model::NewTerminal;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

/// Starts a terminal in a project: a terminal session, running the login
/// shell (or a command) in the project's folder.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/terminals",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = StartTerminalDto,
    responses(
        (status = 201, description = "The terminal's session", body = SessionDto),
        (status = 403, description = "`other_machine_not_allowed`: a machine's token, for a project on another machine", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`terminal_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `terminal_host_unavailable`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn start_terminal(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<StartTerminalDto>,
) -> Result<(StatusCode, Json<SessionDto>), ApiError> {
    if caller.machine().is_some() {
        caller.may_reach(state.projects.get(id).await?.machine_id)?;
    }
    let (session, terminal) = state
        .terminals
        .start(
            id,
            NewTerminal {
                title: body.title,
                command: body.command,
                folder: body.folder_id,
                cols: body.cols,
                rows: body.rows,
                plugin: body.plugin,
                context: body.context,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(SessionDto::new(
            &session,
            &[],
            std::slice::from_ref(&terminal),
        )),
    ))
}

/// Starts a terminal in a session, beside its chats and other terminals, in the
/// project's folder; `title` names it there.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/terminals",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The session's ID")),
    request_body = StartTerminalDto,
    responses(
        (status = 201, description = "The new terminal", body = TerminalDto),
        (status = 403, description = "`other_machine_not_allowed`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`session_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`terminal_failed`, `session_archived`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `terminal_host_unavailable`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn start_session_terminal(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<StartTerminalDto>,
) -> Result<(StatusCode, Json<TerminalDto>), ApiError> {
    if caller.machine().is_some() {
        let session = state.sessions.get(id).await?;
        caller.may_reach(state.projects.get(session.project_id).await?.machine_id)?;
    }
    let (_, terminal) = state
        .terminals
        .start_in(
            id,
            NewTerminal {
                title: body.title,
                command: body.command,
                folder: None,
                cols: body.cols,
                rows: body.rows,
                plugin: body.plugin,
                context: body.context,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(TerminalDto::from(&terminal))))
}

/// Renames a terminal, or moves it to another session of its project.
#[utoipa::path(
    patch,
    path = "/api/v1/terminals/{id}",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The terminal's ID")),
    request_body = UpdateTerminalDto,
    responses(
        (status = 200, description = "The terminal", body = TerminalDto),
        (status = 404, description = "`terminal_not_found`, `session_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`chat_in_other_project`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_terminal(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<UpdateTerminalDto>,
) -> Result<Json<TerminalDto>, ApiError> {
    let label = body.label.as_ref().map(Option::as_deref);
    let terminal = state.terminals.change(id, label, body.session_id).await?;
    Ok(Json(TerminalDto::from(&terminal)))
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RemoveQuery {
    /// Stop it first if it runs; else a running one is refused.
    #[serde(default)]
    pub stop: bool,
}

/// Removes a terminal from its session: an exited one, or a running one
/// with `stop=true`, which ends its process.
#[utoipa::path(
    delete,
    path = "/api/v1/terminals/{id}",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The terminal's ID"), RemoveQuery),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`terminal_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`terminal_running`: stop it first, or ask with `stop=true`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_terminal(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RemoveQuery>,
) -> Result<StatusCode, ApiError> {
    state.terminals.remove(id, query.stop).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A terminal's state.
#[utoipa::path(
    get,
    path = "/api/v1/terminals/{id}",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The terminal's ID")),
    responses(
        (status = 200, description = "The terminal", body = TerminalDto),
        (status = 403, description = "`other_machine_not_allowed`: a machine's token, for another machine's terminal", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`terminal_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_terminal(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<TerminalDto>, ApiError> {
    let terminal = state.terminals.get(id).await?;
    caller.may_reach(terminal.machine_id)?;
    Ok(Json(TerminalDto::from(&terminal)))
}

/// Ends a terminal's process; the terminal stays, exited.
#[utoipa::path(
    post,
    path = "/api/v1/terminals/{id}/kill",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The terminal's ID")),
    responses(
        (status = 200, description = "The terminal", body = TerminalDto),
        (status = 403, description = "`other_machine_not_allowed`: a machine's token, for another machine's terminal", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`terminal_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn kill_terminal(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<TerminalDto>, ApiError> {
    if caller.machine().is_some() {
        caller.may_reach(state.terminals.get(id).await?.machine_id)?;
    }
    Ok(Json(TerminalDto::from(&state.terminals.kill(id).await?)))
}

/// Runs a terminal's shell (or command) again.
#[utoipa::path(
    post,
    path = "/api/v1/terminals/{id}/restart",
    tag = "sessions.terminal",
    params(("id" = Uuid, Path, description = "The terminal's ID")),
    responses(
        (status = 200, description = "The terminal", body = TerminalDto),
        (status = 404, description = "`terminal_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`session_archived`, `terminal_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn restart_terminal(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<TerminalDto>, ApiError> {
    Ok(Json(TerminalDto::from(&state.terminals.restart(id).await?)))
}

/// Attaches to a terminal (a WebSocket): binary frames are its output, text
/// frames its events (`{"type":"reset"}`, `size`, `exited`); binary frames
/// back are input, `{"type":"resize","cols":…,"rows":…}` resizes it.
/// Revoking the client closes it at once.
async fn attach(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let channel = state.terminals.attach(id).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        caller.until_revoked(bridge(socket, channel)).await;
    }))
}

/// Carries a browser's attachment to the terminal's channel and back.
async fn bridge(socket: WebSocket, mut channel: Channel) {
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            from_machine = channel.recv() => {
                let Some(message) = from_machine else { break };
                let Some((&frame, payload)) = message.split_first() else { continue };
                let sent = match frame {
                    kind::OUTPUT => sink.send(Message::Binary(payload.to_vec().into())).await,
                    kind::EVENT => {
                        let text = String::from_utf8_lossy(payload).into_owned();
                        sink.send(Message::Text(text.into())).await
                    }
                    _ => Ok(()),
                };
                if sent.is_err() {
                    break;
                }
            }
            from_browser = stream.next() => {
                let message = match from_browser {
                    Some(Ok(Message::Binary(bytes))) => [&[kind::INPUT][..], &bytes].concat(),
                    Some(Ok(Message::Text(text))) => {
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                            continue;
                        };
                        if value["type"] != "resize" {
                            continue;
                        }
                        [&[kind::RESIZE][..], text.as_bytes()].concat()
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => continue,
                };
                if !channel.send(&message).await {
                    break;
                }
            }
        }
    }
    let _ = sink.send(Message::Close(None)).await;
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TerminalsQuery {
    /// Only those this plugin started.
    pub plugin: String,
}

/// The terminals a plugin started, oldest first. A machine's token sees
/// only its own machine's.
#[utoipa::path(
    get,
    path = "/api/v1/terminals",
    tag = "sessions.terminal",
    params(TerminalsQuery),
    responses((status = 200, description = "The terminals", body = TerminalListDto))
)]
pub async fn list_terminals(
    State(state): State<AppState>,
    caller: Caller,
    QueryParams(query): QueryParams<TerminalsQuery>,
) -> Result<Json<TerminalListDto>, ApiError> {
    let items = state
        .terminals
        .by_plugin(&query.plugin)
        .await?
        .iter()
        .filter(|terminal| caller.may_reach(terminal.machine_id).is_ok())
        .map(TerminalDto::from)
        .collect();
    Ok(Json(TerminalListDto { items }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_terminals))
        .routes(routes!(start_terminal))
        .routes(routes!(start_session_terminal))
        .routes(routes!(get_terminal, update_terminal, remove_terminal))
        .routes(routes!(kill_terminal))
        .routes(routes!(restart_terminal))
}

/// The attachment WebSocket, outside the `OpenAPI` document.
pub fn socket_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/v1/terminals/{id}/attach", get(attach))
}
