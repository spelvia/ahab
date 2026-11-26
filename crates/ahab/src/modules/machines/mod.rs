//! Machines: the server's record of each computer that runs a daemon.

use utoipa_axum::router::OpenApiRouter;

use crate::server::state::AppState;

pub mod agents;
pub mod enrollment;
pub mod identity;
pub mod machine;
pub mod metrics;
pub mod presence;
pub mod usage;

pub use agents::model::{
    AGENTS, AgentState, AgentStatus, Harness, added as added_agent, harness, label as agent_label,
};
pub use agents::service::AgentService;
pub use enrollment::service::EnrollmentService;
pub use identity::model::MachineKey;
pub use machine::model::{Fingerprint, Machine, MachineName, MachineState, PublicKey};
pub use machine::service::MachineService;
pub use metrics::service::MetricsService;
pub use presence::service::{Channel, MachineEvent, Presence, is_shared, shared_error};
pub use usage::service::UsageService;

/// Every route of this module, merged. The daemon connection is a WebSocket,
/// so it is a plain route outside the `OpenAPI` document.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(machine::http::routes())
        .merge(agents::http::routes())
        .merge(usage::http::routes())
        .merge(metrics::http::routes())
        .merge(enrollment::http::routes())
}

/// The daemon connection's route.
pub fn socket_routes() -> axum::Router<AppState> {
    presence::http::routes()
}
