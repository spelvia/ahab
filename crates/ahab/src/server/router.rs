//! The REST API, the `OpenAPI` document, the API docs page, and the web app.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::routing::{any, get};
use axum::{Json, Router};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};
use uuid::Uuid;

use crate::server::error::{self, ApiError};
use crate::server::state::AppState;
use crate::server::web::{self, WebApp};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Ahab API",
        description = "A self-hostable control plane for human-led, agent-assisted software work."
    ),
    // One tag per component, named `module.component`, so the UI's client
    // has one folder per component.
    tags(
        (name = "server.health", description = "Whether the server answers"),
        (name = "platform.feature", description = "Ahab's core features, on or off"),
        (name = "platform.setting", description = "Ahab's own settings, such as the editor's, per server and project"),
        (name = "machines.machine", description = "Computers that run a daemon"),
        (name = "machines.enrollment", description = "Joining machines: approval and one-time codes"),
        (name = "machines.agents", description = "The coding agents each machine can run"),
        (name = "machines.metrics", description = "A machine's CPU, memory, disks and processes, live"),
        (name = "machines.usage", description = "The plan limits of the agents signed in on each machine"),
        (name = "projects.project", description = "Projects: what you work on, each one folder on one machine"),
        (name = "projects.files", description = "A project's files: reading, saving, creating, renaming and deleting them; and the folder picker"),
        (name = "projects.layout", description = "Each project's saved layout"),
        (name = "sessions.session", description = "Sessions: pieces of work in a project"),
        (name = "sessions.chat", description = "Chats: conversations with coding agents in a session"),
        (name = "plugins.plugin", description = "Plugins: install, enable, place and remove; their states and logs"),
        (name = "plugins.runtime", description = "Plugins' commands, run on machines"),
        (name = "plugins.setting", description = "Plugins' settings, per server, machine and project"),
        (name = "clients.client", description = "Clients paired with this server: browsers, phones, command lines, tokens"),
        (name = "clients.pairing", description = "Pairing new clients: links, codes and requests"),
        (name = "tools.registry", description = "Language tools on each machine: the registry's records, their states, runtimes; installing, updating and removing them"),
        (name = "tools.lsp", description = "The language servers each machine runs, their logs and restarting"),
        (name = "tools.problems", description = "Problems: diagnostics per project, check commands, project totals and what agent turns changed"),
        (name = "tools.setting", description = "Which language tools run, where and how they start, per machine and project"),
        (name = "notifications.notification", description = "Notifications: the inbox, reading and clearing them, and the server's settings"),
        (name = "notifications.push", description = "Web Push: browsers that get notifications while Ahab is closed"),
    ),
    components(schemas(
        error::Problem,
        error::FieldError,
        // Streamed as lines, so no response names it.
        crate::modules::projects::search::api::SearchFrameDto
    ))
)]
struct ApiDoc;

fn api() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::server::health::routes())
        .merge(crate::modules::platform::routes())
        .merge(crate::modules::notifications::routes())
        .merge(crate::modules::machines::routes())
        .merge(crate::modules::projects::routes())
        .merge(crate::modules::sessions::routes())
        .merge(crate::modules::plugins::routes())
        .merge(crate::modules::clients::routes())
        .merge(crate::modules::tools::routes())
        .split_for_parts()
}

/// The `OpenAPI` document, as served at `/api/v1/openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api().1
}

/// The whole HTTP application: the API, its documentation, and the web app
/// as `web` says.
pub fn router(state: AppState, web: &WebApp) -> Router {
    let (api_routes, openapi) = api();
    let openapi_json = openapi.clone();

    let allowed_hosts = Arc::clone(&state.allowed_hosts);
    let guarded = state.clone();
    let mut router = Router::new()
        .merge(api_routes)
        .merge(crate::modules::machines::socket_routes())
        .merge(crate::modules::sessions::stream_routes())
        .merge(crate::modules::projects::stream_routes())
        .merge(crate::modules::plugins::stream_routes())
        .merge(crate::modules::tools::stream_routes())
        .merge(crate::modules::notifications::stream_routes())
        .route(
            "/api/v1/openapi.json",
            get(move || async move { Json(openapi_json) }),
        )
        .merge(Scalar::with_url("/api/docs", openapi))
        .route("/api/{*rest}", any(api_not_found))
        .with_state(state);

    router = web::serve(router, web);

    let trace = TraceLayer::new_for_http().make_span_with(|request: &Request| {
        let request_id = request
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("-");
        tracing::info_span!(
            "request",
            method = %request.method(),
            uri = %request.uri(),
            request_id = %request_id,
        )
    });

    // The last layer added runs first: the request ID is set before anything
    // else sees the request, and compression runs after the request ID has
    // been added to error bodies.
    // The host check runs before the other guards.
    router
        .layer(axum::middleware::from_fn_with_state(
            guarded,
            super::access::guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            allowed_hosts,
            super::hosts::check,
        ))
        .layer(CatchPanicLayer::custom(error::panic_response))
        .layer(axum::middleware::from_fn(error::attach_request_id))
        .layer(CompressionLayer::new())
        .layer(trace)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuidV7))
}

/// Request IDs are UUID v7, like every other ID, so they sort by time.
#[derive(Clone, Copy)]
struct MakeRequestUuidV7;

impl MakeRequestId for MakeRequestUuidV7 {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        HeaderValue::from_str(&Uuid::now_v7().to_string())
            .ok()
            .map(RequestId::new)
    }
}

async fn api_not_found() -> ApiError {
    ApiError::not_found("There is nothing at this path.")
}
