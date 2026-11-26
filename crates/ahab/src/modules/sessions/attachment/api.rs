//! The attachments API's types, and the daemon messages that carry files.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// A stored attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Attachment)]
pub struct AttachmentDto {
    pub id: Uuid,
    #[schema(example = "screenshot.png")]
    pub name: String,
    /// Where it is on the project's machine.
    pub path: String,
    #[schema(example = "image/png")]
    pub content_type: String,
    /// In bytes.
    pub size: u64,
}

/// `attachments.put`: a file to keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PutAttachment {
    pub name: String,
    /// The bytes, base64-encoded.
    pub data: String,
}

/// `attachments.get`: a kept file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAttachment {
    pub id: Uuid,
    pub name: String,
}

/// The daemon's answer to `attachments.get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentData {
    /// The bytes, base64-encoded.
    pub data: String,
}
