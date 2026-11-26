//! Notifications: what happened that the server's person should
//! know about (an agent finished, needs them, failed), kept for the inbox
//! and sent live to every open app.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod delivery;
pub mod notification;
pub mod push;

pub use delivery::service::PluginNotifications;
pub use notification::service::{
    ChatSources, Intercept, NotificationService, ReleaseCheck, Watched,
};
pub use push::model::VapidKey;
pub use push::service::PushService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    notification::http::routes()
        .merge(push::http::routes())
        .merge(delivery::http::routes())
}

/// The live stream, outside the `OpenAPI` document.
pub fn stream_routes() -> axum::Router<AppState> {
    notification::http::stream_routes()
}
