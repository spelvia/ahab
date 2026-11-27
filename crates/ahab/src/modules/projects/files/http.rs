//! Files through the daemon: the folder picker on a machine, and reading a
//! project's folders and files.

use axum::Json;
use axum::extract::{DefaultBodyLimit, RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::projects::files::api::{
    CreateFileDto, FileContentDto, FileEntryDto, FileListingDto, FolderListingDto, HomeDto,
    MoveFileDto, MovedFileDto, RenameFileDto, SaveFileDto, SavedFileDto,
};
use crate::modules::projects::files::daemon::watch::MAX_FILES;
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

/// Room for a 2 MiB file's content, escaped as JSON.
const SAVE_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// Where the folder picker starts on a machine: its user's home folder.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{machine}/fs/home",
    tag = "projects.files",
    params(("machine" = String, Path, description = "The machine's name or ID")),
    responses(
        (status = 200, description = "The home folder", body = HomeDto),
        (status = 404, description = "`machine_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn home_folder(
    State(state): State<AppState>,
    PathParams(machine): PathParams<String>,
) -> Result<Json<HomeDto>, ApiError> {
    Ok(Json(state.files.home(&machine).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FoldersQuery {
    /// An absolute folder.
    pub path: String,
    /// Include folders whose name starts with a dot.
    #[serde(default)]
    pub hidden: bool,
}

/// The subfolders of a folder on a machine, for picking a project. Only
/// folder names, never files.
#[utoipa::path(
    get,
    path = "/api/v1/machines/{machine}/fs/directories",
    tag = "projects.files",
    params(("machine" = String, Path, description = "The machine's name or ID"), FoldersQuery),
    responses(
        (status = 200, description = "The subfolders", body = FolderListingDto),
        (status = 403, description = "`permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`machine_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_folders(
    State(state): State<AppState>,
    PathParams(machine): PathParams<String>,
    QueryParams(query): QueryParams<FoldersQuery>,
) -> Result<Json<FolderListingDto>, ApiError> {
    Ok(Json(
        state
            .files
            .folders(&machine, &query.path, query.hidden)
            .await?,
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PathQuery {
    /// Relative to the project, with `/`; empty for its root.
    #[serde(default)]
    pub path: String,
}

/// A folder's entries in a project: folders first, then files.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/files",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID"), PathQuery),
    responses(
        (status = 200, description = "The entries", body = FileListingDto),
        (status = 403, description = "`path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`, `folder_missing`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn list_files(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<PathQuery>,
) -> Result<Json<FileListingDto>, ApiError> {
    Ok(Json(state.files.list(id, &query.path).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReadQuery {
    /// Relative to the project, with `/`.
    #[serde(default)]
    pub path: String,
    /// For a file over 2 MiB, send its first part (`truncated`) instead of
    /// answering `file_too_large`.
    #[serde(default)]
    pub partial: bool,
}

/// A file's content in a project, with its version; files up to 2 MiB
/// (larger ones in part, when asked).
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/file",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID"), ReadQuery),
    responses(
        (status = 200, description = "The file", body = FileContentDto),
        (status = 403, description = "`path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`file_too_large`, `not_a_file`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn read_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<ReadQuery>,
) -> Result<Json<FileContentDto>, ApiError> {
    Ok(Json(
        state.files.read(id, &query.path, query.partial).await?,
    ))
}

/// A project image for the file viewer or a Markdown document.
#[utoipa::path(
    get, path = "/api/v1/projects/{id}/image", tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID"), PathQuery),
    responses((status = 200, description = "The image bytes", content_type = "image/*"),
              (status = 409, description = "Unsupported image or larger than 10 MiB", body = Problem))
)]
pub async fn project_image(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<PathQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let (bytes, mime) = state.files.image(id, &query.path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'; style-src 'unsafe-inline'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "private, no-cache"),
        ],
        bytes,
    ))
}

/// Saves a file: atomically, and only over the version the edit
/// started from (`expectedVersion`); a file changed meanwhile (by an agent,
/// Git or another client) is refused with `file_changed`. Without
/// `expectedVersion` it saves over whatever is there.
#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/file",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID"), PathQuery),
    request_body = SaveFileDto,
    responses(
        (status = 200, description = "Saved", body = SavedFileDto),
        (status = 403, description = "`read_only`, `path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`file_changed`, `file_too_large`, `not_a_file`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn save_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<PathQuery>,
    JsonBody(body): JsonBody<SaveFileDto>,
) -> Result<Json<SavedFileDto>, ApiError> {
    Ok(Json(
        state
            .files
            .write(
                id,
                &query.path,
                &body.content,
                body.expected_version.as_deref(),
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct DeleteQuery {
    /// Relative to the project, with `/`.
    pub path: String,
    /// Delete a file only at this version.
    #[serde(default)]
    pub expected_version: Option<String>,
}

/// Deletes a file, or a folder with everything in it.
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/file",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID"), DeleteQuery),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "`read_only`, `path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`file_changed`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .files
        .delete(id, &query.path, query.expected_version.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Makes a new file or folder, with its missing parent folders;
/// never over something that exists (`already_exists`).
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/files",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = CreateFileDto,
    responses(
        (status = 201, description = "Made", body = FileEntryDto),
        (status = 403, description = "`read_only`, `path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`already_exists`, `not_a_folder`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<CreateFileDto>,
) -> Result<(StatusCode, Json<FileEntryDto>), ApiError> {
    let entry = state
        .files
        .create(id, &body.path, body.kind, body.content.as_deref())
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

/// Moves (renames) a file or folder inside the project; never
/// over something that exists (`already_exists`).
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/files/rename",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = RenameFileDto,
    responses(
        (status = 200, description = "Moved", body = MovedFileDto),
        (status = 403, description = "`read_only`, `path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`already_exists`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn rename_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<RenameFileDto>,
) -> Result<Json<MovedFileDto>, ApiError> {
    Ok(Json(state.files.rename(id, &body.from, &body.to).await?))
}

/// Moves or copies a file or folder inside the project (dragging it in
/// the file tree); over what is there only with `replace`, never into
/// itself.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/files/move",
    tag = "projects.files",
    params(("id" = Uuid, Path, description = "The project's ID")),
    request_body = MoveFileDto,
    responses(
        (status = 200, description = "Moved or copied", body = MovedFileDto),
        (status = 403, description = "`read_only`, `path_outside_folder`, `permission_denied`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`project_not_found`, `not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`already_exists`, `machine_outdated`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`: into itself, or where it already is", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn move_file(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    JsonBody(body): JsonBody<MoveFileDto>,
) -> Result<Json<MovedFileDto>, ApiError> {
    Ok(Json(
        state
            .files
            .move_entry(id, &body.from, &body.to, body.copy, body.replace)
            .await?,
    ))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(home_folder))
        .routes(routes!(list_folders))
        .routes(routes!(list_files, create_file))
        .routes(routes!(project_image))
        .routes(routes!(read_file, save_file, delete_file))
        .routes(routes!(rename_file))
        .routes(routes!(move_file))
        .layer(DefaultBodyLimit::max(SAVE_BODY_LIMIT))
}

/// The `path` values of a query, in order (`?path=a&path=b`).
fn paths(query: Option<&str>) -> Vec<String> {
    form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .filter(|(key, _)| key == "path")
        .map(|(_, value)| value.into_owned())
        .take(MAX_FILES)
        .collect()
}

/// Tells the browser when open files change on disk (server-sent events,
/// outside the `OpenAPI` document): `{"type":"watching",
/// "versions":{path: version}}` once the machine watches them, then
/// `{"type":"changed","path":…,"version":…}` per change (`version` null once
/// a file is gone), or `{"type":"unwatched","reason":…}` when it cannot.
/// Each `path` query value is a file; closing the stream stops the watch.
async fn watch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    RawQuery(query): RawQuery,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    ApiError,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    let channel = state.files.watch(id, paths(query.as_deref())).await?;
    // The channel lives as long as the stream: a browser that goes away
    // stops the watch.
    let events = futures_util::stream::unfold(channel, |mut channel| async move {
        let frame = channel.recv().await?;
        let data = String::from_utf8_lossy(&frame).into_owned();
        Some((Ok(Event::default().data(data)), channel))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

/// The watch stream, outside the `OpenAPI` document.
pub fn stream_routes() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/api/v1/projects/{id}/files/watch",
        axum::routing::get(watch),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_path_of_a_query() {
        assert_eq!(
            paths(Some("path=src%2Fa.rs&other=1&path=b%20c.md")),
            ["src/a.rs", "b c.md"]
        );
        assert!(paths(None).is_empty());
    }
}
