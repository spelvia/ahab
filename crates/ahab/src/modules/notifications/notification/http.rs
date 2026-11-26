//! `/api/v1/notifications`, `/api/v1/notification-settings` and the live
//! `/api/v1/notification-events`.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::notifications::notification::api::{
    ActivityDto, CountDto, NotificationDto, NotificationListDto, SettingsChangeDto, SettingsDto,
};
use crate::modules::notifications::notification::service::NotificationEvent;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only unread ones.
    #[serde(default)]
    pub unread: bool,
    /// Those older than this notification (the next page).
    pub before: Option<Uuid>,
    /// At most this many (1 to 200; 50 by default).
    pub limit: Option<u32>,
}

/// The inbox: newest first, and how many are unread.
#[utoipa::path(
    get,
    path = "/api/v1/notifications",
    tag = "notifications.notification",
    params(ListQuery),
    responses(
        (status = 200, description = "The inbox", body = NotificationListDto),
        (status = 404, description = "`notification_not_found`: `before` names none", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    QueryParams(query): QueryParams<ListQuery>,
) -> Result<Json<NotificationListDto>, ApiError> {
    let (items, unread) = state
        .notifications
        .list(query.unread, query.before, query.limit.unwrap_or(50))
        .await?;
    Ok(Json(NotificationListDto {
        items: items.into_iter().map(NotificationDto::from).collect(),
        unread,
    }))
}

/// Marks a notification read.
#[utoipa::path(
    post,
    path = "/api/v1/notifications/{id}/read",
    tag = "notifications.notification",
    params(("id" = Uuid, Path, description = "The notification's ID")),
    responses(
        (status = 200, description = "Read", body = NotificationDto),
        (status = 404, description = "`notification_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn read_notification(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<NotificationDto>, ApiError> {
    Ok(Json(state.notifications.read(id).await?.into()))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReadAllQuery {
    /// Only this project's.
    pub project: Option<Uuid>,
}

/// Marks every unread notification read, of a project when given.
#[utoipa::path(
    post,
    path = "/api/v1/notifications/read-all",
    tag = "notifications.notification",
    params(ReadAllQuery),
    responses((status = 200, description = "How many were marked", body = CountDto))
)]
pub async fn read_all_notifications(
    State(state): State<AppState>,
    QueryParams(query): QueryParams<ReadAllQuery>,
) -> Result<Json<CountDto>, ApiError> {
    let count = state.notifications.read_all(query.project).await?;
    Ok(Json(CountDto { count }))
}

/// Clears the inbox of read notifications.
#[utoipa::path(
    delete,
    path = "/api/v1/notifications",
    tag = "notifications.notification",
    responses((status = 200, description = "How many were deleted", body = CountDto))
)]
pub async fn clear_notifications(
    State(state): State<AppState>,
) -> Result<Json<CountDto>, ApiError> {
    let count = state.notifications.clear_read().await?;
    Ok(Json(CountDto { count }))
}

/// The server's notification settings, with every kind.
#[utoipa::path(
    get,
    path = "/api/v1/notification-settings",
    tag = "notifications.notification",
    responses((status = 200, description = "The settings", body = SettingsDto))
)]
pub async fn get_notification_settings(
    State(state): State<AppState>,
) -> Result<Json<SettingsDto>, ApiError> {
    Ok(Json(SettingsDto::with(
        state.notifications.settings().await?,
        state.plugin_notifications.kinds().await?,
    )))
}

/// Changes the notification settings, all at once.
#[utoipa::path(
    put,
    path = "/api/v1/notification-settings",
    tag = "notifications.notification",
    request_body = SettingsChangeDto,
    responses(
        (status = 200, description = "The settings now", body = SettingsDto),
        (status = 422, description = "`validation_failed`: `keepDays` out of range", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_notification_settings(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<SettingsChangeDto>,
) -> Result<Json<SettingsDto>, ApiError> {
    let settings = state.notifications.set_settings(body.into()).await?;
    Ok(Json(SettingsDto::with(
        settings,
        state.plugin_notifications.kinds().await?,
    )))
}

/// How often the stream says how many agents work, for the phone's
/// background connection: it stays only while they do.
const ACTIVITY_EVERY: Duration = Duration::from_secs(20);

/// How many agents are working now.
#[utoipa::path(
    get,
    path = "/api/v1/notification-activity",
    tag = "notifications.notification",
    responses((status = 200, description = "The agents working", body = ActivityDto))
)]
pub async fn notification_activity(
    State(state): State<AppState>,
) -> Result<Json<ActivityDto>, ApiError> {
    Ok(Json(ActivityDto {
        working: state.chats.working().await?,
    }))
}

/// Every notification from now on, each `{"type": "created", "notification"}`
/// or `{"type": "changed", "unread"}`; and `{"type": "activity", "working"}`
/// at once and every 20 seconds.
async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let heard = state.notifications.subscribe();
    let mut every = tokio::time::interval(ACTIVITY_EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let chats = std::sync::Arc::clone(&state.chats);
    let stream = futures_util::stream::unfold(
        (heard, every, chats),
        |(mut heard, mut every, chats)| async move {
            let data = tokio::select! {
                event = heard.recv() => match event {
                    Ok(event) => match event.as_ref() {
                        NotificationEvent::Created(notification) => json!({
                            "type": "created",
                            "notification": NotificationDto::from(notification.as_ref().clone()),
                        }),
                        NotificationEvent::Changed { unread } => {
                            json!({ "type": "changed", "unread": unread })
                        }
                    },
                    // Missed some: the app fetches the inbox again.
                    Err(broadcast::error::RecvError::Lagged(_)) => json!({ "type": "changed" }),
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                _ = every.tick() => {
                    let working = chats.working().await.unwrap_or_default();
                    json!({ "type": "activity", "working": working })
                }
            };
            Some((
                Ok(SseEvent::default().data(data.to_string())),
                (heard, every, chats),
            ))
        },
    );
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_notifications, clear_notifications))
        .routes(routes!(read_notification))
        .routes(routes!(read_all_notifications))
        .routes(routes!(
            get_notification_settings,
            set_notification_settings
        ))
        .routes(routes!(notification_activity))
}

pub fn stream_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/v1/notification-events", axum::routing::get(events))
}
