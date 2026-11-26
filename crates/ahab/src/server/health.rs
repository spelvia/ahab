//! `GET /api/v1/health`: whether the server answers. Liveness only, with no
//! database check, so it stays cheap for `ahab status`, containers and load
//! balancers. Clients also read the server's version from it.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::server::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: HealthStatus,
    /// The server's version.
    #[schema(example = "0.1.0")]
    pub version: &'static str,
}

/// Always `ok`: an answer means the server is up.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Ok,
}

/// Whether the server answers, and its version.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "platform.about",
    responses((status = 200, description = "The server answers", body = Health))
)]
pub async fn health() -> Json<Health> {
    Json(Health {
        status: HealthStatus::Ok,
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// The health route.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}
