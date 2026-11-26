//! `/api/v1/sessions`: a project's sessions and folders, starting a session
//! (with its first chat), renaming, pinning, filing, archiving and deleting
//! it, and its saved layout.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::sessions::session::api::{
    CreateSessionDto, FolderDto, FolderListDto, NameFolderDto, SessionDto, SessionListDto,
    UpdateFolderDto, UpdateSessionDto, WaitingDto,
};
use crate::modules::sessions::session::model::SessionKind;
use crate::modules::sessions::session::service::Change;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

async fn session_dto(state: &AppState, id: Uuid) -> Result<SessionDto, ApiError> {
    let (session, chats) = state.sessions.with_chats(id).await?;
    let terminals = state.terminals.of_session(id).await?;
    Ok(SessionDto::new(&session, &chats, &terminals))
}

/// A project's sessions with their chats, most recent activity first.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}/sessions",
    tag = "sessions.session",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 200, description = "The sessions", body = SessionListDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<Json<SessionListDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let sessions = state.sessions.list(project.id).await?;
    // Each session's terminals, oldest first.
    let mut terminals: std::collections::HashMap<Uuid, Vec<_>> = std::collections::HashMap::new();
    for terminal in state.terminals.of_project(project.id).await? {
        terminals
            .entry(terminal.session_id)
            .or_default()
            .push(terminal);
    }
    for list in terminals.values_mut() {
        list.sort_by_key(|terminal| (terminal.created_at, terminal.id));
    }
    Ok(Json(SessionListDto {
        items: sessions
            .iter()
            .map(|(session, chats)| {
                let theirs = terminals.get(&session.id).map_or(&[][..], Vec::as_slice);
                SessionDto::new(session, chats, theirs)
            })
            .collect(),
    }))
}

/// How many sessions need you, per project.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/waiting",
    tag = "sessions.session",
    responses((status = 200, description = "Counts per project", body = WaitingDto))
)]
pub async fn waiting_sessions(State(state): State<AppState>) -> Result<Json<WaitingDto>, ApiError> {
    Ok(Json(WaitingDto {
        projects: state.sessions.waiting().await?,
    }))
}

/// Starts a session in a project: its conversation with an agent. Its
/// title is the given one, else the first message's.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{project}/sessions",
    tag = "sessions.session",
    params(("project" = String, Path, description = "The project's name or ID")),
    request_body = CreateSessionDto,
    responses(
        (status = 201, description = "The session", body = SessionDto),
        (status = 404, description = "`project_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`agent_not_ready`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_session(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
    JsonBody(body): JsonBody<CreateSessionDto>,
) -> Result<(StatusCode, Json<SessionDto>), ApiError> {
    let id = state.projects.find(&project).await?.id;
    let title = body.title.clone().unwrap_or_else(|| {
        crate::modules::sessions::chat::model::title(
            &crate::modules::sessions::chat::service::title_text(
                &body.chat.message,
                &body.chat.references,
            ),
        )
    });
    let session = state
        .sessions
        .create(id, SessionKind::Chat, Some(&title), body.folder_id)
        .await?;
    let chat = body.chat;
    let started = state
        .chats
        .start(
            session.id,
            &chat.agent,
            &chat.message,
            chat.model.as_deref(),
            chat.effort.as_deref(),
            chat.mode.as_deref(),
            chat.fast,
            &chat.attachments,
            &chat.references,
            chat.command.as_ref(),
        )
        .await;
    if let Err(error) = started {
        // Refused before the chat existed (such as an agent that is not
        // ready): no empty session is left behind.
        if state.chats.list(session.id).await?.is_empty() {
            state.sessions.discard(session.id).await?;
        }
        return Err(error.into());
    }
    Ok((
        StatusCode::CREATED,
        Json(session_dto(&state, session.id).await?),
    ))
}

/// One session with its chats.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 200, description = "The session", body = SessionDto),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_session(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<SessionDto>, ApiError> {
    Ok(Json(session_dto(&state, id).await?))
}

/// Renames, pins or files a session; absent fields stay as they are.
#[utoipa::path(
    patch,
    path = "/api/v1/sessions/{id}",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The session's ID")),
    request_body = UpdateSessionDto,
    responses(
        (status = 200, description = "The session", body = SessionDto),
        (status = 404, description = "`session_not_found`, `folder_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_session(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<UpdateSessionDto>,
) -> Result<Json<SessionDto>, ApiError> {
    state
        .sessions
        .change(
            id,
            Change {
                title: body.title.as_deref(),
                folder: body.folder_id,
                pinned: body.pinned,
            },
        )
        .await?;
    Ok(Json(session_dto(&state, id).await?))
}

/// Archives a session: its live chats end, and it is listed apart. Its
/// history is kept.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/archive",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 200, description = "The session", body = SessionDto),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn archive_session(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<SessionDto>, ApiError> {
    state.sessions.get(id).await?;
    state.chats.end_session(id).await?;
    state.terminals.end_session(id).await?;
    state.sessions.archive(id).await?;
    Ok(Json(session_dto(&state, id).await?))
}

/// Brings an archived session back to the list.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/unarchive",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 200, description = "The session", body = SessionDto),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn unarchive_session(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<SessionDto>, ApiError> {
    state.sessions.unarchive(id).await?;
    Ok(Json(session_dto(&state, id).await?))
}

/// Deletes an archived session with its chats and their history.
#[utoipa::path(
    delete,
    path = "/api/v1/sessions/{id}",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The session's ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "`session_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`session_not_archived`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_session(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Only archived sessions go (`delete` says so); a terminal goes from
    // its machine too.
    if state.sessions.get(id).await?.archived_at.is_some() {
        state.terminals.delete_session(id).await?;
    }
    state.sessions.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A project's folders of sessions, by name.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}/session-folders",
    tag = "sessions.session",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 200, description = "The folders", body = FolderListDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_session_folders(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<Json<FolderListDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let folders = state.sessions.folders(project.id).await?;
    Ok(Json(FolderListDto {
        items: folders.iter().map(FolderDto::from).collect(),
    }))
}

/// Creates a folder of sessions in a project.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{project}/session-folders",
    tag = "sessions.session",
    params(("project" = String, Path, description = "The project's name or ID")),
    request_body = NameFolderDto,
    responses(
        (status = 201, description = "The folder", body = FolderDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_session_folder(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
    JsonBody(body): JsonBody<NameFolderDto>,
) -> Result<(StatusCode, Json<FolderDto>), ApiError> {
    let project = state.projects.find(&project).await?;
    let folder = state
        .sessions
        .create_folder(project.id, &body.name, body.parent_id)
        .await?;
    Ok((StatusCode::CREATED, Json(FolderDto::from(&folder))))
}

/// Renames a folder, moves it into another folder (or to the top level),
/// or pins it.
#[utoipa::path(
    patch,
    path = "/api/v1/session-folders/{id}",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The folder's ID")),
    request_body = UpdateFolderDto,
    responses(
        (status = 200, description = "The folder", body = FolderDto),
        (status = 404, description = "`folder_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`folder_in_itself`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn rename_session_folder(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<UpdateFolderDto>,
) -> Result<Json<FolderDto>, ApiError> {
    let folder = state
        .sessions
        .change_folder(id, body.name.as_deref(), body.parent_id, body.pinned)
        .await?;
    Ok(Json(FolderDto::from(&folder)))
}

/// Deletes a folder; what it held (sessions and folders) moves up to where it was.
#[utoipa::path(
    delete,
    path = "/api/v1/session-folders/{id}",
    tag = "sessions.session",
    params(("id" = Uuid, Path, description = "The folder's ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "`folder_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_session_folder(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.sessions.delete_folder(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_sessions))
        .routes(routes!(waiting_sessions))
        .routes(routes!(create_session))
        .routes(routes!(get_session, update_session, delete_session))
        .routes(routes!(archive_session))
        .routes(routes!(unarchive_session))
        .routes(routes!(list_session_folders, create_session_folder))
        .routes(routes!(rename_session_folder, delete_session_folder))
}
