//! `/api/v1/plugins/{id}/settings`.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::plugins::setting::api::{
    EffectiveQuery, EffectiveSettingsDto, PluginSettingsChangeDto, PluginSettingsDto, ScopeQuery,
    SettingScopeDto,
};
use crate::modules::plugins::setting::model::Scope;
use crate::modules::plugins::setting::service::At;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

fn at(query: &ScopeQuery) -> At {
    let scope = match query.scope {
        SettingScopeDto::Server => Scope::Server,
        SettingScopeDto::Machine => Scope::Machine,
        SettingScopeDto::Project => Scope::Project,
    };
    At {
        scope,
        id: query.id,
    }
}

/// A plugin's settings at a scope, and their schema.
#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/settings",
    tag = "plugins.setting",
    params(("id" = String, Path, description = "The plugin's ID"), ScopeQuery),
    responses(
        (status = 200, description = "The settings", body = PluginSettingsDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn plugin_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    QueryParams(query): QueryParams<ScopeQuery>,
) -> Result<Json<PluginSettingsDto>, ApiError> {
    let (schema, values) = state.settings.get(&id, at(&query)).await?;
    Ok(Json(PluginSettingsDto { schema, values }))
}

/// Sets a plugin's values at a scope; its entries hear of it.
#[utoipa::path(
    put,
    path = "/api/v1/plugins/{id}/settings",
    tag = "plugins.setting",
    params(("id" = String, Path, description = "The plugin's ID"), ScopeQuery),
    request_body = PluginSettingsChangeDto,
    responses(
        (status = 200, description = "Saved", body = PluginSettingsDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn save_plugin_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    QueryParams(query): QueryParams<ScopeQuery>,
    JsonBody(body): JsonBody<PluginSettingsChangeDto>,
) -> Result<Json<PluginSettingsDto>, ApiError> {
    state.settings.put(&id, at(&query), body.values).await?;
    let (schema, values) = state.settings.get(&id, at(&query)).await?;
    Ok(Json(PluginSettingsDto { schema, values }))
}

/// The values in effect on a machine, or for a project.
#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/settings/effective",
    tag = "plugins.setting",
    params(("id" = String, Path, description = "The plugin's ID"), EffectiveQuery),
    responses(
        (status = 200, description = "The values", body = EffectiveSettingsDto),
        (status = 404, description = "`plugin_not_found`, `project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn effective_plugin_settings(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    QueryParams(query): QueryParams<EffectiveQuery>,
) -> Result<Json<EffectiveSettingsDto>, ApiError> {
    let values = state
        .settings
        .effective(&id, query.machine, query.project)
        .await?;
    Ok(Json(EffectiveSettingsDto { values }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(plugin_settings, save_plugin_settings))
        .routes(routes!(effective_plugin_settings))
}
