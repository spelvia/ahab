//! `/api/v1/features`: Ahab's core features, on or off.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::platform::feature::api::{FeatureChangeDto, FeatureDto, FeatureListDto};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

async fn listed(state: &AppState) -> Result<FeatureListDto, ApiError> {
    let items = state
        .features
        .list()
        .await?
        .into_iter()
        .map(|(feature, enabled)| FeatureDto {
            id: feature.id.to_owned(),
            name: feature.name.to_owned(),
            description: feature.description.to_owned(),
            enabled,
            required: feature.required,
        })
        .collect();
    Ok(FeatureListDto { items })
}

/// Ahab's core features, and which are on.
#[utoipa::path(
    get,
    path = "/api/v1/features",
    tag = "platform.feature",
    responses((status = 200, description = "The features", body = FeatureListDto))
)]
pub async fn list_features(
    State(state): State<AppState>,
) -> Result<Json<FeatureListDto>, ApiError> {
    Ok(Json(listed(&state).await?))
}

/// Switches a core feature on or off, for everyone.
#[utoipa::path(
    put,
    path = "/api/v1/features/{id}",
    tag = "platform.feature",
    params(("id" = String, Path, description = "The feature's ID, such as `git`")),
    request_body = FeatureChangeDto,
    responses(
        (status = 200, description = "Every feature, after the change", body = FeatureListDto),
        (status = 404, description = "`feature_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: Sessions is always on", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_feature(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    JsonBody(body): JsonBody<FeatureChangeDto>,
) -> Result<Json<FeatureListDto>, ApiError> {
    state.features.set(&id, body.enabled).await?;
    Ok(Json(listed(&state).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_features))
        .routes(routes!(set_feature))
}
