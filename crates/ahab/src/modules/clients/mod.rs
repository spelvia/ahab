//! Clients: the browsers, phones, command lines and scripts
//! paired with this server, and pairing new ones. Machines are not clients:
//! they join with their own keys (`machines`).

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod client;
pub mod pairing;

pub use client::model::{Client, ClientKind, Seen};
pub use client::service::ClientService;
pub use pairing::service::PairingService;

/// Every route of this module, merged.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(client::http::routes())
        .merge(pairing::http::routes())
}
