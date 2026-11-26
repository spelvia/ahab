//! The layouts API's request and response types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::modules::projects::layout::model::Layout;

/// A project's saved windows: its tiles and tabs, as the client wrote them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ProjectLayout)]
pub struct LayoutDto {
    #[schema(value_type = Object)]
    pub layout: Value,
    /// Goes up with every save; send it back when saving.
    pub revision: i64,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<&Layout> for LayoutDto {
    fn from(layout: &Layout) -> Self {
        Self {
            layout: layout.layout.clone(),
            revision: layout.revision,
            updated_at: layout.updated_at,
        }
    }
}

/// A layout to save over `revision` (0 for the first save).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SaveLayout)]
pub struct SaveLayoutDto {
    #[schema(value_type = Object)]
    pub layout: Value,
    pub revision: i64,
}
