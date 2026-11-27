//! `/api/v1/machines/{id}/tools`: the registry with a machine's states and
//! runtimes, and installing, updating and removing tools there.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::tools::registry::api::{MachineToolsDto, ToolJobDto};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{PathParams, QueryParams};
use crate::server::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ToolsQuery {
    /// Look for runtimes and tools again ("Scan again").
    #[serde(default)]
    rescan: bool,
}

/// Every tool record with its state on a machine (installed by Ahab,
/// found, available, cannot run), and the machine's runtimes.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/tools",
    tag = "tools.registry",
    params(("id" = Uuid, Path, description = "The machine's ID"), ToolsQuery),
    responses(
        (status = 200, description = "The tools", body = MachineToolsDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_tools(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<ToolsQuery>,
) -> Result<Json<MachineToolsDto>, ApiError> {
    Ok(Json(state.tools.machine_tools(id, query.rescan).await?))
}

async fn change(
    state: &AppState,
    id: Uuid,
    tool: &str,
    action: &str,
) -> Result<(StatusCode, Json<ToolJobDto>), ApiError> {
    Ok((
        StatusCode::ACCEPTED,
        Json(state.tools.change_tool(id, tool, action).await?),
    ))
}

/// Installs a tool on a machine, into Ahab's data folder there, at the
/// version the registry pins. Its progress follows as `tools.job` events.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/tools/{tool}/install",
    tag = "tools.registry",
    params(("id" = Uuid, Path, description = "The machine's ID"), ("tool" = String, Path, description = "The tool's ID")),
    responses(
        (status = 202, description = "The job, started", body = ToolJobDto),
        (status = 404, description = "`machine_not_found`, `tool_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`tool_busy`, `tool_not_installable`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn install_tool(
    State(state): State<AppState>,
    PathParams((id, tool)): PathParams<(Uuid, String)>,
) -> Result<(StatusCode, Json<ToolJobDto>), ApiError> {
    change(&state, id, &tool, "install").await
}

/// Installs the registry's newer version beside the old one, then restarts
/// the tool's servers and removes the old version.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/tools/{tool}/update",
    tag = "tools.registry",
    params(("id" = Uuid, Path, description = "The machine's ID"), ("tool" = String, Path, description = "The tool's ID")),
    responses(
        (status = 202, description = "The job, started", body = ToolJobDto),
        (status = 404, description = "`machine_not_found`, `tool_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`tool_busy`, `tool_not_installable`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_tool(
    State(state): State<AppState>,
    PathParams((id, tool)): PathParams<(Uuid, String)>,
) -> Result<(StatusCode, Json<ToolJobDto>), ApiError> {
    change(&state, id, &tool, "update").await
}

/// Stops a tool's servers and deletes what Ahab installed of it.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/tools/{tool}/remove",
    tag = "tools.registry",
    params(("id" = Uuid, Path, description = "The machine's ID"), ("tool" = String, Path, description = "The tool's ID")),
    responses(
        (status = 202, description = "The job, started", body = ToolJobDto),
        (status = 404, description = "`machine_not_found`, `tool_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`tool_busy`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_tool(
    State(state): State<AppState>,
    PathParams((id, tool)): PathParams<(Uuid, String)>,
) -> Result<(StatusCode, Json<ToolJobDto>), ApiError> {
    change(&state, id, &tool, "remove").await
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tools))
        .routes(routes!(install_tool))
        .routes(routes!(update_tool))
        .routes(routes!(remove_tool))
}
