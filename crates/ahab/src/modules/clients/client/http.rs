//! `/api/v1/clients`: the clients paired with this server, renaming and
//! revoking them, and tokens for scripts.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::clients::client::api::{
    ClientDto, ClientListDto, CreateTokenDto, CreatedTokenDto, RenameClientDto,
};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams, Requester, clear_cookie};
use crate::server::state::AppState;

/// The client making the request; routes here are never public.
fn me(caller: &Caller) -> Result<uuid::Uuid, ApiError> {
    caller
        .client_id()
        .ok_or_else(|| ApiError::unauthorized("not_paired", "Only a paired client can do this."))
}

/// The clients paired with this server, oldest first. Revoked clients are
/// not listed.
#[utoipa::path(
    get,
    path = "/api/v1/clients",
    tag = "clients.client",
    responses(
        (status = 200, description = "The clients", body = ClientListDto),
        (status = 401, description = "`not_paired`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_clients(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<ClientListDto>, ApiError> {
    let clients = state.clients.list().await?;
    let current = caller.client_id();
    Ok(Json(ClientListDto {
        items: clients
            .iter()
            .map(|client| ClientDto::new(client, current, state.clients.is_unused(client)))
            .collect(),
    }))
}

/// The client making this request.
#[utoipa::path(
    get,
    path = "/api/v1/clients/current",
    tag = "clients.client",
    responses(
        (status = 200, description = "This client", body = ClientDto),
        (status = 401, description = "`not_paired`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn current_client(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<ClientDto>, ApiError> {
    let client = caller
        .client()
        .ok_or_else(|| ApiError::unauthorized("not_paired", "Only a paired client can do this."))?;
    Ok(Json(ClientDto::new(
        client,
        Some(client.id),
        state.clients.is_unused(client),
    )))
}

/// Renames a client.
#[utoipa::path(
    patch,
    path = "/api/v1/clients/{id}",
    tag = "clients.client",
    params(("id" = Uuid, Path, description = "The client's ID")),
    request_body = RenameClientDto,
    responses(
        (status = 200, description = "The renamed client", body = ClientDto),
        (status = 404, description = "`client_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: the name is invalid", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn rename_client(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<RenameClientDto>,
) -> Result<Json<ClientDto>, ApiError> {
    let client = state.clients.rename(id, &body.name).await?;
    Ok(Json(ClientDto::new(
        &client,
        caller.client_id(),
        state.clients.is_unused(&client),
    )))
}

/// Revokes a client: its next request gets `401 not_paired`, and its open
/// streams and connections close now. Revoking "This computer" writes it a
/// new credential at once.
#[utoipa::path(
    delete,
    path = "/api/v1/clients/{id}",
    tag = "clients.client",
    params(("id" = Uuid, Path, description = "The client's ID")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "`client_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn revoke_client(
    State(state): State<AppState>,
    caller: Caller,
    requester: Requester,
    PathParams(id): PathParams<Uuid>,
) -> Result<Response, ApiError> {
    state.clients.revoke(id).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    // A browser that revoked itself forgets its cookie.
    if caller.client_id() == Some(id)
        && let Ok(cookie) = HeaderValue::from_str(&clear_cookie(&requester))
    {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    Ok(response)
}

/// Creates a token for a script: a client of kind `token`. The token is
/// shown only in this answer.
#[utoipa::path(
    post,
    path = "/api/v1/clients/tokens",
    tag = "clients.client",
    request_body = CreateTokenDto,
    responses(
        (status = 201, description = "The token", body = CreatedTokenDto),
        (status = 422, description = "`validation_failed`: the name is invalid", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_token(
    State(state): State<AppState>,
    caller: Caller,
    JsonBody(body): JsonBody<CreateTokenDto>,
) -> Result<(StatusCode, Json<CreatedTokenDto>), ApiError> {
    let by = me(&caller)?;
    let paired = state.clients.create_token(&body.name, Some(by)).await?;
    state.pairings.close_first_run().await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedTokenDto {
            token: paired.credential,
            client: ClientDto::new(&paired.client, Some(by), false),
        }),
    ))
}

/// This component's routes.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_clients))
        .routes(routes!(current_client))
        .routes(routes!(rename_client, revoke_client))
        .routes(routes!(create_token))
}
