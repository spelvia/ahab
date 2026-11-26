//! `/api/v1/projects/{id}/search` and `/files/index`.

use std::convert::Infallible;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::projects::search::api::{FileIndexDto, SearchQueryDto, SearchResultsDto};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams};
use crate::server::state::AppState;

/// Searches the project's files' contents, following `.gitignore`, at most
/// 2,000 matches and 10 seconds.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/search",
    tag = "projects.search",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = SearchQueryDto,
    responses(
        (status = 200, description = "The matches, by file", body = SearchResultsDto),
        (status = 404, description = "`project_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: a pattern that does not parse", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn search_files(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<SearchQueryDto>,
) -> Result<Json<SearchResultsDto>, ApiError> {
    Ok(Json(state.search.search(id, body).await?))
}

/// Searches the project's files' contents like `search`, but answers as
/// the machine finds them: newline-delimited JSON, a `SearchFrame` per line
/// (`files`, then one `done`). Closing the request stops the search.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/search/stream",
    tag = "projects.search",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = SearchQueryDto,
    responses(
        (status = 200, description = "`application/x-ndjson`: a `SearchFrame` per line"),
        (status = 404, description = "`project_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: a pattern that does not parse", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn stream_search(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<SearchQueryDto>,
) -> Result<Response, ApiError> {
    let channel = state.search.stream(id, body).await?;
    // Each frame is one JSON object; the line ends after it. The channel
    // lives as long as the body: a browser that goes away stops the search.
    let lines = futures_util::stream::unfold(channel, |mut channel| async move {
        let mut line = channel.recv().await?;
        line.push(b'\n');
        Some((Ok::<_, Infallible>(Bytes::from(line)), channel))
    });
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(lines),
    )
        .into_response())
}

/// The project's files, for quick open: paths following its ignore rules.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/files/index",
    tag = "projects.search",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "The paths", body = FileIndexDto),
        (status = 404, description = "`project_not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn file_index(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<FileIndexDto>, ApiError> {
    Ok(Json(state.search.index(id).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(search_files))
        .routes(routes!(stream_search))
        .routes(routes!(file_index))
}
