//! Plugins and notifications: plugins send the kinds
//! their manifests declare, change or drop notifications through the
//! `notifications.send` hook, hear `notification.created`, and deliver
//! notifications to their destinations (ntfy, Slack).

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::modules::machines::Presence;
use crate::modules::notifications::delivery::model::{self, HOOK, PluginSend};
use crate::modules::notifications::notification::api::PluginKind;
use crate::modules::notifications::notification::model::{NewNotification, Notification};
use crate::modules::notifications::notification::service::{
    Intercept, NotificationEvent, NotificationService,
};
use crate::modules::plugins::{Plugin, PluginHostService, PluginService, SettingService};
use crate::modules::projects::ProjectService;
use crate::shared::error::ServiceError;

/// How long the server waits for a machine's plugins to answer the hook:
/// each plugin has 5 seconds there.
const HOOK_WAIT: Duration = Duration::from_secs(20);
/// How long a destination may take, with the trip to its machine.
const DELIVER_WAIT: Duration = Duration::from_secs(35);

pub struct PluginNotifications {
    notifications: Arc<NotificationService>,
    plugins: Arc<PluginService>,
    host: Arc<PluginHostService>,
    settings: Arc<SettingService>,
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
    /// The address people use, for destinations' links.
    public_url: RwLock<Option<String>>,
}

fn passed_over(_code: &str, _message: &str) -> Option<ServiceError> {
    None
}

impl PluginNotifications {
    pub fn new(
        notifications: Arc<NotificationService>,
        plugins: Arc<PluginService>,
        host: Arc<PluginHostService>,
        settings: Arc<SettingService>,
        projects: Arc<ProjectService>,
        presence: Arc<Presence>,
    ) -> Self {
        Self {
            notifications,
            plugins,
            host,
            settings,
            projects,
            presence,
            public_url: RwLock::new(None),
        }
    }

    pub fn use_public_url(&self, url: Option<String>) {
        *self
            .public_url
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = url;
    }

    fn public_url(&self) -> Option<String> {
        self.public_url
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn enabled(&self) -> Result<Vec<Plugin>, ServiceError> {
        Ok(self
            .plugins
            .list()
            .await?
            .into_iter()
            .filter(Plugin::enabled)
            .collect())
    }

    /// The kinds enabled plugins declared, for Settings.
    pub async fn kinds(&self) -> Result<Vec<PluginKind>, ServiceError> {
        Ok(self
            .enabled()
            .await?
            .into_iter()
            .flat_map(|plugin| {
                plugin
                    .manifest
                    .notifications
                    .iter()
                    .map(|declared| PluginKind {
                        id: format!("{}.{}", plugin.id, declared.kind),
                        title: declared.title.clone(),
                        description: declared.description.clone().unwrap_or_default(),
                        plugin: plugin.id.clone(),
                        plugin_name: plugin.name.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect())
    }

    /// Sends what `plugin` asked to, if it is enabled and declared the kind;
    /// `None` when it was not kept (turned off, or a hook dropped it).
    /// A machine may send only for a plugin running on it.
    pub async fn send(
        &self,
        plugin: &str,
        from: Option<Uuid>,
        sent: PluginSend,
    ) -> Result<Option<Notification>, ServiceError> {
        let found = self.plugins.get(plugin).await?;
        if !found.enabled() {
            return Err(ServiceError::Conflict {
                code: "plugin_disabled",
                message: format!("{} is disabled; enable it first.", found.name),
            });
        }
        if let Some(machine) = from
            && !self.host.running_on(plugin).contains(&machine)
        {
            return Err(ServiceError::Forbidden {
                code: "plugin_not_here",
                message: format!("{} does not run on this machine.", found.name),
            });
        }
        let declared: Vec<String> = found
            .manifest
            .notifications
            .iter()
            .map(|declared| declared.kind.clone())
            .collect();
        let new = sent
            .check(plugin, &declared)
            .map_err(ServiceError::Invalid)?;
        if let Some(project) = new.project_id {
            self.projects.get(project).await?;
        }
        self.notifications.send(new).await
    }

    /// One machine for each plugin, where it runs.
    fn placed(&self, plugins: Vec<Plugin>) -> BTreeMap<Uuid, Vec<String>> {
        let mut placed: BTreeMap<Uuid, Vec<String>> = BTreeMap::new();
        for plugin in plugins {
            if let Some(machine) = self.host.running_on(&plugin.id).first() {
                placed.entry(*machine).or_default().push(plugin.id);
            }
        }
        placed
    }

    /// Asks the plugins that declared the hook, on their machines: each may
    /// change the notification or drop it.
    async fn intercept(&self, mut new: NewNotification) -> Option<NewNotification> {
        let hooking: Vec<Plugin> = match self.enabled().await {
            Ok(plugins) => plugins
                .into_iter()
                .filter(|plugin| plugin.manifest.hooks.iter().any(|hook| hook == HOOK))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "cannot read plugins for the notifications hook");
                return Some(new);
            }
        };
        if hooking.is_empty() {
            return Some(new);
        }
        for (machine, plugins) in self.placed(hooking) {
            let answer: Result<Value, ServiceError> = self
                .presence
                .ask(
                    machine,
                    "plugins.intercept",
                    json!({ "hook": HOOK, "plugins": plugins, "data": model::hooked(&new) }),
                    HOOK_WAIT,
                    passed_over,
                    "hook_failed",
                )
                .await;
            match answer {
                Ok(answer) if answer["action"] == "deny" => {
                    tracing::info!(
                        kind = %new.kind,
                        plugin = %answer["plugin"].as_str().unwrap_or_default(),
                        reason = %answer["reason"].as_str().unwrap_or_default(),
                        "a plugin dropped a notification"
                    );
                    return None;
                }
                Ok(answer) => {
                    if answer["changedBy"]
                        .as_array()
                        .is_some_and(|by| !by.is_empty())
                    {
                        new = model::changed(new, &answer["data"]);
                    }
                }
                Err(error) => {
                    tracing::info!(%machine, %error, "plugins passed over for the notifications hook");
                }
            }
        }
        Some(new)
    }

    /// Hands a new notification to listeners (`notification.created`) and,
    /// while it goes outside the app (its kind too), to every plugin destination.
    pub async fn deliver(&self, notification: &Notification) -> Result<(), ServiceError> {
        let outgoing = model::outgoing(notification, self.public_url().as_deref());
        self.presence
            .bus()
            .publish("notification.created", outgoing.clone());
        if !self
            .notifications
            .settings()
            .await?
            .goes_outside(&notification.kind)
        {
            return Ok(());
        }
        for plugin in self.enabled().await? {
            if plugin.manifest.destinations.is_empty() {
                continue;
            }
            let Some(machine) = self.host.running_on(&plugin.id).first().copied() else {
                tracing::info!(plugin = %plugin.id, "a destination's plugin is not running");
                continue;
            };
            let settings = self
                .settings
                .effective(&plugin.id, Some(machine), None)
                .await?;
            for destination in &plugin.manifest.destinations {
                let answer: Result<Value, ServiceError> = self
                    .presence
                    .ask(
                        machine,
                        "plugins.notify",
                        json!({
                            "plugin": plugin.id,
                            "destination": destination.id,
                            "notification": outgoing,
                            "settings": settings,
                        }),
                        DELIVER_WAIT,
                        passed_over,
                        "delivery_failed",
                    )
                    .await;
                if let Err(error) = answer {
                    tracing::warn!(
                        plugin = %plugin.id,
                        destination = %destination.id,
                        %error,
                        "a destination did not take a notification"
                    );
                }
            }
        }
        Ok(())
    }

    /// Delivers every new notification, for as long as the server runs.
    pub fn spawn(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let mut events = self.notifications.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if let NotificationEvent::Created(notification) = event.as_ref() {
                            let service = Arc::clone(&service);
                            let notification = notification.clone();
                            tokio::spawn(async move {
                                if let Err(error) = service.deliver(&notification).await {
                                    tracing::warn!(%error, "cannot deliver a notification to plugins");
                                }
                            });
                        }
                    }
                    Err(RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "plugin destinations fell behind");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

#[async_trait::async_trait]
impl Intercept for PluginNotifications {
    async fn before_send(&self, new: NewNotification) -> Option<NewNotification> {
        self.intercept(new).await
    }
}
