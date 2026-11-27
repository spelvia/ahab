//! Sending attachments to a project's machine and reading them back.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;
use uuid::Uuid;

use crate::modules::machines::Presence;
use crate::modules::projects::ProjectService;
use crate::modules::sessions::attachment::api::{
    AttachmentData, AttachmentDto, GetAttachment, PutAttachment,
};
use crate::modules::sessions::attachment::model::{self, MAX_BYTES};
use crate::shared::error::{FieldError, ServiceError};

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct AttachmentService {
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
}

/// The daemon's own attachment errors, as the API reports them.
fn attachment_error(code: &str, message: &str) -> Option<ServiceError> {
    let message = message.to_owned();
    match code {
        "attachment_not_found" => Some(ServiceError::NotFound {
            code: "attachment_not_found",
            message,
        }),
        "attachment_too_large" | "too_many_attachments" | "invalid_params" => Some(
            ServiceError::Invalid(vec![FieldError::new("attachments", message)]),
        ),
        _ => None,
    }
}

impl AttachmentService {
    pub fn new(projects: Arc<ProjectService>, presence: Arc<Presence>) -> Self {
        Self { projects, presence }
    }

    async fn ask(&self, project: Uuid, method: &str, params: Value) -> Result<Value, ServiceError> {
        let machine = self.projects.get(project).await?.machine_id;
        self.presence
            .ask(
                machine,
                method,
                params,
                TIMEOUT,
                attachment_error,
                "machine_request_failed",
            )
            .await
    }

    /// Keeps a file on the project's machine.
    pub async fn upload(
        &self,
        project: Uuid,
        name: &str,
        bytes: &[u8],
    ) -> Result<AttachmentDto, ServiceError> {
        if bytes.is_empty() {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "file",
                "The file is empty.",
            )]));
        }
        if bytes.len() > MAX_BYTES {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "file",
                "Attachments are at most 10 MiB.",
            )]));
        }
        let put = PutAttachment {
            name: model::safe_name(name),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        let answer = self
            .ask(
                project,
                "attachments.put",
                serde_json::to_value(put)
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
            )
            .await?;
        serde_json::from_value(answer)
            .map_err(|error| ServiceError::Internal(format!("unreadable attachment: {error}")))
    }

    /// A kept file's bytes and type.
    pub async fn download(
        &self,
        project: Uuid,
        id: Uuid,
        name: &str,
    ) -> Result<(Vec<u8>, &'static str), ServiceError> {
        let get = GetAttachment {
            id,
            name: name.to_owned(),
        };
        let answer = self
            .ask(
                project,
                "attachments.get",
                serde_json::to_value(get)
                    .map_err(|error| ServiceError::Internal(error.to_string()))?,
            )
            .await?;
        let data: AttachmentData = serde_json::from_value(answer)
            .map_err(|error| ServiceError::Internal(format!("unreadable attachment: {error}")))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.data)
            .map_err(|error| ServiceError::Internal(format!("unreadable attachment: {error}")))?;
        Ok((bytes, model::content_type(name)))
    }
}
