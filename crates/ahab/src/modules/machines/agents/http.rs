//! `/api/v1/machines/{id}/agents`: which coding agents a machine can run.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::agents::api::{
    AddAgentDto, AddedAgentDto, AgentCatalogDto, AgentStatusDto, AgentStatusListDto,
    AgentUpdateListDto, AgentUpdatedDto,
};
use crate::modules::machines::agents::model::AgentStatus;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{PathParams, QueryParams};
use crate::server::state::AppState;

fn list(statuses: &[AgentStatus]) -> AgentStatusListDto {
    AgentStatusListDto {
        items: statuses.iter().map(AgentStatusDto::from).collect(),
    }
}

/// The agents on a machine, as last detected; empty until its daemon has
/// answered once.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/agents",
    tag = "machines.agents",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "The agents", body = AgentStatusListDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_agents(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<AgentStatusListDto>, ApiError> {
    Ok(Json(list(&state.agents.list(id).await?)))
}

/// Detects the agents on a machine again, now.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/agents/refresh",
    tag = "machines.agents",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "The agents, just detected", body = AgentStatusListDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn refresh_agents(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<AgentStatusListDto>, ApiError> {
    Ok(Json(list(&state.agents.refresh(id).await?)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UpdatesQuery {
    /// Check now instead of answering from the last hour.
    #[serde(default)]
    refresh: bool,
}

/// Whether newer versions of a machine's agents are out (from npm), and
/// whether Ahab can install them there. Checked at most hourly.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/agents/updates",
    tag = "machines.agents",
    params(("id" = Uuid, Path, description = "The machine's ID"), UpdatesQuery),
    responses(
        (status = 200, description = "Each installed agent's update", body = AgentUpdateListDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn agent_updates(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<UpdatesQuery>,
) -> Result<Json<AgentUpdateListDto>, ApiError> {
    Ok(Json(state.agents.updates(id, query.refresh).await?))
}

/// Updates an agent on a machine with the agent's own updater (`claude
/// update`, `codex update`, `pi update --self`), then detects the agents
/// again. Takes up to a few minutes; agents installed by Nix are refused.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/agents/{agent}/update",
    tag = "machines.agents",
    params(
        ("id" = Uuid, Path, description = "The machine's ID"),
        ("agent" = String, Path, description = "`claude-code`, `codex` or `pi`"),
    ),
    responses(
        (status = 200, description = "What the update did", body = AgentUpdatedDto),
        (status = 404, description = "`machine_not_found`, `agent_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_agent(
    State(state): State<AppState>,
    PathParams((id, agent)): PathParams<(Uuid, String)>,
) -> Result<Json<AgentUpdatedDto>, ApiError> {
    Ok(Json(state.agents.update(id, &agent).await?))
}

/// Every agent Ahab can run: its own, and the ACP registry's (the copy
/// shipped with Ahab until a fresh one is fetched, daily).
#[utoipa::path(
    get,
    path = "/api/v1/agents/catalog",
    tag = "machines.agents",
    responses((status = 200, description = "The agents", body = AgentCatalogDto))
)]
pub async fn agent_catalog(State(state): State<AppState>) -> Json<AgentCatalogDto> {
    Json(state.agents.catalog())
}

/// `/api/v1/agents/{agent}/logo`: an agent's logo, an SVG: Stubb's for its
/// own agents, the registry's (fetched once, and served only when it is a
/// plain SVG) for others. An image for the browser, outside the API
/// document.
pub async fn agent_logo(
    State(state): State<AppState>,
    PathParams(agent): PathParams<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let svg = state.agents.logo(&agent).await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
            // Even a logo that slipped through runs nothing.
            (
                axum::http::header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; sandbox",
            ),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        svg,
    ))
}

/// Adds an agent to a machine: from the ACP registry (`agent`, such as
/// `acp:gemini`; npx and uvx agents) or custom (`name` and `program`).
/// Detection runs again in the background.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/agents",
    tag = "machines.agents",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    request_body = AddAgentDto,
    responses(
        (status = 201, description = "Added", body = AddedAgentDto),
        (status = 404, description = "`machine_not_found`, `agent_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`agent_not_addable`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`invalid`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn add_agent(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    Json(request): Json<AddAgentDto>,
) -> Result<(StatusCode, Json<AddedAgentDto>), ApiError> {
    let added = state.agents.add(id, request).await?;
    Ok((StatusCode::CREATED, Json(AddedAgentDto::from(&added))))
}

/// Removes an agent added to a machine; built-in agents cannot be removed.
#[utoipa::path(
    delete,
    path = "/api/v1/machines/{id}/agents/{agent}",
    tag = "machines.agents",
    params(
        ("id" = Uuid, Path, description = "The machine's ID"),
        ("agent" = String, Path, description = "Such as `acp:gemini` or `custom:my-agent`"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`machine_not_found`, `agent_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_agent(
    State(state): State<AppState>,
    PathParams((id, agent)): PathParams<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    state.agents.remove(id, &agent).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_agents, add_agent))
        .routes(routes!(refresh_agents))
        .routes(routes!(agent_updates))
        .routes(routes!(update_agent))
        .routes(routes!(agent_catalog))
        .route(
            "/api/v1/agents/{agent}/logo",
            axum::routing::get(agent_logo),
        )
        .routes(routes!(remove_agent))
}
