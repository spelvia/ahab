//! `POST /api/v1/plugins/{id}/commands/{name}`.

use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::machines::Channel;
use crate::modules::plugins::command::api::{CommandResultDto, RunCommandDto};
use crate::modules::plugins::command::service::Target;
use crate::modules::plugins::host::model::END;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;
use crate::shared::error::{FieldError, ServiceError};

/// Runs a plugin's command on a machine: the one given, the project's
/// (which also gives the command its folder), or the only one the plugin
/// runs on. Closing the request cancels the command. A machine's token
/// (its plugins calling) runs commands only on its own machine; without a
/// machine or project, that is where they run.
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{id}/commands/{name}",
    tag = "plugins.runtime",
    params(
        ("id" = String, Path, description = "The plugin's ID"),
        ("name" = String, Path, description = "The command's name in the plugin, such as `containers.list`"),
    ),
    request_body = RunCommandDto,
    responses(
        (status = 200, description = "What the command answered", body = CommandResultDto),
        (status = 403, description = "`other_machine_not_allowed`: a machine's token, for another machine or its project", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`plugin_not_found`, `command_unknown`, `project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_disabled`, `plugin_not_running`, `command_failed` (with the plugin's message, and `pluginCode` when the plugin gave a code)", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: input the command refuses, or which machine is unclear", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `plugin_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn run_plugin_command(
    State(state): State<AppState>,
    caller: Caller,
    PathParams((id, name)): PathParams<(String, String)>,
    JsonBody(body): JsonBody<RunCommandDto>,
) -> Result<Json<CommandResultDto>, ApiError> {
    let target = match (body.machine, body.project) {
        (_, Some(project)) => Target::Project(project),
        (Some(machine), None) => Target::Machine(machine),
        (None, None) => Target::Anywhere,
    };
    let target = match (caller.machine(), target) {
        (None, target) => target,
        // A machine's plugins reach their own machine only.
        (Some(own), Target::Anywhere) => Target::Machine(own),
        (Some(_), Target::Machine(machine)) => {
            caller.may_reach(machine)?;
            target
        }
        (Some(_), Target::Project(project)) => {
            caller.may_reach(state.projects.get(project).await?.machine_id)?;
            target
        }
    };
    let output = state.commands.run(&id, &name, body.input, target).await?;
    Ok(Json(CommandResultDto { output }))
}

#[derive(Debug, serde::Deserialize)]
pub struct ChannelQuery {
    pub machine: Option<uuid::Uuid>,
    pub project: Option<uuid::Uuid>,
    /// The channel's params, as JSON.
    pub params: Option<String>,
}

/// Opens a plugin's channel (a WebSocket): binary frames both ways are its
/// bytes; a text frame `{"type":"end","reason":…}` says the plugin closed it.
/// Revoking the client closes it at once.
async fn open_channel(
    State(state): State<AppState>,
    caller: Caller,
    PathParams((id, name)): PathParams<(String, String)>,
    QueryParams(query): QueryParams<ChannelQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let params: serde_json::Value = match query.params.as_deref() {
        Some(text) => serde_json::from_str(text).map_err(|error| {
            ServiceError::Invalid(vec![FieldError::new("params", error.to_string())])
        })?,
        None => serde_json::json!({}),
    };
    let target = match (query.machine, query.project) {
        (_, Some(project)) => Target::Project(project),
        (Some(machine), None) => Target::Machine(machine),
        (None, None) => Target::Anywhere,
    };
    let channel = state
        .commands
        .open_channel(&id, &name, params, target)
        .await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        caller.until_revoked(bridge(socket, channel)).await;
    }))
}

/// Carries a browser's channel to the plugin's and back.
async fn bridge(socket: WebSocket, mut channel: Channel) {
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            from_machine = channel.recv() => {
                let Some(message) = from_machine else { break };
                let Some((&kind, payload)) = message.split_first() else { continue };
                if kind == END {
                    let reason = String::from_utf8_lossy(payload).into_owned();
                    let text = serde_json::json!({ "type": "end", "reason": reason }).to_string();
                    let _ = sink.send(Message::Text(text.into())).await;
                    break;
                }
                if sink.send(Message::Binary(payload.to_vec().into())).await.is_err() {
                    break;
                }
            }
            from_browser = stream.next() => {
                let bytes = match from_browser {
                    Some(Ok(Message::Binary(bytes))) => bytes.to_vec(),
                    Some(Ok(Message::Text(text))) => text.as_bytes().to_vec(),
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => continue,
                };
                if !channel.send(&bytes).await {
                    break;
                }
            }
        }
    }
    let _ = sink.send(Message::Close(None)).await;
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(run_plugin_command))
}

/// The channel WebSocket, outside the `OpenAPI` document.
pub fn socket_routes() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/api/v1/plugins/{id}/channels/{name}",
        axum::routing::get(open_channel),
    )
}
