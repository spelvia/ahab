//! Pairing: links and codes a paired client offers (`/api/v1/pairings`),
//! pairing with them (`/api/v1/pair`), and requests a new client makes and
//! a paired one answers (`/api/v1/pairing-requests`).

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::clients::client::api::ClientDto;
use crate::modules::clients::client::model::ClientKind;
use crate::modules::clients::client::service::Paired;
use crate::modules::clients::pairing::api::{
    CreatePairingDto, CreatePairingRequestDto, CreatedPairingDto, CreatedPairingRequestDto,
    PairDto, PairedDto, PairingAnswerDto, PairingLinkDto, PairingRequestDto, PairingRequestListDto,
    PairingStateDto,
};
use crate::modules::clients::pairing::service::{Answer, Asker, Introduction};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams, QueryParams, Requester, set_cookie};
use crate::server::state::AppState;

/// How long a wait for an answer lasts at most.
const LONGEST_WAIT: u64 = 30;
/// How long it lasts when the requester does not say.
const DEFAULT_WAIT: u64 = 25;

fn asker(requester: &Requester) -> Asker {
    Asker {
        seen: requester.seen(),
        user_agent: requester.user_agent.clone(),
    }
}

fn me(caller: &Caller) -> Result<Uuid, ApiError> {
    caller
        .client_id()
        .ok_or_else(|| ApiError::unauthorized("not_paired", "Only a paired client can do this."))
}

/// A browser gets its credential as a cookie it cannot read; other clients
/// get it in the answer.
fn hand_over(paired: &Paired, requester: &Requester) -> (Option<String>, Option<HeaderValue>) {
    if paired.client.kind == ClientKind::Browser {
        (
            None,
            HeaderValue::from_str(&set_cookie(&paired.credential, requester)).ok(),
        )
    } else {
        (Some(paired.credential.clone()), None)
    }
}

fn with_cookie(mut response: Response, cookie: Option<HeaderValue>) -> Response {
    if let Some(cookie) = cookie {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}

/// Creates a pairing link and code for a new client (Add client, `ahab
/// clients add`, `ahab open`). They work once, for 10 minutes; the secret
/// and code are shown only in this answer.
#[utoipa::path(
    post,
    path = "/api/v1/pairings",
    tag = "clients.pairing",
    request_body = CreatePairingDto,
    responses(
        (status = 201, description = "The link and code", body = CreatedPairingDto),
        (status = 403, description = "`not_local`: only this computer may ask for the first-run link", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`already_paired`: a client is paired, so there is no first-run link", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_pairing(
    State(state): State<AppState>,
    caller: Caller,
    requester: Requester,
    JsonBody(body): JsonBody<CreatePairingDto>,
) -> Result<(StatusCode, Json<CreatedPairingDto>), ApiError> {
    let by = me(&caller)?;
    let offered = if body.first_run {
        if caller.client().map(|client| client.kind) != Some(ClientKind::Local) {
            return Err(ApiError::forbidden(
                "not_local",
                "Only the server's own computer may ask for the first-run link.",
            ));
        }
        state.pairings.first_run().await?.ok_or_else(|| {
            ApiError::conflict(
                "already_paired",
                "A client is paired already: pair more with Add client or `ahab clients add`.",
            )
        })?
    } else {
        state.pairings.offer(by).await?
    };
    let url = format!(
        "{}/pair#{}",
        state.access.link_base(&requester),
        offered.secret
    );
    Ok((
        StatusCode::CREATED,
        Json(CreatedPairingDto {
            id: offered.pairing.id,
            url,
            secret: offered.secret,
            code: offered.code,
            expires_at: offered.pairing.expires_at,
        }),
    ))
}

/// A pairing link this server offered, to see whether it was used.
#[utoipa::path(
    get,
    path = "/api/v1/pairings/{id}",
    tag = "clients.pairing",
    params(("id" = Uuid, Path, description = "The pairing link's ID")),
    responses(
        (status = 200, description = "The link", body = PairingLinkDto),
        (status = 404, description = "`pairing_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_pairing(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<PairingLinkDto>, ApiError> {
    let (pairing, now_state) = state.pairings.link(id).await?;
    Ok(Json(PairingLinkDto::new(&pairing, now_state)))
}

/// Withdraws a pairing link before it is used.
#[utoipa::path(
    delete,
    path = "/api/v1/pairings/{id}",
    tag = "clients.pairing",
    params(("id" = Uuid, Path, description = "The pairing link's ID")),
    responses(
        (status = 204, description = "Withdrawn"),
        (status = 404, description = "`pairing_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn withdraw_pairing(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.pairings.withdraw(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pairs this client with a pairing link's secret or a pairing code. A
/// browser gets its credential as a cookie; anything else gets it in the
/// answer. Ten tries a minute per address.
#[utoipa::path(
    post,
    path = "/api/v1/pair",
    tag = "clients.pairing",
    request_body = PairDto,
    responses(
        (status = 201, description = "Paired", body = PairedDto),
        (status = 403, description = "`pairing_invalid`, `pairing_expired`", body = Problem, content_type = "application/problem+json"),
        (status = 429, description = "`too_many_attempts`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn pair(
    State(state): State<AppState>,
    requester: Requester,
    JsonBody(body): JsonBody<PairDto>,
) -> Result<Response, ApiError> {
    let paired = state
        .pairings
        .pair(
            body.secret.as_deref(),
            body.code.as_deref(),
            &Introduction {
                name: body.name,
                kind: body.kind,
            },
            &asker(&requester),
        )
        .await?;
    let (credential, cookie) = hand_over(&paired, &requester);
    let answer = PairedDto {
        client: ClientDto::new(&paired.client, Some(paired.client.id), false),
        credential,
    };
    Ok(with_cookie(
        (StatusCode::CREATED, Json(answer)).into_response(),
        cookie,
    ))
}

/// A new client asks to be paired. Show the code; wait for the answer with
/// the secret. Every paired browser sees the request; it expires after 10
/// minutes.
#[utoipa::path(
    post,
    path = "/api/v1/pairing-requests",
    tag = "clients.pairing",
    request_body = CreatePairingRequestDto,
    responses(
        (status = 201, description = "Asked", body = CreatedPairingRequestDto),
        (status = 409, description = "`too_many_requests`", body = Problem, content_type = "application/problem+json"),
        (status = 429, description = "`too_many_attempts`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn request_pairing(
    State(state): State<AppState>,
    requester: Requester,
    JsonBody(body): JsonBody<CreatePairingRequestDto>,
) -> Result<(StatusCode, Json<CreatedPairingRequestDto>), ApiError> {
    let requested = state
        .pairings
        .request(
            &Introduction {
                name: body.name,
                kind: body.kind,
            },
            &asker(&requester),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedPairingRequestDto {
            id: requested.pairing.id,
            code: requested.code,
            secret: requested.secret,
            expires_at: requested.pairing.expires_at,
        }),
    ))
}

/// The waiting secret, asking for the answer.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WaitQuery {
    /// The secret the request was made with.
    pub secret: String,
    /// How long to wait for an answer, in seconds: at most 30, 25 by
    /// default, 0 to answer at once.
    #[serde(default)]
    pub wait: Option<u64>,
}

/// The new client waits for the answer (a long poll). Once approved, the
/// answer pairs it: browsers get their cookie, others the credential, once.
#[utoipa::path(
    get,
    path = "/api/v1/pairing-requests/{id}",
    tag = "clients.pairing",
    params(("id" = Uuid, Path, description = "The request's ID"), WaitQuery),
    responses(
        (status = 200, description = "`open`: ask again; `used`: paired; `denied`", body = PairingAnswerDto),
        (status = 403, description = "`pairing_invalid`, `pairing_expired`", body = Problem, content_type = "application/problem+json"),
        (status = 429, description = "`too_many_attempts`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn wait_for_answer(
    State(state): State<AppState>,
    requester: Requester,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<WaitQuery>,
) -> Result<Response, ApiError> {
    let wait = Duration::from_secs(query.wait.unwrap_or(DEFAULT_WAIT).min(LONGEST_WAIT));
    let answer = state
        .pairings
        .wait(id, &query.secret, &asker(&requester), wait)
        .await?;
    Ok(match answer {
        Answer::Waiting(_) => Json(PairingAnswerDto {
            state: PairingStateDto::Open,
            client: None,
            credential: None,
        })
        .into_response(),
        Answer::Denied => Json(PairingAnswerDto {
            state: PairingStateDto::Denied,
            client: None,
            credential: None,
        })
        .into_response(),
        Answer::Paired(paired) => {
            let (credential, cookie) = hand_over(&paired, &requester);
            with_cookie(
                Json(PairingAnswerDto {
                    state: PairingStateDto::Used,
                    client: Some(ClientDto::new(
                        &paired.client,
                        Some(paired.client.id),
                        false,
                    )),
                    credential,
                })
                .into_response(),
                cookie,
            )
        }
    })
}

/// The requests waiting for an answer, oldest first. The event stream also
/// announces them (`pairing.requested`, `pairing.answered`).
#[utoipa::path(
    get,
    path = "/api/v1/pairing-requests",
    tag = "clients.pairing",
    responses((status = 200, description = "The open requests", body = PairingRequestListDto))
)]
pub async fn list_pairing_requests(
    State(state): State<AppState>,
) -> Result<Json<PairingRequestListDto>, ApiError> {
    let requests = state.pairings.requests().await?;
    Ok(Json(PairingRequestListDto {
        items: requests.iter().map(PairingRequestDto::new).collect(),
    }))
}

/// Approves a request: the waiting client is paired. `id` is the request's
/// ID or the code it shows.
#[utoipa::path(
    post,
    path = "/api/v1/pairing-requests/{id}/approve",
    tag = "clients.pairing",
    params(("id" = String, Path, description = "The request's ID, or its code such as 7F3K")),
    responses(
        (status = 200, description = "Approved", body = PairingRequestDto),
        (status = 404, description = "`pairing_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn approve_pairing_request(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(id): PathParams<String>,
) -> Result<Json<PairingRequestDto>, ApiError> {
    let pairing = state.pairings.approve(&id, me(&caller)?).await?;
    Ok(Json(PairingRequestDto::new(&pairing)))
}

/// Denies a request: the waiting client is told so. `id` is the request's
/// ID or the code it shows.
#[utoipa::path(
    post,
    path = "/api/v1/pairing-requests/{id}/deny",
    tag = "clients.pairing",
    params(("id" = String, Path, description = "The request's ID, or its code such as 7F3K")),
    responses(
        (status = 200, description = "Denied", body = PairingRequestDto),
        (status = 404, description = "`pairing_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn deny_pairing_request(
    State(state): State<AppState>,
    PathParams(id): PathParams<String>,
) -> Result<Json<PairingRequestDto>, ApiError> {
    let pairing = state.pairings.deny(&id).await?;
    Ok(Json(PairingRequestDto::new(&pairing)))
}

/// This component's routes.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_pairing))
        .routes(routes!(get_pairing, withdraw_pairing))
        .routes(routes!(pair))
        .routes(routes!(request_pairing, list_pairing_requests))
        .routes(routes!(wait_for_answer))
        .routes(routes!(approve_pairing_request))
        .routes(routes!(deny_pairing_request))
}
