//! The core features API's types.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A core feature of Ahab (not a plugin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CoreFeature)]
pub struct FeatureDto {
    #[schema(example = "git")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Always on (Sessions).
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = CoreFeatureList)]
pub struct FeatureListDto {
    pub items: Vec<FeatureDto>,
}

/// Switching a feature on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = CoreFeatureChange)]
pub struct FeatureChangeDto {
    pub enabled: bool,
}
