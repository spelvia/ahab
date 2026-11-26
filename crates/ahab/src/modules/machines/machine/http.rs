//! `/api/v1/machines`: list, read, rename and remove machines.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::machines::machine::api::{MachineDto, MachineListDto, RenameMachineDto};
use crate::modules::machines::machine::model::Machine;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

/// Every machine: the local one first, then by name.
#[utoipa::path(
    get,
    path = "/api/v1/machines",
    tag = "machines.machine",
    responses((status = 200, description = "The machines", body = MachineListDto))
)]
pub async fn list_machines(
    State(state): State<AppState>,
) -> Result<Json<MachineListDto>, ApiError> {
    let machines = state.machines.list().await?;
    Ok(Json(MachineListDto {
        items: machines
            .iter()
            .map(|machine| dto(&state, machine))
            .collect(),
    }))
}

/// One machine.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{id}",
    tag = "machines.machine",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 200, description = "The machine", body = MachineDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_machine(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<MachineDto>, ApiError> {
    let machine = state.machines.get(id).await?;
    Ok(Json(dto(&state, &machine)))
}

/// Renames a machine. Names are unique.
#[utoipa::path(
    patch,
    path = "/api/v1/machines/{id}",
    tag = "machines.machine",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    request_body = RenameMachineDto,
    responses(
        (status = 200, description = "The renamed machine", body = MachineDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_name_taken`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: the name is invalid", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn rename_machine(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<RenameMachineDto>,
) -> Result<Json<MachineDto>, ApiError> {
    let machine = state.machines.rename(id, &body.name).await?;
    Ok(Json(dto(&state, &machine)))
}

/// Removes a machine; its key stops working and it must join again. The
/// server's own machine cannot be removed.
#[utoipa::path(
    delete,
    path = "/api/v1/machines/{id}",
    tag = "machines.machine",
    params(("id" = Uuid, Path, description = "The machine's ID")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_is_local`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_machine(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.machines.remove(id).await?;
    state.presence.close_removed(id);
    Ok(StatusCode::NO_CONTENT)
}

/// A machine as the API shows it: whether it is connected, and what its
/// daemon lacks.
fn dto(state: &AppState, machine: &Machine) -> MachineDto {
    match state.presence.missing_methods(machine.id) {
        Some(missing) => MachineDto::new(machine, true).lacking(missing),
        None => MachineDto::new(machine, false),
    }
}

/// This component's routes.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_machines))
        .routes(routes!(get_machine, rename_machine, remove_machine))
}
