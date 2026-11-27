//! `/api/v1/sessions`: start sessions, talk to their agents, answer their
//! requests, and watch them live.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::json;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::sessions::chat::api::{
    AgentHistoryDto, AnswerDto, ChatDto, ChatListDto, ChatModelDto, ChatStopReceipt,
    ChatStopTarget, ChatTurnDto, ChatTurnsDto, ChatUsageDto, DefaultsDto, DefaultsListDto,
    EditedFilesDto, EventDto, EventListDto, ForkChatDto, MessageDto, ModelListDto, NextModelDto,
    QueueOrderDto, QueuedEditDto, SaveDefaultsDto, SessionSearchDto, SetModeDto, SetModelDto,
    StartChatDto, TaskOutputDto, TaskOutputQuery, UpdateChatDto,
};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

/// Events per page.
const PAGE: u64 = 500;

/// A session's chats, most recently changed first.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/chats",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 200, description = "The chats", body = ChatListDto),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_chats(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatListDto>, ApiError> {
    let chats = state.chats.list(id).await?;
    Ok(Json(ChatListDto {
        items: chats.iter().map(ChatDto::from).collect(),
    }))
}

/// Starts another chat in a session, beside its other chats and terminals: the
/// same as a new session's first, filed here.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/chats",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The session's ID")),
    request_body = StartChatDto,
    responses(
        (status = 201, description = "The new chat", body = ChatDto),
        (status = 404, description = "`session_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`agent_not_ready`, `session_archived`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn start_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<StartChatDto>,
) -> Result<(StatusCode, Json<ChatDto>), ApiError> {
    let chat = state
        .chats
        .start(
            id,
            &body.agent,
            &body.message,
            body.model.as_deref(),
            body.effort.as_deref(),
            body.mode.as_deref(),
            body.fast,
            &body.attachments,
            &body.references,
            body.command.as_ref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ChatDto::from(&chat))))
}

#[utoipa::path(get, path = "/api/v1/projects/{project}/agents/{agent}/commands", tag = "sessions.chat",
    params(("project" = Uuid, Path), ("agent" = String, Path)), responses((status = 200, description = "First-message commands for this project and agent", body = Vec<super::api::AgentCommand>)))]
pub async fn list_project_commands(
    State(state): State<AppState>,
    PathParams((project, agent)): PathParams<(Uuid, String)>,
) -> Result<Json<Vec<super::api::AgentCommand>>, ApiError> {
    Ok(Json(state.chats.project_commands(project, &agent).await?))
}

#[utoipa::path(get, path = "/api/v1/chats/{id}/commands", tag = "sessions.chat",
    params(("id" = Uuid, Path)), responses((status = 200, description = "Commands available in this live session", body = Vec<super::api::AgentCommand>)))]
pub async fn list_chat_commands(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<Vec<super::api::AgentCommand>>, ApiError> {
    Ok(Json(state.chats.commands(id).await?))
}

/// The live session's provider-defined modes.
#[utoipa::path(get, path = "/api/v1/chats/{id}/modes", tag = "sessions.chat", params(("id" = Uuid, Path)), responses((status = 200, description = "Current and available modes", body = super::api::SessionModes)))]
pub async fn get_chat_modes(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<super::api::SessionModes>, ApiError> {
    Ok(Json(state.chats.modes(id).await?))
}

/// Forks a completed turn into a separate provider conversation.
#[utoipa::path(post, path = "/api/v1/chats/{id}/fork", tag = "sessions.chat",
    params(("id" = Uuid, Path)), request_body = ForkChatDto,
    responses((status = 201, description = "The forked chat", body = ChatDto)))]
pub async fn fork_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<ForkChatDto>,
) -> Result<(StatusCode, Json<ChatDto>), ApiError> {
    let chat = state.chats.fork(id, body.seq, body.new_session).await?;
    Ok((StatusCode::CREATED, Json(ChatDto::from(&chat))))
}

/// Stop one background task or subagent in a live session.
#[utoipa::path(post, path = "/api/v1/chats/{id}/stop-work", tag = "sessions.chat",
    params(("id" = Uuid, Path)), request_body = ChatStopTarget,
    responses((status = 200, description = "The native stop receipt", body = ChatStopReceipt)))]
pub async fn stop_chat_work(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(target): JsonBody<ChatStopTarget>,
) -> Result<Json<ChatStopReceipt>, ApiError> {
    Ok(Json(state.chats.stop_work(id, target).await?))
}

/// A page of one of the chat's background tasks' output. Read on from
/// `nextCursor` to follow a log still being written.
#[utoipa::path(get, path = "/api/v1/chats/{id}/tasks/{task}/output", tag = "sessions.chat",
    params(("id" = Uuid, Path), ("task" = String, Path), TaskOutputQuery),
    responses(
        (status = 200, description = "A page of the task's output", body = TaskOutputDto),
        (status = 409, description = "The agent cannot show a task's output", body = Problem),
    ))]
pub async fn get_task_output(
    State(state): State<AppState>,
    PathParams((id, task)): PathParams<(Uuid, String)>,
    QueryParams(query): QueryParams<TaskOutputQuery>,
) -> Result<Json<TaskOutputDto>, ApiError> {
    let page = state
        .chats
        .task_output(id, &task, query.cursor, query.limit)
        .await?;
    Ok(Json(page))
}

/// One of the chat's subagents' conversation so far, as the events that
/// show it; what it does next comes on the chat's stream (`agentActivity`).
#[utoipa::path(get, path = "/api/v1/chats/{id}/agents/{agent}/history", tag = "sessions.chat",
    params(("id" = Uuid, Path), ("agent" = String, Path)),
    responses(
        (status = 200, description = "The subagent's conversation", body = AgentHistoryDto),
        (status = 409, description = "The agent cannot show its subagents' conversations", body = Problem),
    ))]
pub async fn get_agent_history(
    State(state): State<AppState>,
    PathParams((id, agent)): PathParams<(Uuid, String)>,
) -> Result<Json<AgentHistoryDto>, ApiError> {
    let events = state.chats.agent_history(id, &agent).await?;
    Ok(Json(AgentHistoryDto { events }))
}

/// Renames a chat, or moves it to another session of its project.
#[utoipa::path(
    patch,
    path = "/api/v1/chats/{id}",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    request_body = UpdateChatDto,
    responses(
        (status = 200, description = "The chat", body = ChatDto),
        (status = 404, description = "`chat_not_found`, `session_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`chat_in_other_project`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<UpdateChatDto>,
) -> Result<Json<ChatDto>, ApiError> {
    let chat = state
        .chats
        .change(id, body.title.as_deref(), body.session_id)
        .await?;
    Ok(Json(ChatDto::from(&chat)))
}

/// Removes a stopped chat from its session, with its history.
#[utoipa::path(
    delete,
    path = "/api/v1/chats/{id}",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`chat_live`: stop it first", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.chats.remove(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The files a session's agents edited, from their recorded file changes.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/edited-files",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 200, description = "The files, as the agents named them", body = EditedFilesDto),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_edited_files(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<EditedFilesDto>, ApiError> {
    Ok(Json(EditedFilesDto {
        paths: state.chats.edited_files(id).await?,
    }))
}

/// What a chat's agent has spent so far: tokens and cost, as it reported them.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}/usage",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 200, description = "What it spent", body = ChatUsageDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_chat_usage(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatUsageDto>, ApiError> {
    let (tokens, cost_usd) = state.chats.usage(id).await?;
    Ok(Json(ChatUsageDto { tokens, cost_usd }))
}

/// A chat's turns, all of them, whatever part of its history is loaded.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}/turns",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 200, description = "Its turns", body = ChatTurnsDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_chat_turns(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatTurnsDto>, ApiError> {
    let items = state
        .chats
        .turns(id)
        .await?
        .into_iter()
        .map(|(seq, text)| ChatTurnDto { seq, text })
        .collect();
    Ok(Json(ChatTurnsDto { items }))
}

/// The models an agent offers on a machine, with their reasoning efforts.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/agents/{agent}/models",
    tag = "sessions.chat",
    params(
        ("id" = Uuid, Path, description = "The machine's ID"),
        ("agent" = String, Path, description = "`claude-code`, `codex`, `pi`, or an added agent"),
        ModelsQuery,
    ),
    responses(
        (status = 200, description = "The models", body = ModelListDto),
        (status = 409, description = "`machine_request_failed`: the agent cannot list them", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_models(
    State(state): State<AppState>,
    PathParams((id, agent)): PathParams<(Uuid, String)>,
    QueryParams(query): QueryParams<ModelsQuery>,
) -> Result<Json<ModelListDto>, ApiError> {
    let (items, fetched_at, refreshing) = state.chats.models(id, &agent, query.refresh).await?;
    Ok(Json(ModelListDto {
        items,
        fetched_at,
        refreshing,
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ModelsQuery {
    /// Ask the agent now instead of answering with the models Ahab keeps.
    #[serde(default)]
    refresh: bool,
}

/// One session.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 200, description = "The chat", body = ChatDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatDto>, ApiError> {
    Ok(Json(ChatDto::from(&state.chats.get(id).await?)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AfterQuery {
    /// Events after this number; 0 for all.
    #[serde(default)]
    pub after: i64,
    /// Read a page of earlier events before this sequence, or 0 for the latest page.
    pub before: Option<i64>,
    /// Cap the recent page even when the turn continues into earlier pages.
    #[serde(default)]
    pub bounded: bool,
}

/// A session's kept events, oldest first, up to 500 at a time.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}/events",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID"), AfterQuery),
    responses(
        (status = 200, description = "The events", body = EventListDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_events(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<AfterQuery>,
) -> Result<Json<EventListDto>, ApiError> {
    let (events, more, history_before) = if let Some(before) = query.before {
        let (events, more) = state.chats.history(id, before, query.bounded).await?;
        let next = if more {
            events.first().map_or(0, |event| event.seq)
        } else {
            0
        };
        (events, more, Some(next))
    } else {
        let mut events = state.chats.events(id, query.after, PAGE + 1).await?;
        let more = events.len() as u64 > PAGE;
        events.truncate(usize::try_from(PAGE).unwrap_or(usize::MAX));
        (events, more, None)
    };
    Ok(Json(EventListDto {
        items: events
            .into_iter()
            .map(|event| EventDto {
                seq: event.seq,
                event: event.event,
                created_at: event.created_at,
            })
            .collect(),
        more,
        history_before,
    }))
}

/// Sends a message to the session's agent.
#[utoipa::path(
    post,
    path = "/api/v1/chats/{id}/messages",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    request_body = MessageDto,
    responses(
        (status = 202, description = "Accepted for delivery, including a matching retry"),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`chat_closed`, `submission_changed`, `message_unconfirmed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn send_message(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<MessageDto>,
) -> Result<StatusCode, ApiError> {
    state.chats.send(id, body).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Sends a waiting message next: the agent's running turn is stopped for it.
#[utoipa::path(
    post,
    path = "/api/v1/chats/{id}/queue/{item}/now",
    tag = "sessions.chat",
    params(
        ("id" = Uuid, Path, description = "The chat's ID"),
        ("item" = Uuid, Path, description = "The waiting message's ID"),
    ),
    responses(
        (status = 202, description = "Sent next"),
        (status = 404, description = "`chat_not_found`, `queued_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`chat_closed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn send_queued_now(
    State(state): State<AppState>,
    PathParams((id, item)): PathParams<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state.chats.send_now(id, item).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Puts the messages waiting for the agent's turn in a new order.
#[utoipa::path(
    put,
    path = "/api/v1/chats/{id}/queue",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    request_body = QueueOrderDto,
    responses(
        (status = 204, description = "Reordered"),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn reorder_queue(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<QueueOrderDto>,
) -> Result<StatusCode, ApiError> {
    state.chats.reorder_queue(id, &body.order).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Changes the text of a message waiting for the agent's turn to end.
#[utoipa::path(
    patch,
    path = "/api/v1/chats/{id}/queue/{item}",
    tag = "sessions.chat",
    params(
        ("id" = Uuid, Path, description = "The chat's ID"),
        ("item" = Uuid, Path, description = "The waiting message's ID"),
    ),
    request_body = QueuedEditDto,
    responses(
        (status = 204, description = "Changed"),
        (status = 404, description = "`chat_not_found`, `queued_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`queued_command`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn edit_queued_message(
    State(state): State<AppState>,
    PathParams((id, item)): PathParams<(Uuid, Uuid)>,
    JsonBody(body): JsonBody<QueuedEditDto>,
) -> Result<StatusCode, ApiError> {
    state.chats.get(id).await?;
    state.chats.edit_queued(id, item, &body.text).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Takes back a message waiting for the agent's turn to end.
#[utoipa::path(
    delete,
    path = "/api/v1/chats/{id}/queue/{item}",
    tag = "sessions.chat",
    params(
        ("id" = Uuid, Path, description = "The chat's ID"),
        ("item" = Uuid, Path, description = "The waiting message's ID"),
    ),
    responses(
        (status = 204, description = "Taken back"),
        (status = 404, description = "`chat_not_found`, `queued_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn unqueue_message(
    State(state): State<AppState>,
    PathParams((id, item)): PathParams<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state.chats.get(id).await?;
    state.chats.unqueue(id, item).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Changes what the agent may do without asking. A running Claude Code
/// switches at once, Codex from its next turn; an ended chat resumes in it.
#[utoipa::path(
    put,
    path = "/api/v1/chats/{id}/mode",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    request_body = SetModeDto,
    responses(
        (status = 200, description = "The chat", body = ChatDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_chat_mode(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<SetModeDto>,
) -> Result<Json<ChatDto>, ApiError> {
    let chat = state.chats.set_mode(id, &body.mode).await?;
    Ok(Json(ChatDto::from(&chat)))
}

fn chat_model(
    chat: &crate::modules::sessions::chat::model::Chat,
    fast: Option<bool>,
    next: Option<crate::modules::sessions::chat::service::NextModel>,
) -> ChatModelDto {
    ChatModelDto {
        model: chat.model.clone(),
        effort: chat.effort.clone(),
        fast,
        next: next.map(|next| NextModelDto {
            model: next.model,
            effort: next.effort,
            fast: next.fast,
        }),
    }
}

/// A chat's model and effort, and the ones it goes on with next.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}/model",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 200, description = "Its model", body = ChatModelDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_chat_model(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatModelDto>, ApiError> {
    let chat = state.chats.get(id).await?;
    let fast = state.chats.fast(id).await?;
    Ok(Json(chat_model(&chat, fast, state.chats.next_model(id))))
}

/// Chooses another model (or effort) of the chat's own agent: a stopped
/// chat resumes with it; a live one goes on with it from its next message,
/// its agent's own session resumed with it. The agent never changes.
#[utoipa::path(
    put,
    path = "/api/v1/chats/{id}/model",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    request_body = SetModelDto,
    responses(
        (status = 200, description = "Its model", body = ChatModelDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`model_fixed`: Pi", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_chat_model(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<SetModelDto>,
) -> Result<Json<ChatModelDto>, ApiError> {
    let (chat, next) = state
        .chats
        .set_model(id, body.model.as_deref(), body.effort.as_deref(), body.fast)
        .await?;
    let fast = state.chats.fast(id).await?;
    Ok(Json(chat_model(&chat, fast, next)))
}

/// Every agent's defaults: the model, effort and permission mode a new
/// chat starts with, the last ones picked.
#[utoipa::path(
    get,
    path = "/api/v1/agent-defaults",
    tag = "sessions.chat",
    responses((status = 200, description = "The defaults", body = DefaultsListDto))
)]
pub async fn list_agent_defaults(
    State(state): State<AppState>,
) -> Result<Json<DefaultsListDto>, ApiError> {
    let defaults = state.chats.defaults().await?;
    Ok(Json(DefaultsListDto {
        items: defaults.iter().map(DefaultsDto::from).collect(),
    }))
}

/// Replaces an agent's defaults.
#[utoipa::path(
    put,
    path = "/api/v1/agent-defaults/{agent}",
    tag = "sessions.chat",
    params(("agent" = String, Path, description = "`claude-code`, `codex` or `pi`")),
    request_body = SaveDefaultsDto,
    responses(
        (status = 200, description = "Saved", body = DefaultsDto),
        (status = 404, description = "`agent_unknown`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn save_agent_defaults(
    State(state): State<AppState>,
    PathParams(agent): PathParams<String>,
    JsonBody(body): JsonBody<SaveDefaultsDto>,
) -> Result<Json<DefaultsDto>, ApiError> {
    let defaults = state
        .chats
        .save_defaults(
            &agent,
            body.model.as_deref(),
            body.effort.as_deref(),
            body.mode.as_deref(),
        )
        .await?;
    Ok(Json(DefaultsDto::from(&defaults)))
}

/// Stops the turn the agent is working on.
#[utoipa::path(
    post,
    path = "/api/v1/chats/{id}/interrupt",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 202, description = "Asked to stop"),
        (status = 409, description = "`chat_closed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn interrupt_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.chats.interrupt(id).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Answers a request: a permission, a question, a plan or a form. An
/// answer that does not fit the request is `invalid_decision`, and the
/// request stays open.
#[utoipa::path(
    post,
    path = "/api/v1/chats/{id}/requests/{request}",
    tag = "sessions.chat",
    params(
        ("id" = Uuid, Path, description = "The chat's ID"),
        ("request" = String, Path, description = "The request's ID"),
    ),
    request_body = AnswerDto,
    responses(
        (status = 202, description = "Answered"),
        (status = 409, description = "`chat_closed`, `request_not_open`, `invalid_decision`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn answer_request(
    State(state): State<AppState>,
    PathParams((id, request)): PathParams<(Uuid, String)>,
    JsonBody(body): JsonBody<AnswerDto>,
) -> Result<StatusCode, ApiError> {
    state.chats.respond(id, &request, &body).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Ends a live session; its history is kept.
#[utoipa::path(
    post,
    path = "/api/v1/chats/{id}/close",
    tag = "sessions.chat",
    params(("id" = Uuid, Path, description = "The chat's ID")),
    responses(
        (status = 200, description = "The closed session", body = ChatDto),
        (status = 404, description = "`chat_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn close_chat(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ChatDto>, ApiError> {
    Ok(Json(ChatDto::from(&state.chats.close(id).await?)))
}

/// `GET /api/v1/sessions/{id}/stream?after=`: the kept events after `after`,
/// then live ones (streamed text included), as Server-Sent Events. Each data
/// line is `{ "seq": n | null, "event": …, "at": "<RFC 3339>" }`: when it was
/// kept, or (live) when it reached the server.
async fn stream(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<AfterQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    state.chats.get(id).await?;
    // Listen first, so nothing falls between the stored and the live events.
    let mut live = state.chats.subscribe();
    let sessions = std::sync::Arc::clone(&state.chats);
    let (sender, receiver) = tokio::sync::mpsc::channel::<SseEvent>(1);
    tokio::spawn(async move {
        tokio::select! {
            biased;
            () = sender.closed() => {}
            () = async {
                let mut last = query.after;
                loop {
                    let Ok(page) = sessions.events(id, last, 32).await else {
                        return;
                    };
                    let full = page.len() == 32;
                    for event in page {
                        last = event.seq;
                        let data = json!({ "seq": event.seq, "event": event.event, "at": stamp(event.created_at) })
                            .to_string();
                        if sender.send(SseEvent::default().data(data)).await.is_err() {
                            return;
                        }
                    }
                    if !full {
                        break;
                    }
                }
                // Its subagents and background tasks, and the current durable queue.
                let Ok(queue) = sessions.queue_event(id).await else {
                    return;
                };
                for current in sessions.observation(id).into_iter().chain([queue]) {
                    let data =
                        json!({ "seq": null, "event": current, "at": stamp(crate::shared::time::now()) })
                            .to_string();
                    if sender.send(SseEvent::default().data(data)).await.is_err() {
                        return;
                    }
                }
                loop {
                    match live.recv().await {
                        Ok(event) if event.chat == id => {
                            if let Some(seq) = event.seq {
                                if seq <= last {
                                    continue;
                                }
                                last = seq;
                            }
                            let data = json!({
                                "seq": event.seq,
                                "event": event.event,
                                "at": stamp(crate::shared::time::now()),
                            })
                            .to_string();
                            if sender.send(SseEvent::default().data(data)).await.is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        // Too slow (the client reconnects with its last number), or done.
                        Err(
                            tokio::sync::broadcast::error::RecvError::Lagged(_)
                            | tokio::sync::broadcast::error::RecvError::Closed,
                        ) => return,
                    }
                }
            } => {}
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|event| (Ok(event), receiver))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// The words to find; each must appear (any case).
    pub q: String,
    /// At most this many titles and this many messages (1 to 200; 50 by default).
    pub limit: Option<u64>,
}

/// Searches a project's sessions: their titles, and the messages you and
/// the agents wrote in them. Titles first, then messages, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}/session-search",
    tag = "sessions.chat",
    params(("project" = String, Path, description = "The project's name or ID"), SearchQuery),
    responses(
        (status = 200, description = "What was found", body = SessionSearchDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn search_sessions(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
    QueryParams(query): QueryParams<SearchQuery>,
) -> Result<Json<SessionSearchDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(SessionSearchDto {
        items: state.chats.search(project.id, &query.q, limit).await?,
    }))
}

/// A time as the stream writes it (RFC 3339), for the client's turn timing.
fn stamp(time: time::OffsetDateTime) -> String {
    time.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(search_sessions))
        .routes(routes!(list_chats, start_chat))
        .routes(routes!(list_edited_files))
        .routes(routes!(list_models))
        .routes(routes!(get_chat, update_chat, remove_chat))
        .routes(routes!(fork_chat))
        .routes(routes!(list_chat_commands))
        .routes(routes!(get_chat_modes))
        .routes(routes!(get_chat_usage))
        .routes(routes!(get_chat_turns))
        .routes(routes!(list_project_commands))
        .routes(routes!(get_task_output))
        .routes(routes!(stop_chat_work))
        .routes(routes!(get_agent_history))
        .routes(routes!(list_events))
        .routes(routes!(send_message))
        .routes(routes!(unqueue_message, edit_queued_message))
        .routes(routes!(reorder_queue))
        .routes(routes!(send_queued_now))
        .routes(routes!(interrupt_chat))
        .routes(routes!(set_chat_mode))
        .routes(routes!(get_chat_model, set_chat_model))
        .routes(routes!(list_agent_defaults))
        .routes(routes!(save_agent_defaults))
        .routes(routes!(answer_request))
        .routes(routes!(close_chat))
}

pub fn stream_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/v1/chats/{id}/stream", get(stream))
}
