use std::sync::Arc;

use crate::modules::clients::{ClientService, PairingService};
use crate::modules::machines::{
    AgentService, EnrollmentService, MachineService, MetricsService, Presence, UsageService,
};
use crate::modules::notifications::{NotificationService, PluginNotifications, PushService};
use crate::modules::platform::{CoreSettingService, FeatureService};
use crate::modules::plugins::{CommandService, PluginHostService, PluginService, SettingService};
use crate::modules::projects::{
    FileService, GitService, LayoutService, ProjectService, SearchService,
};
use crate::modules::sessions::{AttachmentService, ChatService, SessionService, TerminalService};
use crate::modules::tools::{
    LanguageServerService, ProblemService, RepositoryActivityService, ToolService,
    ToolSettingService,
};

/// The services the routes use. Built once in [`super::start`].
#[derive(Clone)]
pub struct AppState {
    /// Notifications.
    pub notifications: Arc<NotificationService>,
    pub push: Arc<PushService>,
    pub plugin_notifications: Arc<PluginNotifications>,
    /// The names the server answers to.
    pub allowed_hosts: Arc<crate::server::hosts::AllowedHosts>,
    /// The other guards' settings: proxies, origins, connections.
    pub access: Arc<crate::server::access::Access>,
    pub clients: Arc<ClientService>,
    pub pairings: Arc<PairingService>,
    pub machines: Arc<MachineService>,
    pub presence: Arc<Presence>,
    pub enrollment: Arc<EnrollmentService>,
    pub agents: Arc<AgentService>,
    pub usage: Arc<UsageService>,
    pub metrics: Arc<MetricsService>,
    pub projects: Arc<ProjectService>,
    pub files: Arc<FileService>,
    pub search: Arc<SearchService>,
    pub git: Arc<GitService>,
    pub layouts: Arc<LayoutService>,
    pub sessions: Arc<SessionService>,
    pub chats: Arc<ChatService>,
    pub terminals: Arc<TerminalService>,
    pub attachments: Arc<AttachmentService>,
    pub plugins: Arc<PluginService>,
    pub plugin_host: Arc<PluginHostService>,
    pub commands: Arc<CommandService>,
    pub settings: Arc<SettingService>,
    pub features: Arc<FeatureService>,
    /// Ahab's own settings (the editor's); plugins' are `settings`.
    pub core_settings: Arc<CoreSettingService>,
    /// Language tooling: settings, the registry, language
    /// servers and Problems.
    pub tool_settings: Arc<ToolSettingService>,
    pub tools: Arc<ToolService>,
    pub language_servers: Arc<LanguageServerService>,
    pub problems: Arc<ProblemService>,
    /// Active and pinned repositories per project.
    pub repository_activity: Arc<RepositoryActivityService>,
}
