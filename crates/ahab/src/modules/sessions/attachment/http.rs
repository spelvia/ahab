//! `/api/v1/projects/{id}/attachments`: uploading a file to send with a
//! chat message, and reading it back (for thumbnails).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::sessions::attachment::api::AttachmentDto;
use crate::modules::sessions::attachment::model::MAX_BYTES;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{PathParams, QueryParams};
use crate::server::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UploadQuery {
    /// The file's name, such as `screenshot.png`.
    pub name: String,
}

/// Keeps a file on the project's machine, outside the project's folder, to send
/// with a chat message. The body is the file's bytes (at most 10 MiB).
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/attachments",
    tag = "sessions.attachment",
    params(("id" = Uuid, Path, description = "The project's ID"), UploadQuery),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 201, description = "Kept", body = AttachmentDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 413, description = "Larger than 10 MiB"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn upload_attachment(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<UploadQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<AttachmentDto>), ApiError> {
    let attachment = state.attachments.upload(id, &query.name, &body).await?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

/// A kept attachment's bytes.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/attachments/{attachment}/{name}",
    tag = "sessions.attachment",
    params(
        ("id" = Uuid, Path, description = "The project's ID"),
        ("attachment" = Uuid, Path, description = "The attachment's ID"),
        ("name" = String, Path, description = "Its file name"),
    ),
    responses(
        (status = 200, description = "The file", content_type = "application/octet-stream"),
        (status = 404, description = "`project_not_found`, `attachment_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_attachment(
    State(state): State<AppState>,
    PathParams((id, attachment, name)): PathParams<(Uuid, Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (bytes, content_type) = state.attachments.download(id, attachment, &name).await?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            // Attachments never change.
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_DISPOSITION, "inline"),
        ],
        bytes,
    ))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(upload_attachment))
        .routes(routes!(get_attachment))
        // Room for the largest attachment.
        .layer(DefaultBodyLimit::max(MAX_BYTES + 64 * 1024))
}
