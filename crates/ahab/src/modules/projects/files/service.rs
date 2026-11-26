//! Reading files through a machine's daemon, and the folder picker.

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::machines::{Channel, MachineService, Presence, shared_error};
use crate::modules::projects::files::api::{
    FileContentDto, FileEntryDto, FileListingDto, FolderListingDto, HomeDto, MovedFileDto,
    NewKindDto, SavedFileDto,
};
use crate::modules::projects::project::model::{Project, ProjectMode};
use crate::modules::projects::project::service::ProjectService;
use crate::shared::error::ServiceError;

const TIMEOUT: Duration = Duration::from_secs(30);

pub struct FileService {
    projects: Arc<ProjectService>,
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
}

impl FileService {
    pub fn new(
        projects: Arc<ProjectService>,
        machines: Arc<MachineService>,
        presence: Arc<Presence>,
    ) -> Self {
        Self {
            projects,
            machines,
            presence,
        }
    }

    /// The folder picker's start on `machine`: its user's home folder.
    pub async fn home(&self, machine: &str) -> Result<HomeDto, ServiceError> {
        let machine = self.machines.find(machine).await?;
        self.ask(machine.id, "fs.home", json!({})).await
    }

    /// The folder picker's listing on `machine`: subfolders only.
    pub async fn folders(
        &self,
        machine: &str,
        path: &str,
        hidden: bool,
    ) -> Result<FolderListingDto, ServiceError> {
        let machine = self.machines.find(machine).await?;
        self.ask(
            machine.id,
            "fs.listDirectories",
            json!({ "path": path, "hidden": hidden }),
        )
        .await
    }

    /// A folder's entries inside a project.
    pub async fn list(&self, project: Uuid, path: &str) -> Result<FileListingDto, ServiceError> {
        let project = self.projects.get(project).await?;
        self.ask(
            project.machine_id,
            "fs.list",
            json!({ "project": project.id, "path": path }),
        )
        .await
    }

    /// A file's content inside a project; with `partial`, a large file's
    /// first part instead of `file_too_large`.
    pub async fn read(
        &self,
        project: Uuid,
        path: &str,
        partial: bool,
    ) -> Result<FileContentDto, ServiceError> {
        let project = self.projects.get(project).await?;
        self.ask(
            project.machine_id,
            "fs.read",
            json!({ "project": project.id, "path": path, "partial": partial }),
        )
        .await
    }

    /// Bytes and media type for a browser image preview.
    pub async fn image(
        &self,
        project: Uuid,
        path: &str,
    ) -> Result<(Vec<u8>, &'static str), ServiceError> {
        use base64::Engine as _;
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mime = match extension.as_str() {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "avif" => "image/avif",
            "ico" => "image/x-icon",
            "bmp" => "image/bmp",
            _ => {
                return Err(ServiceError::Conflict {
                    code: "unsupported_image",
                    message: "Unsupported image type.".into(),
                });
            }
        };
        let project = self.projects.get(project).await?;
        let data: String = self
            .ask(
                project.machine_id,
                "fs.image",
                json!({ "project": project.id, "path": path }),
            )
            .await?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|error| ServiceError::Internal(error.to_string()))?;
        Ok((bytes, mime))
    }

    /// The version of a file now, if its machine can say: `None` when the
    /// file is gone, its machine is away, or its daemon is too old to give
    /// versions. For telling whether context someone selected went stale.
    pub async fn current_version(&self, project: Uuid, path: &str) -> Option<String> {
        self.read(project, path, false)
            .await
            .ok()
            .and_then(|file| file.version)
    }

    /// Saves a file's content: only over `expected` when given,
    /// else `file_changed`.
    pub async fn write(
        &self,
        project: Uuid,
        path: &str,
        content: &str,
        expected: Option<&str>,
    ) -> Result<SavedFileDto, ServiceError> {
        let project = self.writable(project).await?;
        self.ask(
            project.machine_id,
            "fs.write",
            json!({
                "project": project.id,
                "path": path,
                "content": content,
                "expectedVersion": expected,
            }),
        )
        .await
    }

    /// Makes a new file or folder, and its missing parent folders.
    pub async fn create(
        &self,
        project: Uuid,
        path: &str,
        kind: NewKindDto,
        content: Option<&str>,
    ) -> Result<FileEntryDto, ServiceError> {
        let project = self.writable(project).await?;
        self.ask(
            project.machine_id,
            "fs.create",
            json!({ "project": project.id, "path": path, "kind": kind, "content": content }),
        )
        .await
    }

    /// Moves a file or folder inside the project.
    pub async fn rename(
        &self,
        project: Uuid,
        from: &str,
        to: &str,
    ) -> Result<MovedFileDto, ServiceError> {
        let project = self.writable(project).await?;
        self.ask(
            project.machine_id,
            "fs.rename",
            json!({ "project": project.id, "from": from, "to": to }),
        )
        .await
    }

    /// Moves or copies a file or folder inside the project, over what is
    /// there only with `replace`. Daemons before `fs.move` cannot:
    /// `machine_outdated` (a plain move is a rename, which they can).
    pub async fn move_entry(
        &self,
        project: Uuid,
        from: &str,
        to: &str,
        copy: bool,
        replace: bool,
    ) -> Result<MovedFileDto, ServiceError> {
        let project = self.writable(project).await?;
        self.ask(
            project.machine_id,
            "fs.move",
            json!({
                "project": project.id,
                "from": from,
                "to": to,
                "copy": copy,
                "replace": replace,
            }),
        )
        .await
    }

    /// Deletes a file (at `expected`, when given) or a folder with
    /// everything in it.
    pub async fn delete(
        &self,
        project: Uuid,
        path: &str,
        expected: Option<&str>,
    ) -> Result<(), ServiceError> {
        let project = self.writable(project).await?;
        let _: serde_json::Value = self
            .ask(
                project.machine_id,
                "fs.delete",
                json!({ "project": project.id, "path": path, "expectedVersion": expected }),
            )
            .await?;
        Ok(())
    }

    /// Watches files of a project on its machine: the channel says
    /// `watching` (with each file's version), then `changed` per file.
    /// Dropping it stops.
    pub async fn watch(&self, project: Uuid, paths: Vec<String>) -> Result<Channel, ServiceError> {
        let project = self.projects.get(project).await?;
        match self
            .presence
            .open_channel(
                project.machine_id,
                "fs.watch",
                json!({ "project": project.id, "paths": paths }),
                TIMEOUT,
            )
            .await?
        {
            Ok(channel) => Ok(channel),
            Err(error) => Err(file_error(&error.code, &error.message)
                .unwrap_or_else(|| shared_error(error, "machine_request_failed"))),
        }
    }

    /// A project that may be written to: read-only ones refuse here,
    /// before asking the machine (which refuses too).
    async fn writable(&self, project: Uuid) -> Result<Project, ServiceError> {
        let project = self.projects.get(project).await?;
        if project.mode == ProjectMode::ReadOnly {
            return Err(ServiceError::Forbidden {
                code: "read_only",
                message: "This project is read-only.".to_owned(),
            });
        }
        Ok(project)
    }

    async fn ask<T: DeserializeOwned>(
        &self,
        machine: Uuid,
        method: &str,
        params: Value,
    ) -> Result<T, ServiceError> {
        self.presence
            .ask(
                machine,
                method,
                params,
                TIMEOUT,
                file_error,
                "machine_request_failed",
            )
            .await
    }
}

/// The daemon's own file errors, as the API reports them; the codes every
/// project request shares are mapped by `Presence::ask`.
fn file_error(code: &str, message: &str) -> Option<ServiceError> {
    let message = message.to_owned();
    Some(match code {
        "not_found" => ServiceError::NotFound {
            code: "not_found",
            message,
        },
        "file_too_large" => ServiceError::Conflict {
            code: "file_too_large",
            message,
        },
        "not_a_file" => ServiceError::Conflict {
            code: "not_a_file",
            message,
        },
        "not_a_folder" => ServiceError::Conflict {
            code: "not_a_folder",
            message,
        },
        "file_changed" => ServiceError::Conflict {
            code: "file_changed",
            message,
        },
        "already_exists" => ServiceError::Conflict {
            code: "already_exists",
            message,
        },
        _ => return None,
    })
}
