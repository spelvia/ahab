//! `/api/v1/machines/{id}/metrics`: a machine's resources, live.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::metrics::api::MachineMetricsDto;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::PathParams;
use crate::server::state::AppState;

/// A machine's CPU, memory, disks and busiest processes now, with every
/// process Ahab started. Process arguments are never sent.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/metrics",
    tag = "machines.metrics",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "The machine's resources", body = MachineMetricsDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn machine_metrics(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<MachineMetricsDto>, ApiError> {
    Ok(Json(state.metrics.metrics(id).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(machine_metrics))
}
