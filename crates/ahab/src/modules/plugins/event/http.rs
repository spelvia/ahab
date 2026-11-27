//! `GET /api/v1/plugin-events`: Ahab's and plugins' events as server-sent
//! events, outside the `OpenAPI` document.

use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

use crate::server::extract::QueryParams;
use crate::server::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Only these events, separated by commas; every event when absent.
    pub names: Option<String>,
    /// Announce stream capabilities and reconnect when events have been missed.
    pub metadata: Option<bool>,
}

/// Every event from now on, each `{"name":…,"data":…}`.
async fn events(
    State(state): State<AppState>,
    QueryParams(query): QueryParams<EventsQuery>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let wanted: Option<HashSet<String>> = query.names.map(|names| {
        names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect()
    });
    let heard = state.presence.bus().subscribe();
    let metadata = query.metadata.unwrap_or(false);
    let stream =
        futures_util::stream::unfold((heard, wanted), move |(mut heard, wanted)| async move {
            loop {
                match heard.recv().await {
                    Ok(event) => {
                        if wanted
                            .as_ref()
                            .is_some_and(|names| !names.contains(&event.name))
                        {
                            continue;
                        }
                        let data = json!({ "name": event.name, "data": event.data }).to_string();
                        return Some((Ok(SseEvent::default().data(data)), (heard, wanted)));
                    }
                    // Reconnecting tells clients to reload state that may have changed.
                    Err(broadcast::error::RecvError::Lagged(_)) if metadata => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
    let announced = metadata.then(|| {
        Ok(SseEvent::default()
            .data(json!({ "name": "stream.ready", "data": { "sessions": true } }).to_string()))
    });
    Sse::new(futures_util::stream::iter(announced).chain(stream))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

pub fn stream_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/v1/plugin-events", axum::routing::get(events))
}
