//! `/api/v1/plugin-states` and `/api/v1/plugins/{id}/logs`.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::plugins::host::api::{
    MachineQuery, PluginLogsDto, PluginMachineStateDto, PluginMachineStateListDto,
};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{PathParams, QueryParams};
use crate::server::state::AppState;

/// How every plugin's daemon entry is doing on the machines online now.
#[utoipa::path(
    get,
    path = "/api/v1/plugin-states",
    tag = "plugins.plugin",
    responses((status = 200, description = "The states", body = PluginMachineStateListDto))
)]
pub async fn plugin_states(State(state): State<AppState>) -> Json<PluginMachineStateListDto> {
    let items = state
        .plugin_host
        .states(None)
        .iter()
        .map(PluginMachineStateDto::from)
        .collect();
    Json(PluginMachineStateListDto { items })
}

/// A plugin's logs on a machine: its output and what it logged, the last
/// 1,000 lines, kept in the daemon's memory.
#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/logs",
    tag = "plugins.plugin",
    params(("id" = String, Path, description = "The plugin's ID"), MachineQuery),
    responses(
        (status = 200, description = "The lines", body = PluginLogsDto),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn plugin_logs(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
    QueryParams(query): QueryParams<MachineQuery>,
) -> Result<Json<PluginLogsDto>, ApiError> {
    let lines = state.plugin_host.logs(&id, query.machine).await?;
    Ok(Json(PluginLogsDto { lines }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(plugin_states))
        .routes(routes!(plugin_logs))
}
