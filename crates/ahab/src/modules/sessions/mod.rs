//! Sessions: pieces of work in a project, each holding chats with coding
//! agents (and later terminals) and its own tabs.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod attachment;
pub mod chat;
pub mod reference;
pub mod session;
pub mod terminal;

pub use attachment::service::AttachmentService;
pub use chat::model::{Chat, ChatState, ChatStatus, Transition, edited_files};
pub use chat::service::{ChatService, ChatTransition};
pub use session::model::{Session, SessionState};
pub use session::service::SessionService;
pub use terminal::service::TerminalService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(session::http::routes())
        .merge(chat::http::routes())
        .merge(attachment::http::routes())
        .merge(terminal::http::routes())
}

/// The live streams, outside the `OpenAPI` document: chats' (Server-Sent
/// Events) and terminals' attachments (web sockets).
pub fn stream_routes() -> axum::Router<AppState> {
    chat::http::stream_routes().merge(terminal::http::socket_routes())
}
