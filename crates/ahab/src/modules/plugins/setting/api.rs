//! The settings API's types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Where settings apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginSettingScope)]
pub enum SettingScopeDto {
    Server,
    Machine,
    Project,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ScopeQuery {
    #[param(inline)]
    pub scope: SettingScopeDto,
    /// The machine's or project's ID; none for the server.
    pub id: Option<Uuid>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EffectiveQuery {
    pub machine: Option<Uuid>,
    /// A project: its machine's values and its own apply.
    pub project: Option<Uuid>,
}

/// A plugin's settings at a scope, with the schema that describes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginSettings)]
pub struct PluginSettingsDto {
    /// The settings' JSON Schema; absent when the plugin has none.
    pub schema: Option<Value>,
    /// The values set at this scope.
    pub values: Value,
}

/// New values at a scope (they replace the ones there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginSettingsChange)]
pub struct PluginSettingsChangeDto {
    pub values: Value,
}

/// The values in effect: defaults, then each scope over the wider ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginEffectiveSettings)]
pub struct EffectiveSettingsDto {
    pub values: Value,
}
