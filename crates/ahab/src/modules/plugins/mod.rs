//! Plugins: packages that add to Ahab's web app, server and
//! daemons. `plugin` installs and enables them.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod command;
pub mod event;
pub mod host;
pub mod plugin;
pub mod setting;

pub use command::service::CommandService;
pub use host::service::PluginHostService;
pub use plugin::model::Plugin;
pub use plugin::service::{PluginPaths, PluginService};
pub use setting::service::SettingService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    plugin::http::routes()
        .merge(host::http::routes())
        .merge(command::http::routes())
        .merge(setting::http::routes())
}

/// This module's routes outside the `OpenAPI` document.
pub fn stream_routes() -> axum::Router<AppState> {
    plugin::http::file_routes()
        .merge(event::http::stream_routes())
        .merge(command::http::socket_routes())
}
