//! Active and pinned repositories:
//! `/api/v1/projects/{id}/repository-activity` and `…/repository-pins`.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::tools::activity::api::{PinRepositoryDto, RepositoryActivityListDto};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

/// A project's repositories and worktrees that were used (a file opened
/// in them, or changed by an agent) or pinned, and which are active.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/repository-activity",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "Their use and pins", body = RepositoryActivityListDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_repository_activity(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<RepositoryActivityListDto>, ApiError> {
    Ok(Json(state.repository_activity.list(id).await?))
}

/// Pins a repository (or one of its worktrees), which keeps it active, or
/// unpins it.
#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/repository-pins",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = PinRepositoryDto,
    responses(
        (status = 200, description = "Their use and pins, after", body = RepositoryActivityListDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn pin_repository(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<PinRepositoryDto>,
) -> Result<Json<RepositoryActivityListDto>, ApiError> {
    Ok(Json(state.repository_activity.pin(id, body).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_repository_activity))
        .routes(routes!(pin_repository))
}
