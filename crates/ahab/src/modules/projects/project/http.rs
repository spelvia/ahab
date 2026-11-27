//! `/api/v1/projects`: list, create, change and delete projects, and open a
//! folder.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use axum::http::header;
use axum::response::IntoResponse;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::modules::projects::project::api::{
    CreateProjectDto, OpenFolderDto, OpenedFolderDto, ProjectDto, ProjectListDto, ProjectOrderDto,
    ProjectStateDto, SetProjectIconDto, UpdateProjectDto,
};
use crate::modules::projects::project::model::{self, Project, ProjectMode, ProjectState};
use crate::modules::projects::project::service::{NewProject, ProjectChanges};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;
use crate::shared::error::{DomainError, FieldError, ServiceError};

/// One project as the API shows it, with its machine's name.
pub async fn project_dto(state: &AppState, project: &Project) -> Result<ProjectDto, ApiError> {
    project_dto_with(state, project, &mut HashMap::new()).await
}

/// A project as the API shows it, reusing machines' names already looked up
/// (`names` carries them over between calls, as a list of projects does).
pub async fn project_dto_with(
    state: &AppState,
    project: &Project,
    names: &mut HashMap<Uuid, String>,
) -> Result<ProjectDto, ApiError> {
    let name = if let Some(name) = names.get(&project.machine_id) {
        name.clone()
    } else {
        let name = state.projects.machine_of(project).await?.name.to_string();
        names.insert(project.machine_id, name.clone());
        name
    };
    Ok(ProjectDto::new(
        project,
        name,
        state.projects.availability(project),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// `archived` lists archived projects instead.
    pub state: Option<ProjectStateDto>,
}

/// Orders the projects by hand, as the project switcher lists them.
#[utoipa::path(
    put,
    path = "/api/v1/project-order",
    tag = "projects.project",
    request_body = ProjectOrderDto,
    responses(
        (status = 204, description = "Ordered"),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_project_order(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<ProjectOrderDto>,
) -> Result<StatusCode, ApiError> {
    state.projects.set_order(&body.ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A project's image mark.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}/icon",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 200, description = "The image", content_type = "image/*"),
        (status = 404, description = "`project_not_found`, `icon_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_project_icon(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<impl IntoResponse, ApiError> {
    let project = state.projects.find(&project).await?;
    let (content_type, bytes) = state.projects.icon_image(project.id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            // Its address changes with it (`?v=`).
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_owned(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
        bytes,
    ))
}

/// Gives a project its own mark: an emoji, or a small image.
#[utoipa::path(
    put,
    path = "/api/v1/projects/{project}/icon",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    request_body = SetProjectIconDto,
    responses(
        (status = 200, description = "The project", body = ProjectDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn set_project_icon(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
    JsonBody(body): JsonBody<SetProjectIconDto>,
) -> Result<Json<ProjectDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let invalid = |message: &str| {
        ApiError::from(ServiceError::from(DomainError::Invalid(vec![
            FieldError::new("icon", message),
        ])))
    };
    let icon = match (body.emoji, body.image) {
        (Some(emoji), None) => model::icon_emoji(&emoji).map_err(ServiceError::from)?,
        (None, Some(image)) => {
            let (head, data) = image
                .strip_prefix("data:")
                .and_then(|rest| rest.split_once(','))
                .ok_or_else(|| invalid("Send the image as a data: URL."))?;
            let content_type = head
                .strip_suffix(";base64")
                .ok_or_else(|| invalid("Send the image base64-encoded."))?;
            let bytes = BASE64
                .decode(data)
                .map_err(|_| invalid("The image is not valid base64."))?;
            model::icon_image(content_type, bytes).map_err(ServiceError::from)?
        }
        _ => return Err(invalid("Send an emoji or an image.")),
    };
    let project = state.projects.set_icon(project.id, Some(icon)).await?;
    Ok(Json(project_dto(&state, &project).await?))
}

/// Takes a project's own mark away: its initial shows again.
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{project}/icon",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 200, description = "The project", body = ProjectDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn remove_project_icon(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<Json<ProjectDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let project = state.projects.set_icon(project.id, None).await?;
    Ok(Json(project_dto(&state, &project).await?))
}

/// Projects in the order chosen by hand, then by name: active ones, or archived ones.
#[utoipa::path(
    get,
    path = "/api/v1/projects",
    tag = "projects.project",
    params(ListQuery),
    responses((status = 200, description = "The projects", body = ProjectListDto))
)]
pub async fn list_projects(
    State(state): State<AppState>,
    QueryParams(query): QueryParams<ListQuery>,
) -> Result<Json<ProjectListDto>, ApiError> {
    let wanted = query.state.map_or(ProjectState::Active, Into::into);
    let mut items = Vec::new();
    let mut names = HashMap::new();
    for project in state.projects.list(wanted).await? {
        items.push(project_dto_with(&state, &project, &mut names).await?);
    }
    Ok(Json(ProjectListDto { items }))
}

/// Creates a project for a folder on a machine. The machine must be online,
/// to confirm the folder.
#[utoipa::path(
    post,
    path = "/api/v1/projects",
    tag = "projects.project",
    request_body = CreateProjectDto,
    responses(
        (status = 201, description = "The new project", body = ProjectDto),
        (status = 403, description = "`folder_not_readable`", body = Problem, content_type = "application/problem+json"),
        (status = 404, description = "`machine_not_found`, `folder_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`machine_pending`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<CreateProjectDto>,
) -> Result<(StatusCode, Json<ProjectDto>), ApiError> {
    let project = state
        .projects
        .create(NewProject {
            name: body.name,
            description: body.description,
            machine: body.machine,
            path: body.path,
            mode: body.mode.map_or(ProjectMode::ReadWrite, Into::into),
            group: body.group,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(project_dto(&state, &project).await?),
    ))
}

/// Opens a folder: its project if it is one, else a new project named after
/// the folder. What `ahab open` and the first-run picker use.
#[utoipa::path(
    post,
    path = "/api/v1/projects/open",
    tag = "projects.project",
    request_body = OpenFolderDto,
    responses(
        (status = 200, description = "The project", body = OpenedFolderDto),
        (status = 404, description = "`machine_not_found`, `folder_not_found`, `project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`folder_in_several_projects`, `project_elsewhere`, `project_name_ambiguous`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn open_folder(
    State(state): State<AppState>,
    JsonBody(body): JsonBody<OpenFolderDto>,
) -> Result<Json<OpenedFolderDto>, ApiError> {
    let machine = match (&body.machine, &body.machine_fingerprint) {
        (Some(machine), _) => machine.clone(),
        (None, Some(fingerprint)) => state
            .machines
            .find_by_fingerprint(fingerprint)
            .await?
            .id
            .to_string(),
        (None, None) => state
            .machines
            .list()
            .await?
            .into_iter()
            .find(|machine| machine.is_local)
            .map(|machine| machine.id.to_string())
            .ok_or_else(|| {
                ApiError::not_found("This server has no machine of its own: say which machine.")
            })?,
    };
    let opened = state
        .projects
        .open(&machine, &body.path, body.project.as_deref())
        .await?;
    Ok(Json(OpenedFolderDto {
        project: project_dto(&state, &opened.project).await?,
        created: opened.created,
    }))
}

/// One project, by name or ID.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 200, description = "The project", body = ProjectDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_project(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<Json<ProjectDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    Ok(Json(project_dto(&state, &project).await?))
}

/// Renames, describes, groups, archives or restores a project, or changes
/// its mode.
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{project}",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    request_body = UpdateProjectDto,
    responses(
        (status = 200, description = "The project", body = ProjectDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 422, description = "`validation_failed`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn update_project(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
    JsonBody(body): JsonBody<UpdateProjectDto>,
) -> Result<Json<ProjectDto>, ApiError> {
    let project = state.projects.find(&project).await?;
    let project = state
        .projects
        .update(
            project.id,
            ProjectChanges {
                name: body.name,
                description: body.description.map(Some),
                state: body.state.map(Into::into),
                mode: body.mode.map(Into::into),
                group: body.group.map(Some),
            },
        )
        .await?;
    Ok(Json(project_dto(&state, &project).await?))
}

/// Deletes a project and everything Ahab keeps for it. Files are never
/// touched.
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{project}",
    tag = "projects.project",
    params(("project" = String, Path, description = "The project's name or ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn delete_project(
    State(state): State<AppState>,
    PathParams(project): PathParams<String>,
) -> Result<StatusCode, ApiError> {
    let project = state.projects.find(&project).await?;
    state.projects.delete(project.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_projects, create_project))
        .routes(routes!(open_folder))
        .routes(routes!(get_project, update_project, delete_project))
        .routes(routes!(set_project_order))
        .routes(routes!(
            get_project_icon,
            set_project_icon,
            remove_project_icon
        ))
}
