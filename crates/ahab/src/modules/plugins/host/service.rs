//! The server's side of running plugins: telling each machine which
//! plugins to run (when it connects and whenever plugins change), sending
//! it the builds it lacks, and keeping what machines report.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::machines::{MachineService, Presence};
use crate::modules::plugins::host::model::{
    Missing, PluginRef, RunState, StateReport, SyncAnswer, SyncRequest, Wanted, encode_file,
};
use crate::modules::plugins::plugin::model::{Entry, Plugin};
use crate::modules::plugins::plugin::service::PluginService;
use crate::modules::plugins::setting::service::SettingService;
use crate::shared::error::ServiceError;
use crate::shared::events::Event;
use crate::shared::sync::lock;
use crate::shared::time::Clock;

/// How long a machine has to answer a plugin request.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A plugin's state on a machine, as last reported.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineState {
    pub machine: Uuid,
    pub report: StateReport,
    pub since: OffsetDateTime,
}

pub struct PluginHostService {
    plugins: Arc<PluginService>,
    settings: Arc<SettingService>,
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
    states: Mutex<HashMap<(String, Uuid), MachineState>>,
    /// One sync at a time per machine.
    syncing: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
}

/// The files under `dir`, with paths relative to it (`/` between parts).
fn files_under(dir: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                let path = entry.path();
                let relative = path
                    .strip_prefix(dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, std::fs::read(&path)?));
            }
        }
    }
    Ok(files)
}

impl PluginHostService {
    pub fn new(
        plugins: Arc<PluginService>,
        settings: Arc<SettingService>,
        machines: Arc<MachineService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            plugins,
            settings,
            machines,
            presence,
            clock,
            states: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashMap::new()),
        }
    }

    /// For as long as the server runs: syncs machines as they connect and
    /// as plugins change, and keeps the states they report.
    pub fn spawn_background(self: &Arc<Self>) {
        let mut events = self.presence.take_events_starting("plugin.");
        let service = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event.kind.as_str() {
                    "plugin.state" => {
                        if let Ok(report) = serde_json::from_value::<StateReport>(event.data) {
                            service.keep(event.machine, report);
                        }
                    }
                    "plugin.event" => {
                        let (plugin, name) = (&event.data["plugin"], &event.data["name"]);
                        if let (Some(plugin), Some(name)) = (plugin.as_str(), name.as_str()) {
                            service
                                .presence
                                .bus()
                                .publish(format!("{plugin}.{name}"), event.data["data"].clone());
                        }
                        if event.seq > 0 {
                            service
                                .presence
                                .ack(event.machine, event.generation, event.seq);
                        }
                    }
                    _ => {}
                }
            }
        });
        let service = Arc::clone(self);
        let mut connected = self.presence.subscribe();
        tokio::spawn(async move {
            loop {
                match connected.recv().await {
                    Ok(machine) => service.sync_later(machine),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        // Every event, to the machines whose plugins may listen to it.
        let service = Arc::clone(self);
        let mut heard = self.presence.bus().subscribe();
        tokio::spawn(async move {
            loop {
                match heard.recv().await {
                    Ok(event) => service.deliver(&event),
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "plugins missed events");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        let service = Arc::clone(self);
        let mut changed = self.plugins.subscribe();
        tokio::spawn(async move {
            loop {
                match changed.recv().await {
                    Ok(plugin) => {
                        // Browsers load, reload or unload its web entry.
                        service
                            .presence
                            .bus()
                            .publish("plugins.changed", json!({ "plugin": plugin }));
                        for machine in service.presence.online() {
                            service.sync_later(machine);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        for machine in service.presence.online() {
                            service.sync_later(machine);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    fn keep(&self, machine: Uuid, report: StateReport) {
        let key = (report.plugin.clone(), machine);
        let mut states = lock(&self.states);
        // A stopped plugin is no longer the machine's.
        if report.state == RunState::Stopped {
            states.remove(&key);
            return;
        }
        let since = match states.get(&key) {
            Some(kept) if kept.report.state == report.state => kept.since,
            _ => self.clock.now(),
        };
        states.insert(
            key,
            MachineState {
                machine,
                report,
                since,
            },
        );
    }

    fn sync_later(self: &Arc<Self>, machine: Uuid) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = service.sync(machine).await {
                tracing::info!(%machine, %error, "cannot sync the machine's plugins");
            }
        });
    }

    /// Sends an event to every machine with a running plugin that listens to
    /// it (as it declared when it started).
    fn deliver(&self, event: &Event) {
        let machines: std::collections::HashSet<Uuid> = lock(&self.states)
            .values()
            .filter(|state| {
                state.report.state == RunState::Running
                    && state
                        .report
                        .declared
                        .as_ref()
                        .and_then(|declared| declared["events"].as_array())
                        .is_some_and(|names| names.iter().any(|name| name == event.name.as_str()))
            })
            .map(|state| state.machine)
            .collect();
        for machine in machines {
            let presence = Arc::clone(&self.presence);
            let params = json!({ "name": event.name, "data": event.data });
            tokio::spawn(async move {
                let _: Result<serde_json::Value, _> = presence
                    .ask(
                        machine,
                        "plugins.deliver",
                        params,
                        TIMEOUT,
                        |_, _| None,
                        "plugin_event_failed",
                    )
                    .await;
            });
        }
    }

    /// Tells `machine` which plugins to run, and sends the builds it lacks.
    pub async fn sync(&self, machine: Uuid) -> Result<(), ServiceError> {
        let guard = Arc::clone(lock(&self.syncing).entry(machine).or_default());
        let _held = guard.lock().await;
        let name = self.machines.get(machine).await?.name.to_string();
        let plugins: Vec<Plugin> = self
            .plugins
            .list()
            .await?
            .into_iter()
            .filter(|plugin| plugin.runs_on(machine))
            .collect();
        let mut wanted = Vec::with_capacity(plugins.len());
        for plugin in &plugins {
            wanted.push(Wanted {
                id: plugin.id.clone(),
                digest: plugin.digest.clone(),
                entry: match &plugin.manifest.entries.daemon {
                    Some(Entry::File(path)) => Some(path.clone()),
                    _ => None,
                },
                settings: self
                    .settings
                    .effective(&plugin.id, Some(machine), None)
                    .await?,
                hooks: Some(plugin.manifest.hooks.clone()),
            });
        }
        let request = SyncRequest {
            machine: name,
            plugins: wanted,
        };
        let params = serde_json::to_value(&request)
            .map_err(|error| ServiceError::Internal(error.to_string()))?;
        let answer: SyncAnswer = self
            .presence
            .ask(
                machine,
                "plugins.sync",
                params,
                TIMEOUT,
                |_, _| None,
                "plugin_sync_failed",
            )
            .await?;
        {
            let mut states = lock(&self.states);
            states.retain(|(plugin, on), _| {
                *on != machine || plugins.iter().any(|wanted| &wanted.id == plugin)
            });
        }
        for report in answer.states {
            self.keep(machine, report);
        }
        for missing in answer.missing {
            if let Err(error) = self.send(machine, &plugins, &missing).await {
                tracing::warn!(%machine, plugin = %missing.id, %error, "cannot send a plugin to the machine");
            }
        }
        Ok(())
    }

    /// Sends one build to `machine`, a file per channel frame.
    async fn send(
        &self,
        machine: Uuid,
        plugins: &[Plugin],
        missing: &Missing,
    ) -> Result<(), ServiceError> {
        let Some(plugin) = plugins
            .iter()
            .find(|plugin| plugin.id == missing.id && plugin.digest == missing.digest)
        else {
            return Ok(());
        };
        let dir = self.plugins.files_dir(plugin);
        let files = tokio::task::spawn_blocking(move || files_under(&dir))
            .await
            .map_err(|error| ServiceError::Internal(error.to_string()))?
            .map_err(|error| {
                ServiceError::Internal(format!("reading the plugin's files: {error}"))
            })?;
        let channel = match self
            .presence
            .open_channel(
                machine,
                "plugins.receive",
                json!({ "id": missing.id, "digest": missing.digest }),
                TIMEOUT,
            )
            .await?
        {
            Ok(channel) => channel,
            Err(error) => return Err(ServiceError::Internal(error.message)),
        };
        for (path, bytes) in &files {
            if !channel.send(&encode_file(path, bytes)).await {
                return Err(ServiceError::Internal("the machine went away".to_owned()));
            }
        }
        // Dropping the channel closes it: the machine then has every file.
        drop(channel);
        Ok(())
    }

    /// The states machines reported, for one plugin or all; machines that
    /// are offline now are left out.
    /// The machines where `plugin`'s daemon entry runs now.
    pub fn running_on(&self, plugin: &str) -> Vec<Uuid> {
        self.states(Some(plugin))
            .into_iter()
            .filter(|state| state.report.state == RunState::Running)
            .map(|state| state.machine)
            .collect()
    }

    pub fn states(&self, plugin: Option<&str>) -> Vec<MachineState> {
        let mut states: Vec<MachineState> = lock(&self.states)
            .iter()
            .filter(|((id, machine), _)| {
                plugin.is_none_or(|plugin| plugin == id) && self.presence.is_online(*machine)
            })
            .map(|(_, state)| state.clone())
            .collect();
        states.sort_by(|a, b| {
            (a.report.plugin.as_str(), a.machine).cmp(&(b.report.plugin.as_str(), b.machine))
        });
        states
    }

    /// A plugin's logs on `machine`, oldest first.
    pub async fn logs(&self, plugin: &str, machine: Uuid) -> Result<Vec<String>, ServiceError> {
        self.plugins.get(plugin).await?;
        let params = serde_json::to_value(PluginRef {
            plugin: plugin.to_owned(),
        })
        .map_err(|error| ServiceError::Internal(error.to_string()))?;
        self.presence
            .ask(
                machine,
                "plugins.logs",
                params,
                TIMEOUT,
                |_, _| None,
                "plugin_logs_failed",
            )
            .await
    }
}
