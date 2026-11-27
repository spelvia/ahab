//! `/api/v1/settings/{section}`: Ahab's own settings.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Map, Value};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::platform::setting::api::SettingsDto;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectQuery {
    /// A project's ID: its values, over the server's.
    #[serde(default)]
    pub project: Option<Uuid>,
}

/// A section of settings: its fields, the values in effect (for the
/// project, when given) and the values set for the server and the project.
#[utoipa::path(
    get,
    path = "/api/v1/settings/{section}",
    tag = "platform.setting",
    params(("section" = String, Path, description = "The section, such as `editor`"), ProjectQuery),
    responses(
        (status = 200, description = "The section", body = SettingsDto),
        (status = 404, description = "`settings_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_settings(
    State(state): State<AppState>,
    PathParams(section): PathParams<String>,
    QueryParams(query): QueryParams<ProjectQuery>,
) -> Result<Json<SettingsDto>, ApiError> {
    Ok(Json(
        state
            .core_settings
            .get(&section, query.project)
            .await?
            .into(),
    ))
}

/// Changes settings for the server, or for a project (`project`): each
/// named field takes its value; `null` unsets it there.
#[utoipa::path(
    patch,
    path = "/api/v1/settings/{section}",
    tag = "platform.setting",
    params(("section" = String, Path, description = "The section, such as `editor`"), ProjectQuery),
    request_body(content = Object, description = "Field names and their new values, or `null`"),
    responses(
        (status = 200, description = "The section, after the change", body = SettingsDto),
        (status = 404, description = "`settings_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: an unknown field or a value that does not fit", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn change_settings(
    State(state): State<AppState>,
    PathParams(section): PathParams<String>,
    QueryParams(query): QueryParams<ProjectQuery>,
    JsonBody(change): JsonBody<Map<String, Value>>,
) -> Result<Json<SettingsDto>, ApiError> {
    Ok(Json(
        state
            .core_settings
            .change(&section, query.project, &change)
            .await?
            .into(),
    ))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_settings, change_settings))
}
