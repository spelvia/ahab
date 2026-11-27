//! The types of plugins' notifications API.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

/// What a plugin sends (`ahab.notifications.send`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginNotification)]
pub struct PluginSendDto {
    /// A kind the plugin's manifest declares, without its ID.
    #[schema(example = "pipeline.failed")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// `info` (the default), `success`, `warning` or `error`.
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub project: Option<Uuid>,
    /// `{"type": "route", "path"}`, `{"type": "url", "url"}` or
    /// `{"type": "window", "window", "props"}`.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub target: Option<Value>,
    /// Unread ones of the same group are replaced.
    #[serde(default)]
    pub group_key: Option<String>,
}

/// What came of it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginNotificationSent)]
pub struct SentDto {
    /// The notification's ID; none when it was not kept (its kind turned
    /// off, notifications off, or a hook dropped it).
    pub id: Option<Uuid>,
}
