//! Problems: a project's (`/api/v1/projects/{id}/problems`), running
//! its checks, a project's totals, and what an agent's turn changed.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::tools::problems::api::{
    CheckRootsDto, ChecksDto, ProblemListDto, ProblemSummaryDto, ProblemsQuery, RunChecksDto,
    TurnDiagnosticsDto,
};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

/// A project's problems, as last reported by its language servers,
/// linters and checks; grouped by file, filtered by severity, source,
/// path and text.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/problems",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID"), ProblemsQuery),
    responses(
        (status = 200, description = "Its problems", body = ProblemListDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_problems(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<ProblemsQuery>,
) -> Result<Json<ProblemListDto>, ApiError> {
    Ok(Json(state.problems.list(id, &query).await?))
}

/// Runs a project's check commands now (those that run on request, or
/// those named) and stores what they found.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/checks",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = RunChecksDto,
    responses(
        (status = 200, description = "What ran, and the problems after", body = ChecksDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_outdated`, `checks_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn run_checks(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<RunChecksDto>,
) -> Result<Json<ChecksDto>, ApiError> {
    Ok(Json(
        state
            .problems
            .run_checks(id, "request", body.tools, body.checkouts, None)
            .await?,
    ))
}

/// Where Check project would run: each check command's roots, with the
/// repository or worktree each is in. With more than `askAbove` roots, the
/// web app asks which to check.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/check-roots",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "The roots", body = CheckRootsDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_outdated`, `checks_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`, `machine_timeout`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn check_roots(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<CheckRootsDto>, ApiError> {
    Ok(Json(state.problems.check_roots(id).await?))
}

/// A project's problem totals.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/problems/summary",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "Its totals", body = ProblemSummaryDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn problem_summary(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<ProblemSummaryDto>, ApiError> {
    state.projects.get(id).await?;
    Ok(Json(state.problems.summary(id).await?))
}

/// What an agent's turn changed: its new problems and those it fixed, in
/// the files it changed and their dependents.
#[utoipa::path(
    get,
    path = "/api/v1/chats/{id}/turns/{turn}/diagnostics",
    tag = "tools.problems",
    params(("id" = Uuid, Path, description = "The chat's ID"), ("turn" = String, Path, description = "The turn's ID")),
    responses(
        (status = 200, description = "The difference", body = TurnDiagnosticsDto),
        (status = 404, description = "`chat_not_found`, `turn_diagnostics_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn turn_diagnostics(
    State(state): State<AppState>,
    PathParams((id, turn)): PathParams<(Uuid, String)>,
) -> Result<Json<TurnDiagnosticsDto>, ApiError> {
    Ok(Json(state.problems.turn(id, &turn).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_problems))
        .routes(routes!(run_checks))
        .routes(routes!(check_roots))
        .routes(routes!(problem_summary))
        .routes(routes!(turn_diagnostics))
}
