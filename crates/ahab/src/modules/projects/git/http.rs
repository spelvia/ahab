//! `/api/v1/projects/{id}/git/…`: Git in a project.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::modules::projects::git::api::{
    BranchesDto, CommitDetailDto, CommitRequestDto, CreateBranchDto, DiffDto, DiffGroup, GitOp,
    LogDto, OutputDto, PathsDto, RepositoriesDto, StagePartDto, StashRequestDto, StashesDto,
    StatusDto, SwitchDto,
};
use crate::server::error::{ApiError, Problem};
use crate::server::extract::{JsonBody, PathParams, QueryParams};
use crate::server::state::AppState;

/// The errors of Git operations that only read, for the API document.
#[derive(utoipa::IntoResponses)]
#[expect(dead_code, reason = "describes responses; never built")]
enum GitReads {
    #[response(
        status = 403,
        description = "`path_outside_folder`",
        content_type = "application/problem+json"
    )]
    Forbidden(Problem),
    #[response(
        status = 404,
        description = "`project_not_found`, `not_a_repository`, `folder_missing`",
        content_type = "application/problem+json"
    )]
    NotFound(Problem),
    #[response(
        status = 409,
        description = "`git_busy`, `git_missing`, `git_timeout`, `git_failed`",
        content_type = "application/problem+json"
    )]
    Conflict(Problem),
    #[response(
        status = 422,
        description = "`validation_failed`",
        content_type = "application/problem+json"
    )]
    Invalid(Problem),
    #[response(
        status = 503,
        description = "`machine_offline`, `project_unknown`",
        content_type = "application/problem+json"
    )]
    Unavailable(Problem),
}

/// The errors of Git operations that change the repository or talk to a
/// remote, for the API document.
#[derive(utoipa::IntoResponses)]
#[expect(dead_code, reason = "describes responses; never built")]
enum GitWrites {
    #[response(
        status = 403,
        description = "`read_only`, `path_outside_folder`",
        content_type = "application/problem+json"
    )]
    Forbidden(Problem),
    #[response(
        status = 404,
        description = "`project_not_found`, `not_a_repository`, `folder_missing`",
        content_type = "application/problem+json"
    )]
    NotFound(Problem),
    #[response(
        status = 409,
        description = "`git_busy`, `git_missing`, `git_timeout`, `identity_missing`, `nothing_to_commit`, `nothing_to_continue`, `would_overwrite_changes`, `credentials_needed`, `merge_conflict`, `branch_not_merged`, `branch_exists`, `hook_failed`, `no_remote`, `index_changed`, `git_failed`",
        content_type = "application/problem+json"
    )]
    Conflict(Problem),
    #[response(
        status = 422,
        description = "`validation_failed`",
        content_type = "application/problem+json"
    )]
    Invalid(Problem),
    #[response(
        status = 503,
        description = "`machine_offline`, `project_unknown`",
        content_type = "application/problem+json"
    )]
    Unavailable(Problem),
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct DiffQuery {
    /// A repository inside the project's folder; absent for the one it is in.
    pub repository: Option<String>,
    /// Relative to the repository.
    pub path: String,
    /// Before a rename.
    pub old_path: Option<String>,
    #[param(inline)]
    pub group: DiffGroup,
    /// With `group=commit`.
    pub commit: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LogQuery {
    /// A repository inside the project's folder; absent for the one it is in.
    pub repository: Option<String>,
    pub skip: Option<u32>,
    /// At most 200; 50 by default.
    pub limit: Option<u32>,
    /// Every branch, remote branch and tag, not only the current branch's history.
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PopQuery {
    /// A repository inside the project's folder; absent for the one it is in.
    pub repository: Option<String>,
    /// Remove the stash once applied.
    pub pop: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RepoQuery {
    /// A repository inside the project's folder (its path there); absent
    /// for the repository the folder is in.
    pub repository: Option<String>,
}

/// A branch's new name.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = GitRename)]
pub struct RenameDto {
    pub from: String,
    pub to: String,
}

/// A branch to delete.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = GitDeleteBranch)]
pub struct DeleteBranchDto {
    pub name: String,
    /// Also when it is not merged.
    #[serde(default)]
    pub force: bool,
}

/// The project's Git status: branch, upstream, ahead and behind, and its changes by group (an empty answer with `repository: false` outside any repository).
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/status",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = StatusDto),
        GitReads,
    )
)]
pub async fn git_status(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<StatusDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Status)
            .await?,
    ))
}

/// A file's two sides, for a group or a commit, to show side by side.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/diff",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), DiffQuery),
    responses(
        (status = 200, description = "Done", body = DiffDto),
        GitReads,
    )
)]
pub async fn git_diff(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<DiffQuery>,
) -> Result<Json<DiffDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Diff {
                    path: query.path,
                    old_path: query.old_path,
                    group: query.group,
                    commit: query.commit,
                },
            )
            .await?,
    ))
}

/// Stages files, or everything.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/stage",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = PathsDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_stage(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<PathsDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Stage {
                    paths: body.paths,
                    all: body.all,
                },
            )
            .await?,
    ))
}

/// Sets a file's staged content: stages or unstages hunks and lines.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/stage-part",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = StagePartDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_stage_part(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<StagePartDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::StagePart {
                    path: body.path,
                    content: body.content,
                    index: body.index,
                },
            )
            .await?,
    ))
}

/// Unstages files, or everything.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/unstage",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = PathsDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_unstage(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<PathsDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Unstage {
                    paths: body.paths,
                    all: body.all,
                },
            )
            .await?,
    ))
}

/// Discards files' changes in the working tree; untracked files are deleted. Only inside the project's folder.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/discard",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = PathsDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_discard(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<PathsDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Discard { paths: body.paths },
            )
            .await?,
    ))
}

/// Commits what is staged, or amends the last commit.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/commit",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = CommitRequestDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_commit(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<CommitRequestDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Commit {
                    message: body.message,
                    amend: body.amend,
                },
            )
            .await?,
    ))
}

/// Local and remote branches, most recent first.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/branches",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = BranchesDto),
        GitReads,
    )
)]
pub async fn git_branches(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<BranchesDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Branches)
            .await?,
    ))
}

/// Switches branch (a remote one: a local branch tracking it), stashing changes first when asked.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/switch",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = SwitchDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_switch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<SwitchDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Switch {
                    branch: body.branch,
                    remote: body.remote,
                    stash: body.stash,
                },
            )
            .await?,
    ))
}

/// Creates a branch, from the current one or `from`, and switches to it.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/branches",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = CreateBranchDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_create_branch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<CreateBranchDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::CreateBranch {
                    name: body.name,
                    from: body.from,
                },
            )
            .await?,
    ))
}

/// Renames a local branch.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/branches/rename",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = RenameDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_rename_branch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<RenameDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::RenameBranch {
                    from: body.from,
                    to: body.to,
                },
            )
            .await?,
    ))
}

/// Deletes a local branch; one not merged only with `force`.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/branches/delete",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = DeleteBranchDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_delete_branch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<DeleteBranchDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::DeleteBranch {
                    name: body.name,
                    force: body.force,
                },
            )
            .await?,
    ))
}

/// Fetches every remote.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/fetch",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_fetch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Fetch)
            .await?,
    ))
}

/// Pulls the current branch, as the machine's Git is configured to.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/pull",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_pull(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Pull)
            .await?,
    ))
}

/// Pushes the current branch, setting its upstream when it has none.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/push",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_push(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Push)
            .await?,
    ))
}

/// Commits, newest first, paged: the current branch's, or with `all`
/// every branch's, remote branch's and tag's (the graph), parents after
/// their children.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/log",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), LogQuery),
    responses(
        (status = 200, description = "Done", body = LogDto),
        GitReads,
    )
)]
pub async fn git_log(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<LogQuery>,
) -> Result<Json<LogDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Log {
                    skip: query.skip.unwrap_or(0),
                    limit: query.limit.unwrap_or(50),
                    all: query.all.unwrap_or(false),
                },
            )
            .await?,
    ))
}

/// A commit: its message and files.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/commits/{commit}",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery, ("commit" = String, Path, description = "The commit's hash")),
    responses(
        (status = 200, description = "Done", body = CommitDetailDto),
        GitReads,
    )
)]
pub async fn git_show(
    State(state): State<AppState>,
    PathParams((id, commit)): PathParams<(Uuid, String)>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<CommitDetailDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Show { commit })
            .await?,
    ))
}

/// The stashes, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/stashes",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = StashesDto),
        GitReads,
    )
)]
pub async fn git_stashes(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<StashesDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Stashes)
            .await?,
    ))
}

/// Stashes the changes.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/stashes",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = StashRequestDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_stash(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<StashRequestDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::Stash {
                    message: body.message,
                    untracked: body.untracked,
                },
            )
            .await?,
    ))
}

/// Applies a stash; with `pop`, removes it once applied.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/stashes/{index}/apply",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), ("index" = u32, Path, description = "The stash's index"), PopQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_stash_apply(
    State(state): State<AppState>,
    PathParams((id, index)): PathParams<(Uuid, u32)>,
    QueryParams(query): QueryParams<PopQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::StashApply {
                    index,
                    pop: query.pop.unwrap_or(false),
                },
            )
            .await?,
    ))
}

/// Drops a stash.
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/git/stashes/{index}",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery, ("index" = u32, Path, description = "The stash's index")),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_stash_drop(
    State(state): State<AppState>,
    PathParams((id, index)): PathParams<(Uuid, u32)>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::StashDrop { index })
            .await?,
    ))
}

/// Marks conflicted files resolved (stages them).
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/resolve",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    request_body = PathsDto,
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_resolve(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
    JsonBody(body): JsonBody<PathsDto>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(
                id,
                query.repository.clone(),
                GitOp::MarkResolved { paths: body.paths },
            )
            .await?,
    ))
}

/// Continues the unfinished merge, rebase, cherry-pick or revert.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/continue",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_continue(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Continue)
            .await?,
    ))
}

/// Aborts the unfinished merge, rebase, cherry-pick or revert.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/abort",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_abort(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Abort)
            .await?,
    ))
}

/// Makes the project's folder a Git repository.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/git/init",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID"), RepoQuery),
    responses(
        (status = 200, description = "Done", body = OutputDto),
        GitWrites,
    )
)]
pub async fn git_init(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<Json<OutputDto>, ApiError> {
    Ok(Json(
        state
            .git
            .run(id, query.repository.clone(), GitOp::Init)
            .await?,
    ))
}

/// Where the project's Git is: the repository its folder is in, or the
/// repositories inside its folder.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/git/repositories",
    tag = "projects.git",
    params(("id" = Uuid, Path, description = "The project's ID")),
    responses(
        (status = 200, description = "The repositories", body = RepositoriesDto),
        (status = 404, description = "`project_not_found`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`machine_offline`", body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn git_repositories(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
) -> Result<Json<RepositoriesDto>, ApiError> {
    Ok(Json(state.git.run(id, None, GitOp::Repositories).await?))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(git_repositories))
        .routes(routes!(git_status))
        .routes(routes!(git_diff))
        .routes(routes!(git_stage))
        .routes(routes!(git_stage_part))
        .routes(routes!(git_unstage))
        .routes(routes!(git_discard))
        .routes(routes!(git_commit))
        .routes(routes!(git_branches, git_create_branch))
        .routes(routes!(git_switch))
        .routes(routes!(git_rename_branch))
        .routes(routes!(git_delete_branch))
        .routes(routes!(git_fetch))
        .routes(routes!(git_pull))
        .routes(routes!(git_push))
        .routes(routes!(git_log))
        .routes(routes!(git_show))
        .routes(routes!(git_stashes, git_stash))
        .routes(routes!(git_stash_apply))
        .routes(routes!(git_stash_drop))
        .routes(routes!(git_resolve))
        .routes(routes!(git_continue))
        .routes(routes!(git_abort))
        .routes(routes!(git_init))
}

/// Tells the browser when the checkout changes (server-sent events, outside
/// the `OpenAPI` document): `{"type":"watching"}` once its machine watches
/// it, `{"type":"changed"}` after changes, `{"type":"unwatched","reason":…}`
/// when it cannot (then keep asking for the status). Closing it stops the
/// watch.
async fn watch(
    State(state): State<AppState>,
    PathParams(id): PathParams<Uuid>,
    QueryParams(query): QueryParams<RepoQuery>,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    ApiError,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    let channel = state.git.watch(id, query.repository).await?;
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
    axum::Router::new().route("/api/v1/projects/{id}/git/watch", axum::routing::get(watch))
}
