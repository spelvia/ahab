//! The notifications API's types.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::notifications::notification::model::{self, Level, Notification, Settings};

/// How important a notification is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NotificationLevel)]
pub enum LevelDto {
    Info,
    Success,
    Warning,
    Error,
}

impl From<Level> for LevelDto {
    fn from(level: Level) -> Self {
        match level {
            Level::Info => Self::Info,
            Level::Success => Self::Success,
            Level::Warning => Self::Warning,
            Level::Error => Self::Error,
        }
    }
}

/// Something that happened, for the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Notification)]
pub struct NotificationDto {
    pub id: Uuid,
    /// Such as `sessions.finished`.
    #[schema(example = "sessions.finished")]
    pub kind: String,
    /// `ahab`, or a plugin's ID.
    pub source: String,
    pub level: LevelDto,
    #[schema(example = "Claude Code finished")]
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// What opening it shows: `{"type": "session", "projectId",
    /// "sessionId", "chatId"}`; other types may come.
    #[schema(value_type = Object)]
    pub target: Value,
    /// Unread ones of a group stand for it: a newer one replaces the older
    /// (a phone shows one notification per group).
    #[serde(default)]
    pub group_key: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub read_at: Option<OffsetDateTime>,
}

impl From<Notification> for NotificationDto {
    fn from(notification: Notification) -> Self {
        Self {
            id: notification.id,
            kind: notification.kind,
            source: notification.source,
            level: notification.level.into(),
            title: notification.title,
            body: notification.body,
            project_id: notification.project_id,
            target: notification.target,
            group_key: notification.group_key,
            created_at: notification.created_at,
            read_at: notification.read_at,
        }
    }
}

/// The inbox, newest first, and how many are unread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = NotificationList)]
pub struct NotificationListDto {
    pub items: Vec<NotificationDto>,
    pub unread: u64,
}

/// How many notifications a change touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = NotificationCount)]
pub struct CountDto {
    pub count: u64,
}

/// A kind of notification, and whether it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NotificationKind)]
pub struct KindDto {
    #[schema(example = "sessions.finished")]
    pub id: String,
    pub title: String,
    pub description: String,
    pub enabled: bool,
    /// Shown outside the app too (the system's notifications, phones), while
    /// notifications go outside at all.
    pub outside: bool,
    /// `ahab`, or the plugin that sends it.
    #[schema(example = "ahab")]
    pub source: String,
    /// What Settings groups it under: Ahab, or the plugin's name.
    pub source_name: String,
}

/// The server's notification settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NotificationSettings)]
pub struct SettingsDto {
    /// Notifications at all.
    pub enabled: bool,
    /// Days read notifications are kept; none keeps them forever.
    pub keep_days: Option<u32>,
    pub kinds: Vec<KindDto>,
    /// Shown outside the app too: the browser's, the desktop's, the phone's.
    pub outside: bool,
}

impl From<Settings> for SettingsDto {
    fn from(settings: Settings) -> Self {
        Self::with(settings, Vec::new())
    }
}

/// A kind a plugin declared, for Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginKind {
    /// Its full kind, `<plugin>.<kind>`.
    pub id: String,
    pub title: String,
    pub description: String,
    pub plugin: String,
    pub plugin_name: String,
}

impl SettingsDto {
    /// The settings with Ahab's kinds, then those plugins declared.
    pub fn with(settings: Settings, plugin_kinds: Vec<PluginKind>) -> Self {
        let enabled = |id: &str| settings.kinds.get(id).copied().unwrap_or(true);
        let outside = |id: &str| !settings.inside_only.contains(id);
        let mut kinds: Vec<KindDto> = model::KINDS
            .iter()
            .map(|kind| KindDto {
                id: kind.id.to_owned(),
                title: kind.title.to_owned(),
                description: kind.description.to_owned(),
                enabled: enabled(kind.id),
                outside: outside(kind.id),
                source: "ahab".to_owned(),
                source_name: "Ahab".to_owned(),
            })
            .collect();
        kinds.extend(plugin_kinds.into_iter().map(|kind| KindDto {
            enabled: enabled(&kind.id),
            outside: outside(&kind.id),
            id: kind.id,
            title: kind.title,
            description: kind.description,
            source: kind.plugin,
            source_name: kind.plugin_name,
        }));
        Self {
            enabled: settings.enabled,
            keep_days: settings.keep_days,
            kinds,
            outside: settings.outside,
        }
    }
}

/// Changing the settings: all of them at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NotificationSettingsChange)]
pub struct SettingsChangeDto {
    pub enabled: bool,
    /// None keeps read notifications forever.
    #[serde(default)]
    pub keep_days: Option<u32>,
    /// Kinds by ID, on or off; kinds left out are on.
    #[serde(default)]
    pub kinds: BTreeMap<String, bool>,
    #[serde(default = "on")]
    pub outside: bool,
    /// Kinds shown only inside Ahab, never outside it.
    #[serde(default)]
    pub inside_only: BTreeSet<String>,
}

const fn on() -> bool {
    true
}

impl From<SettingsChangeDto> for Settings {
    fn from(change: SettingsChangeDto) -> Self {
        Self {
            enabled: change.enabled,
            keep_days: change.keep_days,
            kinds: change.kinds,
            outside: change.outside,
            inside_only: change.inside_only,
        }
    }
}

/// The agents working now: a phone's background connection stays while
/// there are some.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NotificationActivity)]
pub struct ActivityDto {
    pub working: u64,
}
