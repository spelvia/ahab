//! `/api/v1/projects/{id}/layouts/{clientClass}`: a project's windows.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::projects::layout::api::{LayoutDto, SaveLayoutDto};
use crate::modules::projects::layout::model::ClientClass;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

fn class(value: &str) -> Result<ClientClass, ApiError> {
    ClientClass::parse(value)
        .ok_or_else(|| ApiError::not_found("The client class is `desktop` or `mobile`."))
}

/// A project's saved windows; 404 `layout_not_found` before the first save.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/layouts/{clientClass}",
    tag = "projects.layout",
    params(
        ("id" = Uuid, Path, description = "The project's ID"),
        ("clientClass" = String, Path, description = "`desktop` or `mobile`"),
    ),
    responses(
        (status = 200, description = "The layout", body = LayoutDto),
        (status = 404, description = "`project_not_found`, `layout_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_layout(
    State(state): State<AppState>,
    PathParams((id, client_class)): PathParams<(Uuid, String)>,
) -> Result<Json<LayoutDto>, ApiError> {
    let layout = state
        .layouts
        .get(id, class(&client_class)?)
        .await?
        .ok_or_else(|| ApiError::not_found_as("layout_not_found", "No layout is saved yet."))?;
    Ok(Json(LayoutDto::from(&layout)))
}

/// Saves a project's windows over the revision the client last read.
#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/layouts/{clientClass}",
    tag = "projects.layout",
    params(
        ("id" = Uuid, Path, description = "The project's ID"),
        ("clientClass" = String, Path, description = "`desktop` or `mobile`"),
    ),
    request_body = SaveLayoutDto,
    responses(
        (status = 200, description = "Saved", body = LayoutDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`layout_changed`: another client saved first", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: too large", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn save_layout(
    State(state): State<AppState>,
    PathParams((id, client_class)): PathParams<(Uuid, String)>,
    JsonBody(body): JsonBody<SaveLayoutDto>,
) -> Result<Json<LayoutDto>, ApiError> {
    let layout = state
        .layouts
        .save(id, class(&client_class)?, body.layout, body.revision)
        .await?;
    Ok(Json(LayoutDto::from(&layout)))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_layout, save_layout))
}
