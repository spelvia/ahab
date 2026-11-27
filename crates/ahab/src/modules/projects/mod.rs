//! Projects: what you work on, each one folder on one machine.
//! Also reading a project's files, the folder picker, each project's
//! windows, searching its files and Git.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod files;
pub mod git;
pub mod layout;
pub mod project;
pub mod search;

pub use files::service::FileService;
pub use git::model::{Checkout, CheckoutKind, Checkouts};
pub use git::service::GitService;
pub use layout::service::LayoutService;
pub use project::model::{Availability, Project, ProjectMode, ProjectName, ProjectState};
pub use project::service::ProjectService;
pub use search::service::SearchService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(project::http::routes())
        .merge(files::http::routes())
        .merge(layout::http::routes())
        .merge(search::http::routes())
        .merge(git::http::routes())
}

/// This module's streams, outside the `OpenAPI` document.
pub fn stream_routes() -> axum::Router<AppState> {
    git::http::stream_routes().merge(files::http::stream_routes())
}
