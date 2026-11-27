//! `/api/v1/plugins`: installing, enabling, placing and removing plugins,
//! and their files.

use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::plugins::plugin::api::{
    EnablePluginDto, InstallPluginDto, InstalledPluginDto, PluginChangeDto, PluginDto,
    PluginListDto, PluginSourceDto,
};
use crate::modules::plugins::plugin::model::Source;
use crate::modules::plugins::plugin::service::Installed;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

fn installed(installed: &Installed) -> InstalledPluginDto {
    InstalledPluginDto {
        plugin: PluginDto::from(&installed.plugin),
        warnings: installed.warnings.clone(),
    }
}

/// The installed plugins, in the order their interceptors run.
#[utoipa::path(
    get,
    path = "/api/v1/plugins",
    tag = "plugins.plugin",
    responses((status = 200, description = "The plugins", body = PluginListDto))
)]
pub async fn list_plugins(State(state): State<AppState>) -> Result<Json<PluginListDto>, ApiError> {
    let items = state
        .plugins
        .list()
        .await?
        .iter()
        .map(PluginDto::from)
        .collect();
    Ok(Json(PluginListDto { items }))
}

/// Installs a plugin from a folder on the server's host or a Git URL. It
/// starts disabled.
#[utoipa::path(
    post,
    path = "/api/v1/plugins",
    tag = "plugins.plugin",
    request_body = InstallPluginDto,
    responses(
        (status = 201, description = "Installed", body = InstalledPluginDto),
        (status = 409, description = "`plugin_exists`, `plugin_build_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: no manifest, a manifest that breaks a rule, entries not built", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn install_plugin(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<InstallPluginDto>,
) -> Result<(axum::http::StatusCode, Json<InstalledPluginDto>), ApiError> {
    let source = match body.source {
        PluginSourceDto::Folder => Source::Folder,
        PluginSourceDto::Git => Source::Git,
        PluginSourceDto::Dev => Source::Dev,
    };
    let done = state.plugins.install(source, &body.location).await?;
    Ok((axum::http::StatusCode::CREATED, Json(installed(&done))))
}

/// One plugin.
#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    responses(
        (status = 200, description = "The plugin", body = PluginDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
) -> Result<Json<PluginDto>, ApiError> {
    Ok(Json(PluginDto::from(&state.plugins.get(&id).await?)))
}

/// Changes where a plugin runs (when it asks to be placed) or its place
/// among interceptors.
#[utoipa::path(
    patch,
    path = "/api/v1/plugins/{id}",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    request_body = PluginChangeDto,
    responses(
        (status = 200, description = "Changed", body = PluginDto),
        (status = 404, description = "`plugin_not_found`, `machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn change_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    JsonBody(body): JsonBody<PluginChangeDto>,
) -> Result<Json<PluginDto>, ApiError> {
    let plugin = state
        .plugins
        .change(&id, body.machines, body.position)
        .await?;
    Ok(Json(PluginDto::from(&plugin)))
}

/// Removes a disabled plugin and its files.
#[utoipa::path(
    delete,
    path = "/api/v1/plugins/{id}",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_enabled`: disable it first", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.plugins.remove(&id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Turns a plugin on.
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{id}/enable",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    request_body = EnablePluginDto,
    responses(
        (status = 200, description = "Enabled", body = PluginDto),
        (status = 404, description = "`plugin_not_found`, `machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_dependency_missing`: a plugin its `dependsOn` names is not installed, enabled or in range", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn enable_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    JsonBody(body): JsonBody<EnablePluginDto>,
) -> Result<Json<PluginDto>, ApiError> {
    Ok(Json(PluginDto::from(
        &state.plugins.enable(&id, body.machines).await?,
    )))
}

/// Turns a plugin off everywhere.
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{id}/disable",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    responses(
        (status = 200, description = "Disabled", body = PluginDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_required`: enabled plugins depend on it", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn disable_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
) -> Result<Json<PluginDto>, ApiError> {
    Ok(Json(PluginDto::from(&state.plugins.disable(&id).await?)))
}

/// Installs a plugin again from its folder or Git repository, keeping
/// whether it is enabled.
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{id}/update",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID")),
    responses(
        (status = 200, description = "Updated", body = InstalledPluginDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_build_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_plugin(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
) -> Result<Json<InstalledPluginDto>, ApiError> {
    Ok(Json(installed(&state.plugins.update(&id).await?)))
}

/// One of a plugin's installed files, such as its web entry
/// (`/api/v1/plugins/{id}/files/dist/web.js`), outside the `OpenAPI`
/// document (the path has slashes). The web app asks with `?v=<digest>`.
async fn plugin_file(
    State(state): State<AppState>,
    PathParams((id, path)): PathParams<(String, String)>,
) -> Result<Response, ApiError> {
    let (bytes, kind) = state.plugins.file(&id, &path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, kind),
            // The web app asks with `?v=<digest>`; a new build is a new address.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        bytes,
    )
        .into_response())
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_plugins, install_plugin))
        .routes(routes!(get_plugin, change_plugin, remove_plugin))
        .routes(routes!(enable_plugin))
        .routes(routes!(disable_plugin))
        .routes(routes!(update_plugin))
}

/// The plugins' files, outside the `OpenAPI` document.
pub fn file_routes() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/api/v1/plugins/{id}/files/{*path}",
        axum::routing::get(plugin_file),
    )
}
