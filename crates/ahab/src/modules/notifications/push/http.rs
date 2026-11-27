//! `/api/v1/push/key` and `/api/v1/push-subscriptions`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::notifications::push::api::{
    CreateSubscriptionDto, PushKeyDto, SubscriptionDto, TestDto,
};
use crate::modules::notifications::push::model::NewSubscription;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams};
use crate::server::state::AppState;

/// The server's VAPID public key, which browsers subscribe with.
#[utoipa::path(
    get,
    path = "/api/v1/push/key",
    tag = "notifications.push",
    responses((status = 200, description = "The key", body = PushKeyDto))
)]
pub async fn push_key(State(state): State<AppState>) -> Result<Json<PushKeyDto>, ApiError> {
    Ok(Json(PushKeyDto {
        public_key: state.push.public_key()?,
    }))
}

/// The browsers that get pushes, oldest first.
#[utoipa::path(
    get,
    path = "/api/v1/push-subscriptions",
    tag = "notifications.push",
    responses((status = 200, description = "The subscriptions", body = Vec<SubscriptionDto>))
)]
pub async fn list_subscriptions(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<SubscriptionDto>>, ApiError> {
    let me = caller.client_id();
    Ok(Json(
        state
            .push
            .list()
            .await?
            .into_iter()
            .map(|subscription| SubscriptionDto::new(subscription, me))
            .collect(),
    ))
}

/// Subscribes this browser: it gets pushes while "outside the app" is on.
/// Subscribing again with the same endpoint replaces the old one.
#[utoipa::path(
    post,
    path = "/api/v1/push-subscriptions",
    tag = "notifications.push",
    request_body = CreateSubscriptionDto,
    responses(
        (status = 201, description = "Subscribed", body = SubscriptionDto),
        (status = 401, description = "`not_paired`: only a paired client subscribes", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: not an https endpoint or not real keys", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_subscription(
    State(state): State<AppState>,
    caller: Caller,
    JsonBody(body): JsonBody<CreateSubscriptionDto>,
) -> Result<(StatusCode, Json<SubscriptionDto>), ApiError> {
    let client = caller
        .client_id()
        .ok_or_else(|| ApiError::unauthorized("not_paired", "Only a paired client can do this."))?;
    let new = NewSubscription::parse(
        &body.endpoint,
        &body.keys.p256dh,
        &body.keys.auth,
        &body.label,
    )
    .map_err(|error| crate::shared::error::ServiceError::Invalid(vec![error]))?;
    let subscription = state.push.subscribe(client, new).await?;
    Ok((
        StatusCode::CREATED,
        Json(SubscriptionDto::new(subscription, Some(client))),
    ))
}

/// Stops pushes to a browser.
#[utoipa::path(
    delete,
    path = "/api/v1/push-subscriptions/{id}",
    tag = "notifications.push",
    params(("id" = Uuid, Path, description = "The subscription's ID")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "`push_subscription_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_subscription(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.push.unsubscribe(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pushes a test notification to a browser.
#[utoipa::path(
    post,
    path = "/api/v1/push-subscriptions/{id}/test",
    tag = "notifications.push",
    params(("id" = Uuid, Path, description = "The subscription's ID")),
    responses(
        (status = 200, description = "What came of it", body = TestDto),
        (status = 404, description = "`push_subscription_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn test_subscription(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<TestDto>, ApiError> {
    Ok(Json(TestDto {
        outcome: state.push.test(id).await?.into(),
    }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(push_key))
        .routes(routes!(list_subscriptions, create_subscription))
        .routes(routes!(delete_subscription))
        .routes(routes!(test_subscription))
}
