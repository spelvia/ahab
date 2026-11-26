//! Asking a project's machine to run Git.

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::modules::machines::{Channel, Presence, shared_error};
use crate::modules::projects::git::api::{GitOp, GitRequest};
use crate::modules::projects::project::service::ProjectService;
use crate::shared::error::{FieldError, ServiceError};

/// The daemon gives remotes 5 minutes; this leaves room to answer.
const NETWORK: Duration = Duration::from_secs(330);
const LOCAL: Duration = Duration::from_secs(75);

pub struct GitService {
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
}

/// The daemon's own Git errors, as the API reports them; the codes every
/// project request shares are mapped by `Presence::ask`.
fn git_error(code: &str, message: &str) -> Option<ServiceError> {
    let message = message.to_owned();
    let conflict = |code: &'static str| {
        Some(ServiceError::Conflict {
            code,
            message: message.clone(),
        })
    };
    match code {
        "not_a_repository" => Some(ServiceError::NotFound {
            code: "not_a_repository",
            message,
        }),
        "invalid_path" | "invalid_commit" | "invalid_branch" | "invalid_message" => {
            Some(ServiceError::Invalid(vec![FieldError::new(
                match code {
                    "invalid_path" => "paths",
                    "invalid_commit" => "commit",
                    "invalid_message" => "message",
                    _ => "branch",
                },
                message,
            )]))
        }
        "git_missing" => conflict("git_missing"),
        "git_busy" => conflict("git_busy"),
        "git_timeout" => conflict("git_timeout"),
        "identity_missing" => conflict("identity_missing"),
        "nothing_to_commit" => conflict("nothing_to_commit"),
        "would_overwrite_changes" => conflict("would_overwrite_changes"),
        "credentials_needed" => conflict("credentials_needed"),
        "merge_conflict" => conflict("merge_conflict"),
        "branch_not_merged" => conflict("branch_not_merged"),
        "branch_exists" => conflict("branch_exists"),
        "hook_failed" => conflict("hook_failed"),
        "no_remote" => conflict("no_remote"),
        "nothing_to_continue" => conflict("nothing_to_continue"),
        "index_changed" => conflict("index_changed"),
        _ => None,
    }
}

impl GitService {
    pub fn new(projects: Arc<ProjectService>, presence: Arc<Presence>) -> Self {
        Self { projects, presence }
    }

    /// Watches the project's checkout (in `repository` inside it, when
    /// given) on its machine: the channel says `watching`, then `changed`
    /// after changes, or `unwatched` when it cannot. Dropping it stops.
    pub async fn watch(
        &self,
        project: Uuid,
        repository: Option<String>,
    ) -> Result<Channel, ServiceError> {
        let project = self.projects.get(project).await?;
        match self
            .presence
            .open_channel(
                project.machine_id,
                "git.watch",
                serde_json::json!({ "project": project.id, "repository": repository }),
                LOCAL,
            )
            .await?
        {
            Ok(channel) => Ok(channel),
            Err(error) => Err(git_error(&error.code, &error.message)
                .unwrap_or_else(|| shared_error(error, "git_failed"))),
        }
    }

    /// Runs one Git operation in the project's repository.
    pub async fn run<T: DeserializeOwned>(
        &self,
        project: Uuid,
        repository: Option<String>,
        op: GitOp,
    ) -> Result<T, ServiceError> {
        let project = self.projects.get(project).await?;
        let limit = if op.network() { NETWORK } else { LOCAL };
        let params = serde_json::to_value(GitRequest {
            project: project.id,
            repository,
            op,
        })
        .map_err(|error| ServiceError::Internal(error.to_string()))?;
        self.presence
            .ask(
                project.machine_id,
                "git.run",
                params,
                limit,
                git_error,
                "git_failed",
            )
            .await
    }
}
