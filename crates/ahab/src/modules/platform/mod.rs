//! The platform: the database, which every other module builds on,
//! Ahab's core features, on or off, and Ahab's own settings.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod database;
pub mod feature;
pub mod setting;

pub use feature::service::FeatureService;
pub use setting::service::CoreSettingService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    feature::http::routes().merge(setting::http::routes())
}
