//! `/api/v1/machines/{id}/tool-settings` and
//! `/api/v1/projects/{id}/tool-settings`: which tools, where and how they
//! start, per machine and per project.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::tools::setting::model::ToolSettings;
use crate::modules::tools::setting::store::Owner;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

/// A machine's tool settings: paths, extra environment, tools turned off
/// or added there, how many servers run at once.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/tool-settings",
    tag = "tools.setting",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "Its settings", body = ToolSettings),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_machine_tool_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ToolSettings>, ApiError> {
    state.machines.get(id).await?;
    Ok(Json(state.tool_settings.get(Owner::Machine(id)).await?))
}

/// Replaces a machine's tool settings.
#[utoipa::path(
    put,
    path = "/api/v1/machines/{id}/tool-settings",
    tag = "tools.setting",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    request_body = ToolSettings,
    responses(
        (status = 200, description = "Its settings", body = ToolSettings),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn put_machine_tool_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<ToolSettings>,
) -> Result<Json<ToolSettings>, ApiError> {
    state.machines.get(id).await?;
    Ok(Json(
        state.tool_settings.put(Owner::Machine(id), body).await?,
    ))
}

/// A project's tool choices: tools per language and their order, format
/// on save, check commands, the environment, languages turned off. They
/// follow the project to every machine.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/tool-settings",
    tag = "tools.setting",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "Its settings", body = ToolSettings),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_project_tool_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ToolSettings>, ApiError> {
    state.projects.get(id).await?;
    Ok(Json(state.tool_settings.get(Owner::Project(id)).await?))
}

/// Replaces a project's tool choices.
#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/tool-settings",
    tag = "tools.setting",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = ToolSettings,
    responses(
        (status = 200, description = "Its settings", body = ToolSettings),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn put_project_tool_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<ToolSettings>,
) -> Result<Json<ToolSettings>, ApiError> {
    state.projects.get(id).await?;
    Ok(Json(
        state.tool_settings.put(Owner::Project(id), body).await?,
    ))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            get_machine_tool_settings,
            put_machine_tool_settings
        ))
        .routes(routes!(
            get_project_tool_settings,
            put_project_tool_settings
        ))
}
