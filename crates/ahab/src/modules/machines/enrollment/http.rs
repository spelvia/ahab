//! Joining: `/api/v1/enrollments`, approving machines, and one-time codes.

use axum::Json;
use axum::extract::State;
use axum::http::header::HOST;
use axum::http::{HeaderMap, StatusCode};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::enrollment::api::{
    CreateJoinCodeDto, CreatedJoinCodeDto, EnrollRequestDto, EnrollmentDto, JoinCodeDto,
    JoinCodeListDto,
};
use crate::modules::machines::enrollment::service::EnrollRequest;
use crate::modules::machines::machine::api::MachineDto;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, Requester};
use crate::server::state::AppState;

/// A machine asks to join. It proves it holds its key; with a one-time code
/// it is approved at once, otherwise it waits (15 minutes at most) for
/// approval. The same key asking again gets its request back.
#[utoipa::path(
    post,
    path = "/api/v1/enrollments",
    tag = "machines.enrollment",
    request_body = EnrollRequestDto,
    responses(
        (status = 201, description = "Pending, or active with a code", body = EnrollmentDto),
        (status = 401, description = "`signature_invalid`", body = Problem, content_type = "application/problem+json"),
        (status = 403, description = "`join_code_invalid`: unknown, used or expired", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`too_many_pending`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
    JsonBody(body): JsonBody<EnrollRequestDto>,
) -> Result<(StatusCode, Json<EnrollmentDto>), ApiError> {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let request = EnrollRequest {
        public_key: body.public_key,
        signature: body.signature,
        hostname: body.hostname,
        code: body.code,
    };
    let machine = state.enrollment.enroll(&request, host).await?;
    let fingerprint = state.presence.server_fingerprint();
    Ok((
        StatusCode::CREATED,
        Json(EnrollmentDto::new(&machine, fingerprint)),
    ))
}

/// Where a request to join stands; the joining machine asks until it is
/// approved.
#[utoipa::path(
    get,
    path = "/api/v1/enrollments/{id}",
    tag = "machines.enrollment",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "Pending or active", body = EnrollmentDto),
        (status = 404, description = "`enrollment_not_found`: rejected or expired", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn enrollment_status(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<EnrollmentDto>, ApiError> {
    let machine = state.enrollment.status(id).await?;
    let fingerprint = state.presence.server_fingerprint();
    Ok(Json(EnrollmentDto::new(&machine, fingerprint)))
}

/// Approves a pending machine, so it may connect. Compare its check code and
/// fingerprint with the joining machine's screen first.
#[utoipa::path(
    post,
    path = "/api/v1/machines/{id}/approve",
    tag = "machines.enrollment",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "The approved machine", body = MachineDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_not_pending`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn approve_machine(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<MachineDto>, ApiError> {
    let machine = state.machines.approve(id).await?;
    Ok(Json(MachineDto::new(
        &machine,
        state.presence.is_online(machine.id),
    )))
}

/// A new one-time code: a machine joining with it is approved at once. The
/// code is shown only in this answer; it works once, for 15 minutes.
#[utoipa::path(
    post,
    path = "/api/v1/join-codes",
    tag = "machines.enrollment",
    request_body = CreateJoinCodeDto,
    responses(
        (status = 201, description = "The code", body = CreatedJoinCodeDto),
        (status = 422, description = "`validation_failed`: the name is invalid", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_join_code(
    State(state): State<AppState>,
    requester: Requester,
    JsonBody(body): JsonBody<CreateJoinCodeDto>,
) -> Result<(StatusCode, Json<CreatedJoinCodeDto>), ApiError> {
    let (join_code, code) = state.enrollment.create_code(body.name.as_deref()).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedJoinCodeDto {
            code,
            join_code: JoinCodeDto::from(&join_code),
            server_url: Some(state.access.link_base(&requester)),
            server_fingerprint: Some(state.presence.server_fingerprint()),
        }),
    ))
}

/// The codes that still work, newest first, without the codes themselves.
#[utoipa::path(
    get,
    path = "/api/v1/join-codes",
    tag = "machines.enrollment",
    responses((status = 200, description = "The codes", body = JoinCodeListDto))
)]
pub async fn list_join_codes(
    State(state): State<AppState>,
) -> Result<Json<JoinCodeListDto>, ApiError> {
    let codes = state.enrollment.list_codes().await?;
    Ok(Json(JoinCodeListDto {
        items: codes.iter().map(JoinCodeDto::from).collect(),
    }))
}

/// Withdraws a code.
#[utoipa::path(
    delete,
    path = "/api/v1/join-codes/{id}",
    tag = "machines.enrollment",
    params(("id" = Uuid, Path, description = "The code's ID")),
    responses(
        (status = 204, description = "Withdrawn"),
        (status = 404, description = "`join_code_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_join_code(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.enrollment.delete_code(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// This component's routes.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(enroll))
        .routes(routes!(enrollment_status))
        .routes(routes!(approve_machine))
        .routes(routes!(create_join_code, list_join_codes))
        .routes(routes!(delete_join_code))
}
