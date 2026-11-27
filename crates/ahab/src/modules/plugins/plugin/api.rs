//! The plugins API's request and response types. The server returns them
//! and the command line reads them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::plugins::plugin::model::{Placement, Plugin, Source};

/// Where a plugin was installed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginSource)]
pub enum PluginSourceDto {
    /// A folder on the server's host.
    Folder,
    /// A Git repository, cloned (and built when needed) by the server.
    Git,
    /// A folder being worked on, followed as it changes.
    Dev,
}

impl From<Source> for PluginSourceDto {
    fn from(source: Source) -> Self {
        match source {
            Source::Folder => Self::Folder,
            Source::Git => Self::Git,
            Source::Dev => Self::Dev,
        }
    }
}

/// Where a plugin's daemon entry runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginPlacement)]
pub enum PluginPlacementDto {
    /// Every machine.
    All,
    /// The machines in `machines`.
    Chosen,
}

impl From<Placement> for PluginPlacementDto {
    fn from(placement: Placement) -> Self {
        match placement {
            Placement::All => Self::All,
            Placement::Chosen => Self::Chosen,
        }
    }
}

/// An installed plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Plugin)]
pub struct PluginDto {
    #[schema(example = "docker")]
    pub id: String,
    #[schema(example = "Docker")]
    pub name: String,
    #[schema(example = "0.1.0")]
    pub version: String,
    pub description: Option<String>,
    pub source: PluginSourceDto,
    /// The folder or Git URL it came from.
    pub location: String,
    /// Which build is installed (SHA-256 of its files).
    pub digest: String,
    pub enabled: bool,
    /// When it was enabled: also the order its interceptors run in, with
    /// `position`.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub enabled_at: Option<OffsetDateTime>,
    pub position: i32,
    /// Its entries: `web`, `server`, `daemon`.
    pub entries: Vec<String>,
    /// The hooks it intercepts.
    pub hooks: Vec<String>,
    /// The secrets it asks for.
    pub secrets: Vec<String>,
    /// Other plugins it uses, with version ranges.
    pub depends_on: std::collections::BTreeMap<String, String>,
    pub placement: PluginPlacementDto,
    /// Where it runs when `placement` is `chosen`.
    pub machines: Vec<Uuid>,
    /// The manifest as installed.
    pub manifest: Value,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<&Plugin> for PluginDto {
    fn from(plugin: &Plugin) -> Self {
        Self {
            id: plugin.id.clone(),
            name: plugin.name.clone(),
            version: plugin.version.clone(),
            description: plugin.description.clone(),
            source: plugin.source.into(),
            location: plugin.location.clone(),
            digest: plugin.digest.clone(),
            enabled: plugin.enabled(),
            enabled_at: plugin.enabled_at,
            position: plugin.position,
            entries: plugin
                .manifest
                .entries
                .all()
                .map(|(name, _)| name.to_owned())
                .collect(),
            hooks: plugin.manifest.hooks.clone(),
            secrets: plugin.manifest.secrets.clone(),
            depends_on: plugin.manifest.depends_on.clone(),
            placement: plugin.manifest.machines.into(),
            machines: plugin.machines.clone(),
            manifest: serde_json::to_value(&plugin.manifest).unwrap_or(Value::Null),
            created_at: plugin.created_at,
            updated_at: plugin.updated_at,
        }
    }
}

/// The installed plugins, in the order their interceptors run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = PluginList)]
pub struct PluginListDto {
    pub items: Vec<PluginDto>,
}

/// What to install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = InstallPlugin)]
pub struct InstallPluginDto {
    pub source: PluginSourceDto,
    /// A folder on the server's host, or a Git URL.
    #[schema(example = "https://github.com/you/ahab-docker")]
    pub location: String,
}

/// A plugin as installed, and what its manifest should fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = InstalledPlugin)]
pub struct InstalledPluginDto {
    pub plugin: PluginDto,
    pub warnings: Vec<String>,
}

/// Turning a plugin on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = EnablePlugin)]
pub struct EnablePluginDto {
    /// Where it runs, for a plugin whose placement is `chosen`.
    #[serde(default)]
    pub machines: Option<Vec<Uuid>>,
}

/// Changing a plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginChange)]
pub struct PluginChangeDto {
    /// Where it runs, for a plugin whose placement is `chosen`.
    #[serde(default)]
    pub machines: Option<Vec<Uuid>>,
    /// Its place among interceptors (lower runs first).
    #[serde(default)]
    pub position: Option<i32>,
}
