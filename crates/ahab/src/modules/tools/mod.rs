//! Language tooling: language servers, formatters, linters and
//! check commands on each project's machine. `registry` holds the tool
//! records and installing, `lsp` the servers and windows' channels,
//! `problems` Diagnostics, checks and turns, `setting` the choices per
//! machine and project.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod activity;
pub mod lsp;
pub mod problems;
pub mod registry;
pub mod setting;

pub use activity::service::RepositoryActivityService;
pub use lsp::service::LanguageServerService;
pub use problems::service::ProblemService;
pub use registry::service::ToolService;
pub use setting::service::ToolSettingService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    registry::http::routes()
        .merge(lsp::http::routes())
        .merge(problems::http::routes())
        .merge(setting::http::routes())
        .merge(activity::http::routes())
}

/// This module's routes outside the `OpenAPI` document.
pub fn stream_routes() -> axum::Router<AppState> {
    lsp::http::socket_routes()
}
