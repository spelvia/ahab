//! `/api/v1/machines/{id}/agents/usage`: the plan limits of the agents
//! signed in on a machine.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::usage::api::AgentUsageListDto;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{PathParams, QueryParams};
use crate::server::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UsageQuery {
    /// Ask the machine now instead of answering from the last 5 minutes.
    #[serde(default)]
    refresh: bool,
}

/// The plan limits of the agents signed in on a machine (Claude Code's and
/// Codex's): how much of each limit is used, and when it starts over. Asked
/// of the machine at most every 5 minutes; an offline machine answers with
/// what it last said.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}/agents/usage",
    tag = "machines.usage",
    params(("id" = Uuid, Path, description = "The machine's ID"), UsageQuery),
    responses(
        (status = 200, description = "The limits, per agent", body = AgentUsageListDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn agent_usage(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<UsageQuery>,
) -> Result<Json<AgentUsageListDto>, ApiError> {
    Ok(Json(state.usage.usage(id, query.refresh).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(agent_usage))
}
