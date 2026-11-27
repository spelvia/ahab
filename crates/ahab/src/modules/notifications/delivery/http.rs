//! `/api/v1/plugins/{id}/notifications`: a plugin sends a notification
//! (from its daemon, with the machine's token).

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::modules::notifications::delivery::api::{PluginSendDto, SentDto};
use crate::modules::notifications::delivery::model::PluginSend;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{Caller, JsonBody, PathParams};
use crate::server::state::AppState;

/// Sends a notification from a plugin, of a kind its manifest declares.
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{id}/notifications",
    tag = "notifications.notification",
    params(("id" = String, Path, description = "The plugin's ID")),
    request_body = PluginSendDto,
    responses(
        (status = 200, description = "Sent, or not kept", body = SentDto),
        (status = 403, description = "`plugin_not_here`: the plugin does not run on the calling machine", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`plugin_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`plugin_disabled`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: a kind it did not declare, or a field out of range", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn send_plugin_notification(
    State(state): State<AppState>,
    caller: Caller,
    PathParams(plugin): PathParams<String>,
    JsonBody(body): JsonBody<PluginSendDto>,
) -> Result<Json<SentDto>, ApiError> {
    let sent = PluginSend {
        kind: body.kind,
        title: body.title,
        body: body.body,
        level: body.level,
        project: body.project,
        target: body.target,
        group_key: body.group_key,
    };
    let kept = state
        .plugin_notifications
        .send(&plugin, caller.machine(), sent)
        .await?;
    Ok(Json(SentDto {
        id: kept.map(|notification| notification.id),
    }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(send_plugin_notification))
}
