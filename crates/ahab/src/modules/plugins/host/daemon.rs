//! The daemon's plugin host: the builds this machine has, one Node process
//! per daemon entry it should run, speaking the plugin protocol on its
//! pipes, restarted when it crashes, with its logs kept in memory.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::format_description;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::modules::plugins::host::model::{
    Decoder, Frame, Missing, RunState, StateReport, SyncAnswer, SyncRequest, Wanted,
    collection_source, confined, encode, lsp_diagnostic, valid_collection,
};
use crate::modules::plugins::plugin::model::{PLUGIN_API, digest_of, inside};
use crate::shared::sync::lock;
use crate::shared::time::display;

/// The oldest Node that runs plugins.
const NODE_MAJOR: u32 = 24;
/// `initialize` must answer within this.
const INITIALIZE: Duration = Duration::from_secs(10);
/// A stopping entry gets this long to answer `shutdown`, then this long to
/// exit, before it is killed.
const GRACE: Duration = Duration::from_secs(5);
/// This many crashes within [`CRASH_WINDOW`] mark a plugin failed.
const CRASHES: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(120);
/// The longest wait before starting a crashed entry again.
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// What a plugin sends on a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelOut {
    Data(Vec<u8>),
    /// It closed the channel, with a reason for people when it gave one.
    End(Option<String>),
}

/// An interceptor must answer within this, or it is passed over.
const HOOK_LIMIT: Duration = Duration::from_secs(5);
/// How long a notification destination may take to deliver.
const DELIVER_LIMIT: Duration = Duration::from_secs(30);
/// Frames a channel holds for a slow reader before it ends.
const CHANNEL_FRAMES: usize = 512;
/// A command stops after this long.
pub const COMMAND_LIMIT: Duration = Duration::from_secs(600);
/// Lines of logs kept per plugin.
const LOG_LINES: usize = 1000;

/// Sends the server an event: its kind, data, and whether it is kept while
/// the server is away (a plugin's lasting events) or live only.
pub type Emit = Arc<dyn Fn(&str, Value, bool) + Send + Sync>;

/// A plugin's diagnostics for one project (plugin API 0.1.3): its
/// collection's problems by file (relative to the project, LSP's form),
/// replacing what the collection said of those files, or of every file
/// with `replace_all`.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginDiagnostics {
    pub project: uuid::Uuid,
    /// `<plugin>.<collection>`.
    pub source: String,
    pub files: std::collections::BTreeMap<String, Vec<Value>>,
    pub replace_all: bool,
}

/// Where plugins' diagnostics go, and how projects' folders are found:
/// given by the daemon, which knows both.
pub struct DiagnosticsWiring {
    /// A project's real folder on this machine, when it serves it.
    pub folder_of: Arc<dyn Fn(uuid::Uuid) -> Option<PathBuf> + Send + Sync>,
    pub out: Arc<dyn Fn(PluginDiagnostics) + Send + Sync>,
}

/// How long a plugin's check (`onCheck`) may take.
const CHECK_LIMIT: Duration = Duration::from_mins(15);

/// An error from a plugin's process: a code, and a message for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailure {
    pub code: String,
    pub message: String,
}

impl PluginFailure {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value, PluginFailure>>>>;

/// A running entry's pipes: requests to it, and their answers.
pub struct Process {
    writer: mpsc::Sender<Vec<u8>>,
    pending: Arc<Pending>,
    next: AtomicU64,
}

impl Process {
    fn send(&self, message: &Value) -> bool {
        let Ok(body) = serde_json::to_vec(message) else {
            return false;
        };
        self.writer
            .try_send(encode(&Frame {
                channel: None,
                body,
            }))
            .is_ok()
    }

    /// Sends a request and waits for its answer, for at most `limit`.
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        limit: Duration,
    ) -> Result<Value, PluginFailure> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = oneshot::channel();
        lock(&self.pending).insert(id, answer);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if !self.send(&message) {
            lock(&self.pending).remove(&id);
            return Err(PluginFailure::new(
                "plugin_not_running",
                "The plugin stopped.",
            ));
        }
        // Dropped before the answer (the caller went away): the plugin is
        // told to stop working on it.
        let mut waiting = Waiting {
            id: Some(id),
            process: self,
        };
        let answer = tokio::time::timeout(limit, answered).await;
        waiting.id = None;
        match answer {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(PluginFailure::new(
                "plugin_not_running",
                "The plugin stopped.",
            )),
            Err(_) => {
                lock(&self.pending).remove(&id);
                self.notify("$/cancelRequest", json!({ "id": id }));
                Err(PluginFailure::new(
                    "plugin_timeout",
                    format!("The plugin did not answer {method} in time."),
                ))
            }
        }
    }

    /// Answers a request the plugin sent.
    fn answer(&self, id: Value, answered: Result<Value, PluginFailure>) {
        let message = match answered {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(failure) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32000,
                    "message": failure.message,
                    "data": { "code": failure.code },
                },
            }),
        };
        self.send(&message);
    }

    /// Sends a notification.
    pub fn notify(&self, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
}

/// A request to a plugin that has no answer yet.
struct Waiting<'a> {
    id: Option<u64>,
    process: &'a Process,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            lock(&self.process.pending).remove(&id);
            self.process.notify("$/cancelRequest", json!({ "id": id }));
        }
    }
}

/// One entry being run: its build, and how to stop it.
struct Supervised {
    digest: String,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
    process: Arc<Mutex<Option<Arc<Process>>>>,
}

#[derive(Default)]
struct Inner {
    machine: String,
    /// The plugins in the order their interceptors run.
    order: Vec<String>,
    /// The hooks each running plugin declared.
    hooks: HashMap<String, Vec<String>>,
    wanted: HashMap<String, Wanted>,
    running: HashMap<String, Supervised>,
    /// Builds that crashed too often: not started again until they change
    /// or are enabled again.
    failed: HashMap<String, String>,
    states: HashMap<String, StateReport>,
}

/// The machine's plugins.
pub struct PluginHost {
    /// `<data>/plugins/run`: `<id>/<digest>/` per build.
    root: PathBuf,
    /// The server's address: plugins' calls to Ahab go to its API.
    server: String,
    http: reqwest::Client,
    /// The token the server gave this connection, for the API.
    api_token: Mutex<Option<String>>,
    node: Option<PathBuf>,
    emit: Emit,
    /// Open channels: the plugin each is with, and where its output goes.
    channels: Mutex<HashMap<uuid::Uuid, (String, mpsc::Sender<ChannelOut>)>>,
    /// The events each plugin listens to.
    subscribed: Mutex<HashMap<String, std::collections::HashSet<String>>>,
    inner: Mutex<Inner>,
    logs: Mutex<HashMap<String, VecDeque<String>>>,
    found_node: tokio::sync::OnceCell<Result<PathBuf, String>>,
    diagnostics: std::sync::OnceLock<DiagnosticsWiring>,
    /// The projects and sources each plugin's collections hold problems in.
    collections: Mutex<HashMap<String, std::collections::BTreeSet<(uuid::Uuid, String)>>>,
    /// The plugins that check on demand (`onCheck`).
    checkers: Mutex<std::collections::BTreeSet<String>>,
}

/// The major version in `v24.18.0`.
fn major(version: &str) -> Option<u32> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

impl PluginHost {
    pub fn new(data_dir: &Path, node: Option<PathBuf>, server: &str, emit: Emit) -> Arc<Self> {
        Arc::new(Self {
            root: data_dir.join("plugins").join("run"),
            server: server.trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            api_token: Mutex::new(None),
            node,
            emit,
            channels: Mutex::new(HashMap::new()),
            subscribed: Mutex::new(HashMap::new()),
            inner: Mutex::new(Inner::default()),
            logs: Mutex::new(HashMap::new()),
            found_node: tokio::sync::OnceCell::new(),
            diagnostics: std::sync::OnceLock::new(),
            collections: Mutex::new(HashMap::new()),
            checkers: Mutex::new(std::collections::BTreeSet::new()),
        })
    }

    /// The Node that runs plugins here, checked once: 24 or later.
    async fn node(&self) -> Result<PathBuf, String> {
        self.found_node
            .get_or_init(|| async {
                let node = self.node.clone().unwrap_or_else(|| PathBuf::from("node"));
                let missing = || {
                    format!(
                        "Node {NODE_MAJOR} or later was not found ({}); set `[plugins] node` in config.toml",
                        node.display()
                    )
                };
                let output = tokio::process::Command::new(&node)
                    .arg("--version")
                    .stdin(Stdio::null())
                    .output()
                    .await
                    .map_err(|_| missing())?;
                let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                match major(&version) {
                    Some(found) if found >= NODE_MAJOR => Ok(node),
                    Some(_) => Err(format!(
                        "{} is Node {version}; plugins need {NODE_MAJOR} or later",
                        node.display()
                    )),
                    None => Err(missing()),
                }
            })
            .await
            .clone()
    }

    fn set_state(&self, report: StateReport) {
        if let Ok(data) = serde_json::to_value(&report) {
            (self.emit)("plugin.state", data, false);
        }
        lock(&self.inner)
            .states
            .insert(report.plugin.clone(), report);
    }

    fn state(
        &self,
        wanted: &Wanted,
        state: RunState,
        message: Option<String>,
        declared: Option<Value>,
    ) {
        self.set_state(StateReport {
            plugin: wanted.id.clone(),
            digest: wanted.digest.clone(),
            state,
            message,
            declared,
        });
    }

    /// Adds a line to a plugin's logs.
    pub fn log(&self, plugin: &str, line: impl Into<String>) {
        let line = format!(
            "{} {}",
            display(
                OffsetDateTime::now_utc(),
                format_description!("[hour]:[minute]:[second]")
            ),
            line.into()
        );
        let mut logs = lock(&self.logs);
        let lines = logs.entry(plugin.to_owned()).or_default();
        if lines.len() >= LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// A plugin's kept logs, oldest first.
    pub fn logs(&self, plugin: &str) -> Vec<String> {
        lock(&self.logs)
            .get(plugin)
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// A running plugin's process.
    pub fn process(&self, plugin: &str) -> Option<Arc<Process>> {
        lock(&self.inner)
            .running
            .get(plugin)
            .and_then(|supervised| lock(&supervised.process).clone())
    }

    fn build_dir(&self, id: &str, digest: &str) -> PathBuf {
        self.root.join(id).join(digest)
    }

    /// Whether the plugin's build is here whole (its entry is there); a
    /// partial one is removed, to be sent again.
    fn has_build(&self, plugin: &Wanted) -> bool {
        let dir = self.build_dir(&plugin.id, &plugin.digest);
        let whole = plugin
            .entry
            .as_deref()
            .is_some_and(|entry| dir.join(entry).is_file());
        if !whole && dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        whole
    }

    /// Runs what `request` lists and stops the rest; answers the builds this
    /// machine lacks, which the server then sends.
    pub fn sync(self: &Arc<Self>, request: SyncRequest) -> SyncAnswer {
        let order: Vec<String> = request.plugins.iter().map(|p| p.id.clone()).collect();
        let wanted: HashMap<String, Wanted> = request
            .plugins
            .into_iter()
            .map(|plugin| (plugin.id.clone(), plugin))
            .collect();
        let mut stopping = Vec::new();
        let mut missing = Vec::new();
        let mut start = Vec::new();
        {
            let mut inner = lock(&self.inner);
            inner.machine = request.machine;
            inner.order = order;
            // Running plugins whose settings changed are told.
            for (id, plugin) in &wanted {
                let before = inner.wanted.get(id).map(|w| &w.settings);
                if before.is_some_and(|before| before != &plugin.settings)
                    && let Some(supervised) = inner.running.get(id)
                    && let Some(process) = lock(&supervised.process).clone()
                {
                    process.notify("settings.changed", json!({ "settings": plugin.settings }));
                }
            }
            let ids: Vec<String> = inner.running.keys().cloned().collect();
            for id in ids {
                let keep = inner.running.get(&id).is_some_and(|supervised| {
                    !supervised.task.is_finished()
                        && wanted
                            .get(&id)
                            .is_some_and(|w| w.digest == supervised.digest)
                });
                if !keep && let Some(supervised) = inner.running.remove(&id) {
                    stopping.push((id, supervised));
                }
            }
            inner
                .failed
                .retain(|id, digest| wanted.get(id).is_some_and(|w| &w.digest == digest));
            for plugin in wanted.values() {
                if inner.running.contains_key(&plugin.id)
                    || inner.failed.get(&plugin.id) == Some(&plugin.digest)
                {
                    continue;
                }
                if plugin.entry.is_none() {
                    start.push((plugin.clone(), false));
                } else if self.has_build(plugin) {
                    start.push((plugin.clone(), true));
                } else {
                    missing.push(Missing {
                        id: plugin.id.clone(),
                        digest: plugin.digest.clone(),
                    });
                }
            }
            let gone: Vec<String> = inner
                .states
                .keys()
                .filter(|id| !wanted.contains_key(*id))
                .cloned()
                .collect();
            for id in gone {
                inner.states.remove(&id);
            }
            inner.wanted = wanted;
        }
        for (id, supervised) in stopping {
            let host = Arc::clone(self);
            tokio::spawn(async move {
                let _ = supervised.stop.send(true);
                let _ = supervised.task.await;
                host.prune(&id).await;
            });
        }
        for missing in &missing {
            // Not under the lock: reporting takes it.
            let plugin = lock(&self.inner).wanted.get(&missing.id).cloned();
            if let Some(plugin) = plugin {
                self.state(&plugin, RunState::Receiving, None, None);
            }
        }
        for (plugin, runnable) in start {
            if runnable {
                self.start(plugin);
            } else {
                self.state(
                    &plugin,
                    RunState::Unavailable,
                    Some("Rust daemon entries are not supported yet".to_owned()),
                    None,
                );
            }
        }
        SyncAnswer {
            missing,
            states: self.states(),
        }
    }

    /// Every plugin's latest state, for a server that just connected.
    pub fn states(&self) -> Vec<StateReport> {
        lock(&self.inner).states.values().cloned().collect()
    }

    /// Removes builds of `id` that nothing runs; the whole folder when the
    /// plugin is no longer wanted.
    async fn prune(&self, id: &str) {
        let keep = lock(&self.inner).wanted.get(id).map(|w| w.digest.clone());
        let Ok(mut entries) = tokio::fs::read_dir(self.root.join(id)).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if keep.as_deref() != Some(name.as_str()) && !name.starts_with('.') {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }

    /// Stores a build the server sent, after checking it is the one asked
    /// for, and starts it if it is still wanted.
    pub async fn receive(
        self: &Arc<Self>,
        id: &str,
        digest: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        if files.iter().any(|(path, _)| !inside(path)) {
            return Err("a file outside the plugin's folder".to_owned());
        }
        if digest_of(&files) != digest {
            return Err(format!("the build of {id} arrived damaged"));
        }
        let target = self.build_dir(id, digest);
        if !target.is_dir() {
            // Staged apart, so pruning the plugin's folder never touches it.
            let staging = self
                .root
                .join(".incoming")
                .join(format!("{id}-{digest}-{}", uuid::Uuid::now_v7()));
            for (path, bytes) in &files {
                let to = staging.join(path);
                if let Some(parent) = to.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                tokio::fs::write(&to, bytes)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            let moved = tokio::fs::rename(&staging, &target).await;
            if moved.is_err() {
                let _ = tokio::fs::remove_dir_all(&staging).await;
                // Another copy of the same build got there first.
                if !target.is_dir() {
                    return Err(format!("cannot store the build of {id}"));
                }
            }
        }
        let wanted = lock(&self.inner).wanted.get(id).cloned();
        let running = lock(&self.inner).running.contains_key(id);
        if let Some(plugin) = wanted
            && plugin.digest == digest
            && !running
        {
            self.start(plugin);
            self.prune(id).await;
        }
        Ok(())
    }

    fn start(self: &Arc<Self>, plugin: Wanted) {
        let (stop, stopped) = watch::channel(false);
        let process = Arc::new(Mutex::new(None));
        let host = Arc::clone(self);
        let slot = Arc::clone(&process);
        let digest = plugin.digest.clone();
        let id = plugin.id.clone();
        let task = tokio::spawn(async move { host.supervise(plugin, stopped, slot).await });
        lock(&self.inner).running.insert(
            id,
            Supervised {
                digest,
                stop,
                task,
                process,
            },
        );
    }

    /// Runs one entry until told to stop: starts it, restarts it after a
    /// crash with a growing pause, and gives up after too many crashes.
    async fn supervise(
        self: Arc<Self>,
        plugin: Wanted,
        mut stop: watch::Receiver<bool>,
        slot: Arc<Mutex<Option<Arc<Process>>>>,
    ) {
        let mut crashes: VecDeque<tokio::time::Instant> = VecDeque::new();
        loop {
            self.state(&plugin, RunState::Starting, None, None);
            let node = match self.node().await {
                Ok(node) => node,
                Err(message) => {
                    self.log(&plugin.id, &message);
                    self.state(&plugin, RunState::Unavailable, Some(message), None);
                    let _ = stop.changed().await;
                    return;
                }
            };
            let ended = self.run_once(&plugin, &node, &mut stop, &slot).await;
            *lock(&slot) = None;
            let message = match ended {
                Ended::Stopped => {
                    self.state(&plugin, RunState::Stopped, None, None);
                    return;
                }
                Ended::Crashed(message) => message,
                Ended::Refused(message) => {
                    self.log(&plugin.id, format!("not started: {message}"));
                    lock(&self.inner)
                        .failed
                        .insert(plugin.id.clone(), plugin.digest.clone());
                    self.state(&plugin, RunState::Failed, Some(message), None);
                    return;
                }
            };
            self.log(&plugin.id, format!("stopped: {message}"));
            let Some(pause) = after_crash(&mut crashes, tokio::time::Instant::now()) else {
                let failed = format!(
                    "{message}; it crashed {CRASHES} times in two minutes, so it stays off until it changes or is enabled again"
                );
                lock(&self.inner)
                    .failed
                    .insert(plugin.id.clone(), plugin.digest.clone());
                self.state(&plugin, RunState::Failed, Some(failed), None);
                return;
            };
            self.state(
                &plugin,
                RunState::Restarting,
                Some(format!(
                    "{message}; starting again in {} s",
                    pause.as_secs()
                )),
                None,
            );
            tokio::select! {
                () = tokio::time::sleep(pause) => {}
                _ = stop.changed() => {
                    self.state(&plugin, RunState::Stopped, None, None);
                    return;
                }
            }
        }
    }

    /// Starts the entry once and waits until it ends or is told to stop.
    async fn run_once(
        self: &Arc<Self>,
        plugin: &Wanted,
        node: &Path,
        stop: &mut watch::Receiver<bool>,
        slot: &Arc<Mutex<Option<Arc<Process>>>>,
    ) -> Ended {
        let dir = self.build_dir(&plugin.id, &plugin.digest);
        let Some(entry) = plugin.entry.as_deref() else {
            return Ended::Crashed("no daemon entry".to_owned());
        };
        let machine = lock(&self.inner).machine.clone();
        let mut command = tokio::process::Command::new(node);
        command
            .arg("--enable-source-maps")
            .arg(dir.join(entry))
            .current_dir(&dir)
            .env_clear()
            .env("AHAB_PLUGIN", &plugin.id)
            .env("AHAB_MACHINE", &machine)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for key in [
            "PATH",
            "HOME",
            "USER",
            "LANG",
            "TZ",
            "KUBECONFIG",
            "DOCKER_HOST",
            "XDG_RUNTIME_DIR",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Ended::Crashed(format!("could not start Node: {error}")),
        };
        self.log(
            &plugin.id,
            format!(
                "started build {}",
                &plugin.digest[..12.min(plugin.digest.len())]
            ),
        );
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Ended::Crashed("its pipes did not open".to_owned());
        };
        let process = self.connect(&plugin.id, stdin, stdout, stderr);
        let initialized = process
            .request(
                "initialize",
                json!({
                    "plugin": plugin.id,
                    "machine": machine,
                    "ahabVersion": PLUGIN_API,
                    "settings": plugin.settings,
                }),
                INITIALIZE,
            )
            .await;
        match initialized {
            Ok(declared) => {
                if let Err(problem) = speaks(&declared["protocol"]) {
                    let _ = process.request("shutdown", json!({}), GRACE).await;
                    let _ = child.kill().await;
                    return Ended::Refused(problem);
                }
                *lock(slot) = Some(Arc::clone(&process));
                self.subscribe(&plugin.id, &declared["events"]);
                self.declare_check(&plugin.id, &declared["diagnostics"]);
                self.declare_hooks(&plugin.id, &declared["hooks"]);
                self.state(plugin, RunState::Running, None, Some(declared));
            }
            Err(failure) => {
                let _ = child.kill().await;
                return Ended::Crashed(format!("it did not start: {}", failure.message));
            }
        }
        tokio::select! {
            status = child.wait() => {
                let code = status.ok().and_then(|status| status.code());
                Ended::Crashed(code.map_or_else(|| "it was killed".to_owned(), |code| format!("it exited with code {code}")))
            }
            _ = stop.changed() => {
                let _ = process.request("shutdown", json!({}), GRACE).await;
                if tokio::time::timeout(GRACE, child.wait()).await.is_err() {
                    let _ = child.kill().await;
                }
                self.log(
                    &plugin.id,
                    format!("stopped build {}", &plugin.digest[..12.min(plugin.digest.len())]),
                );
                Ended::Stopped
            }
        }
    }

    /// Records the hooks `plugin` intercepts: those it declared that its
    /// manifest lists. Others are logged and never asked.
    fn declare_hooks(&self, plugin: &str, declared: &Value) {
        let mut refused = Vec::new();
        {
            let mut inner = lock(&self.inner);
            let allowed = inner.wanted.get(plugin).and_then(|w| w.hooks.clone());
            let hooks = declared
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|hook| {
                    let listed = allowed
                        .as_ref()
                        .is_none_or(|allowed| allowed.iter().any(|a| a == hook));
                    if !listed {
                        refused.push((*hook).to_owned());
                    }
                    listed
                })
                .map(str::to_owned)
                .collect();
            inner.hooks.insert(plugin.to_owned(), hooks);
        }
        for hook in refused {
            self.log(
                plugin,
                format!("intercepts {hook}, which its manifest does not list: it is not asked"),
            );
        }
    }

    /// Speaks the protocol on a started entry's pipes.
    fn connect(
        self: &Arc<Self>,
        plugin: &str,
        mut stdin: tokio::process::ChildStdin,
        mut stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
    ) -> Arc<Process> {
        let (writer, mut outgoing) = mpsc::channel::<Vec<u8>>(256);
        tokio::spawn(async move {
            while let Some(bytes) = outgoing.recv().await {
                if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });
        let pending: Arc<Pending> = Arc::default();
        let process = Arc::new(Process {
            writer,
            pending: Arc::clone(&pending),
            next: AtomicU64::new(1),
        });
        let host = Arc::clone(self);
        let id = plugin.to_owned();
        let answering = Arc::downgrade(&process);
        tokio::spawn(async move {
            let mut decoder = Decoder::default();
            let mut chunk = vec![0; 64 * 1024];
            loop {
                let read = match stdout.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => read,
                };
                let frames = match decoder.push(&chunk[..read]) {
                    Ok(frames) => frames,
                    Err(error) => {
                        host.log(&id, format!("its output is not the protocol: {error}"));
                        break;
                    }
                };
                for frame in frames {
                    host.handle(&id, &pending, answering.upgrade(), frame);
                }
            }
            for (_, waiter) in lock(&pending).drain() {
                let _ = waiter.send(Err(PluginFailure::new(
                    "plugin_not_running",
                    "The plugin stopped.",
                )));
            }
            host.end_channels(&id, "The plugin stopped.");
            // A collection is cleared when its plugin stops.
            host.clear_collections(&id);
        });
        let host = Arc::clone(self);
        let id = plugin.to_owned();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                host.log(&id, format!("[stderr] {line}"));
            }
        });
        process
    }

    /// A message from a plugin's process.
    fn handle(
        self: &Arc<Self>,
        plugin: &str,
        pending: &Pending,
        process: Option<Arc<Process>>,
        frame: Frame,
    ) {
        if let Some(channel) = frame.channel {
            let sender = lock(&self.channels)
                .get(&channel)
                .map(|(_, sender)| sender.clone());
            if let Some(sender) = sender
                && sender.try_send(ChannelOut::Data(frame.body)).is_err()
            {
                // A reader too slow to keep up: the channel ends.
                self.close_channel(plugin, channel);
            }
            return;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&frame.body) else {
            self.log(plugin, "a message that is not JSON");
            return;
        };
        match message.get("method").and_then(Value::as_str) {
            None => {
                let Some(id) = message.get("id").and_then(Value::as_u64) else {
                    return;
                };
                let Some(waiter) = lock(pending).remove(&id) else {
                    return;
                };
                let answer = match message.get("error") {
                    Some(error) => Err(PluginFailure {
                        code: error["data"]["code"]
                            .as_str()
                            .unwrap_or("command_failed")
                            .to_owned(),
                        message: error["message"]
                            .as_str()
                            .unwrap_or("The plugin failed.")
                            .to_owned(),
                    }),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = waiter.send(answer);
            }
            Some("log") => {
                let level = message["params"]["level"].as_str().unwrap_or("info");
                let text = message["params"]["message"].as_str().unwrap_or_default();
                self.log(plugin, format!("[{level}] {text}"));
            }
            Some("events.publish") => {
                let params = &message["params"];
                let Some(name) = params["name"].as_str() else {
                    return;
                };
                (self.emit)(
                    "plugin.event",
                    json!({ "plugin": plugin, "name": name, "data": params["data"] }),
                    params["lasting"].as_bool().unwrap_or(false),
                );
            }
            Some("channels.close") => {
                let params = &message["params"];
                if let Some(channel) = params["channel"].as_str().and_then(|id| id.parse().ok()) {
                    let removed = lock(&self.channels).remove(&channel);
                    if let Some((_, sender)) = removed {
                        let reason = params["reason"].as_str().map(str::to_owned);
                        let _ = sender.try_send(ChannelOut::End(reason));
                    }
                }
            }
            Some("events.subscribe") => {
                self.subscribe(plugin, &message["params"]["names"]);
            }
            Some("hooks.declare") => {
                self.declare_hooks(plugin, &message["params"]["hooks"]);
            }
            Some("diagnostics.declare") => {
                self.declare_check(plugin, &message["params"]);
            }
            Some(method @ ("diagnostics.set" | "diagnostics.delete" | "diagnostics.clear")) => {
                self.collection(plugin, method, &message["params"]);
            }
            Some("notifications.send") => {
                let (Some(id), Some(process)) = (message.get("id").cloned(), process) else {
                    return;
                };
                let host = Arc::clone(self);
                let plugin = plugin.to_owned();
                tokio::spawn(async move {
                    let answered = host
                        .api(
                            reqwest::Method::POST,
                            &format!("/plugins/{plugin}/notifications"),
                            Some(message["params"].clone()),
                        )
                        .await;
                    process.answer(id, answered);
                });
            }
            Some("commands.call") => {
                let (Some(id), Some(process)) = (message.get("id").cloned(), process) else {
                    return;
                };
                let host = Arc::clone(self);
                let plugin = plugin.to_owned();
                tokio::spawn(async move {
                    let answered = host.call(&plugin, &message["params"]).await;
                    process.answer(id, answered);
                });
            }
            Some(method) => {
                // Requests the host does not offer yet are answered as such;
                // notifications are left.
                if let (Some(id), Some(process)) = (message.get("id").cloned(), process) {
                    process.send(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("Ahab does not offer {method} yet."),
                            "data": { "code": "method_unknown" },
                        },
                    }));
                }
            }
        }
    }

    /// A call from `plugin` to Ahab (`commands.call`): another plugin's
    /// command, or Ahab's own (`terminals.create`, `terminals.list`,
    /// `terminals.kill`), through the server's API.
    async fn call(&self, plugin: &str, params: &Value) -> Result<Value, PluginFailure> {
        let name = params["name"].as_str().unwrap_or_default();
        let input = params.get("input").cloned().unwrap_or(json!({}));
        let project = params["project"]
            .as_str()
            .or_else(|| input["project"].as_str())
            .map(str::to_owned);
        match name {
            "terminals.create" => {
                let Some(project) = project else {
                    return Err(PluginFailure::new(
                        "invalid_input",
                        "terminals.create needs a project",
                    ));
                };
                let mut command = input["command"].as_str().map(str::to_owned);
                if let Some(args) = input["args"].as_array() {
                    let quoted: Vec<String> = args
                        .iter()
                        .filter_map(Value::as_str)
                        .map(shell_quote)
                        .collect();
                    let mut joined = command.unwrap_or_default();
                    for arg in quoted {
                        if !joined.is_empty() {
                            joined.push(' ');
                        }
                        joined.push_str(&arg);
                    }
                    command = Some(joined);
                }
                let body = json!({
                    "title": input["title"],
                    "command": command,
                    "cols": input["cols"].as_u64().unwrap_or(120),
                    "rows": input["rows"].as_u64().unwrap_or(32),
                    "plugin": plugin,
                });
                let session = self
                    .api(
                        reqwest::Method::POST,
                        &format!("/projects/{project}/terminals"),
                        Some(body),
                    )
                    .await?;
                Ok(json!({ "session": session["id"], "terminal": session["terminal"] }))
            }
            "terminals.list" => {
                let list = self
                    .api(
                        reqwest::Method::GET,
                        &format!("/terminals?plugin={plugin}"),
                        None,
                    )
                    .await?;
                Ok(list["items"].clone())
            }
            "terminals.kill" => {
                let terminal = input["terminal"].as_str().unwrap_or_default();
                let found = self
                    .api(
                        reqwest::Method::GET,
                        &format!("/terminals/{terminal}"),
                        None,
                    )
                    .await?;
                if found["plugin"].as_str() != Some(plugin) {
                    return Err(PluginFailure::new(
                        "terminal_not_found",
                        "This plugin did not start that terminal.",
                    ));
                }
                self.api(
                    reqwest::Method::POST,
                    &format!("/terminals/{terminal}/kill"),
                    Some(json!({})),
                )
                .await
            }
            _ => {
                let Some((target, command)) = name.split_once('.') else {
                    return Err(PluginFailure::new(
                        "command_unknown",
                        format!("{name} is not a command (plugin.command)"),
                    ));
                };
                let body = json!({
                    "input": input,
                    "machine": params["machine"],
                    "project": project,
                });
                let answer = self
                    .api(
                        reqwest::Method::POST,
                        &format!("/plugins/{target}/commands/{command}"),
                        Some(body),
                    )
                    .await?;
                Ok(answer["output"].clone())
            }
        }
    }

    /// Where plugins' diagnostics go; set once, by the daemon.
    pub fn wire_diagnostics(&self, wiring: DiagnosticsWiring) {
        let _ = self.diagnostics.set(wiring);
    }

    /// Notes whether `plugin` checks on demand.
    fn declare_check(&self, plugin: &str, declared: &Value) {
        let mut checkers = lock(&self.checkers);
        if declared["check"].as_bool() == Some(true) {
            checkers.insert(plugin.to_owned());
        } else {
            checkers.remove(plugin);
        }
    }

    /// A plugin's `diagnostics.set`, `.delete` or `.clear`: paths outside
    /// the project's folder and projects this machine does not serve are
    /// dropped, and logged.
    fn collection(&self, plugin: &str, method: &str, params: &Value) {
        let Some(wiring) = self.diagnostics.get() else {
            return;
        };
        let name = params["collection"].as_str().unwrap_or_default();
        if !valid_collection(name) {
            self.log(
                plugin,
                format!("{method}: {name:?} is not a collection name (a-z, 0-9, -)"),
            );
            return;
        }
        let source = collection_source(plugin, name);
        let projects: Vec<uuid::Uuid> = match params["project"].as_str() {
            Some(id) => {
                let Ok(id) = id.parse() else {
                    self.log(plugin, format!("{method}: {id:?} is not a project"));
                    return;
                };
                vec![id]
            }
            // Clearing without a project clears it everywhere.
            None if method == "diagnostics.clear" => lock(&self.collections)
                .get(plugin)
                .into_iter()
                .flatten()
                .filter(|(_, s)| *s == source)
                .map(|(project, _)| *project)
                .collect(),
            None => {
                self.log(plugin, format!("{method} needs a project"));
                return;
            }
        };
        for project in projects {
            let Some(folder) = (wiring.folder_of)(project) else {
                self.log(
                    plugin,
                    format!("{method}: no project {project} on this machine"),
                );
                continue;
            };
            let mut files = std::collections::BTreeMap::new();
            let mut replace_all = false;
            match method {
                "diagnostics.set" => {
                    let given = params["files"].as_object().cloned().unwrap_or_default();
                    for (path, items) in given {
                        let Some(path) = confined(&path, &folder) else {
                            self.log(
                                plugin,
                                format!("{method}: {path} is outside the project; dropped"),
                            );
                            continue;
                        };
                        let items: Vec<Value> = items
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(lsp_diagnostic)
                            .collect();
                        files.insert(path, items);
                    }
                }
                "diagnostics.delete" => {
                    for path in params["paths"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if let Some(path) = confined(path, &folder) {
                            files.insert(path, Vec::new());
                        }
                    }
                }
                _ => replace_all = true,
            }
            {
                let mut collections = lock(&self.collections);
                let held = collections.entry(plugin.to_owned()).or_default();
                if replace_all {
                    held.remove(&(project, source.clone()));
                } else {
                    held.insert((project, source.clone()));
                }
            }
            (wiring.out)(PluginDiagnostics {
                project,
                source: source.clone(),
                files,
                replace_all,
            });
        }
    }

    /// Clears every collection of `plugin` (it stopped, or was disabled).
    fn clear_collections(&self, plugin: &str) {
        lock(&self.checkers).remove(plugin);
        let held = lock(&self.collections).remove(plugin).unwrap_or_default();
        let Some(wiring) = self.diagnostics.get() else {
            return;
        };
        for (project, source) in held {
            (wiring.out)(PluginDiagnostics {
                project,
                source,
                files: std::collections::BTreeMap::new(),
                replace_all: true,
            });
        }
    }

    /// Asks every plugin that checks on demand to check `roots` (folders
    /// relative to the project: its repositories or worktrees) for
    /// `reason` (`request`, `agentTurn`, `save`); waits for them all.
    pub async fn check(&self, project: uuid::Uuid, folder: &Path, roots: &[String], reason: &str) {
        let checkers: Vec<String> = lock(&self.checkers).iter().cloned().collect();
        let asked = checkers.into_iter().filter_map(|plugin| {
            let process = self.process(&plugin)?;
            Some(async move {
                let answer = process
                    .request(
                        "diagnostics.check",
                        json!({
                            "project": { "id": project, "folder": folder },
                            "roots": roots,
                            "reason": reason,
                        }),
                        CHECK_LIMIT,
                    )
                    .await;
                if let Err(failure) = answer {
                    self.log(&plugin, format!("its check failed: {}", failure.message));
                }
            })
        });
        futures_util::future::join_all(asked).await;
    }

    /// The token the server gave the daemon's connection: plugins' calls
    /// to the API carry it.
    pub fn set_api_token(&self, token: Option<String>) {
        *lock(&self.api_token) = token;
    }

    /// Calls the server's API (`path` after `/api/v1`); its errors as the
    /// plugin sees them.
    async fn api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, PluginFailure> {
        let mut request = self
            .http
            .request(method, format!("{}/api/v1{path}", self.server))
            .timeout(COMMAND_LIMIT);
        let token = lock(&self.api_token).clone();
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| PluginFailure::new("ahab_unreachable", error.to_string()))?;
        let ok = response.status().is_success();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if ok {
            Ok(value)
        } else {
            let mut message = value["detail"]
                .as_str()
                .or_else(|| value["title"].as_str())
                .unwrap_or("Ahab refused the call.")
                .to_owned();
            // What was wrong, field by field (`validation_failed`).
            let fields: Vec<String> = value["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|error| {
                    Some(format!(
                        "{}: {}",
                        error["field"].as_str()?,
                        error["message"].as_str()?
                    ))
                })
                .collect();
            if !fields.is_empty() {
                message = format!("{message}: {}", fields.join("; "));
            }
            Err(PluginFailure::new(
                value["code"].as_str().unwrap_or("failed"),
                message,
            ))
        }
    }

    /// Runs a plugin's command, for `project` (its ID and folder) when
    /// given; dropping the future cancels it in the plugin.
    pub async fn command(
        &self,
        plugin: &str,
        name: &str,
        input: Value,
        project: Option<(uuid::Uuid, PathBuf)>,
        settings: Value,
    ) -> Result<Value, PluginFailure> {
        let Some(process) = self.process(plugin) else {
            return Err(PluginFailure::new(
                "plugin_not_running",
                format!("The plugin {plugin} is not running on this machine."),
            ));
        };
        let mut context = match project {
            Some((id, folder)) => json!({ "project": { "id": id, "folder": folder } }),
            None => json!({}),
        };
        if settings.is_object() {
            context["settings"] = settings;
        }
        process
            .request(
                "commands.run",
                json!({ "name": name, "input": input, "context": context }),
                COMMAND_LIMIT,
            )
            .await
    }

    /// Hands `notification` to `plugin`'s destination `destination`: it
    /// returns, or fails to say the delivery failed.
    pub async fn notify(
        &self,
        plugin: &str,
        destination: &str,
        notification: Value,
        settings: Value,
    ) -> Result<Value, PluginFailure> {
        let Some(process) = self.process(plugin) else {
            return Err(PluginFailure::new(
                "plugin_not_running",
                format!("The plugin {plugin} is not running on this machine."),
            ));
        };
        process
            .request(
                "notifications.deliver",
                json!({ "destination": destination, "notification": notification, "settings": settings }),
                DELIVER_LIMIT,
            )
            .await
    }

    /// Opens the plugin's channel `name` as `id`: what the plugin sends on
    /// it comes out of the answer.
    pub async fn open_channel(
        &self,
        plugin: &str,
        name: &str,
        params: Value,
        id: uuid::Uuid,
    ) -> Result<mpsc::Receiver<ChannelOut>, PluginFailure> {
        let Some(process) = self.process(plugin) else {
            return Err(PluginFailure::new(
                "plugin_not_running",
                format!("The plugin {plugin} is not running on this machine."),
            ));
        };
        let (sender, receiver) = mpsc::channel(CHANNEL_FRAMES);
        // Registered first: the plugin may write as soon as it opens it.
        lock(&self.channels).insert(id, (plugin.to_owned(), sender));
        let opened = process
            .request(
                "channels.open",
                json!({ "name": name, "channel": id, "params": params }),
                INITIALIZE,
            )
            .await;
        if let Err(failure) = opened {
            lock(&self.channels).remove(&id);
            return Err(failure);
        }
        Ok(receiver)
    }

    /// Sends bytes to the plugin on a channel.
    pub fn write_channel(&self, plugin: &str, id: uuid::Uuid, bytes: Vec<u8>) {
        if let Some(process) = self.process(plugin) {
            let _ = process.writer.try_send(encode(&Frame {
                channel: Some(id),
                body: bytes,
            }));
        }
    }

    /// Closes a channel from Ahab's side: the plugin is told.
    pub fn close_channel(&self, plugin: &str, id: uuid::Uuid) {
        if lock(&self.channels).remove(&id).is_some()
            && let Some(process) = self.process(plugin)
        {
            process.notify("channels.close", json!({ "channel": id }));
        }
    }

    /// Ends every channel of a plugin that stopped.
    fn end_channels(&self, plugin: &str, reason: &str) {
        let ended: Vec<mpsc::Sender<ChannelOut>> = {
            let mut channels = lock(&self.channels);
            let ids: Vec<uuid::Uuid> = channels
                .iter()
                .filter(|(_, (owner, _))| owner == plugin)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| channels.remove(&id).map(|(_, sender)| sender))
                .collect()
        };
        for sender in ended {
            let _ = sender.try_send(ChannelOut::End(Some(reason.to_owned())));
        }
    }

    /// Asks every running plugin that intercepts `hook`, in order, about
    /// `data`: each may let it be, change it (the next sees the change) or
    /// refuse it. Answers the data and the plugins that changed it, or the
    /// plugin that refused and why. A plugin that does not answer in time
    /// is passed over.
    pub async fn before(
        &self,
        hook: &str,
        data: Value,
    ) -> Result<(Value, Vec<String>), (String, String)> {
        self.before_only(hook, data, None).await
    }

    /// `before`, asking only the plugins in `only` (when given): the server
    /// asks each plugin on one machine.
    pub async fn before_only(
        &self,
        hook: &str,
        mut data: Value,
        only: Option<&[String]>,
    ) -> Result<(Value, Vec<String>), (String, String)> {
        let asked: Vec<String> = {
            let inner = lock(&self.inner);
            inner
                .order
                .iter()
                .filter(|id| only.is_none_or(|only| only.contains(id)))
                .filter(|id| {
                    inner
                        .hooks
                        .get(*id)
                        .is_some_and(|hooks| hooks.iter().any(|h| h == hook))
                })
                .cloned()
                .collect()
        };
        let mut changed_by = Vec::new();
        for plugin in asked {
            let Some(process) = self.process(&plugin) else {
                continue;
            };
            let answer = process
                .request(
                    "hooks.before",
                    json!({ "hook": hook, "data": data }),
                    HOOK_LIMIT,
                )
                .await;
            match answer {
                Ok(answer) => match answer["action"].as_str() {
                    Some("change") => {
                        data = answer["data"].clone();
                        changed_by.push(plugin);
                    }
                    Some("deny") => {
                        let reason = answer["reason"].as_str().unwrap_or("No reason given.");
                        self.log(&plugin, format!("refused {hook}: {reason}"));
                        return Err((plugin, reason.to_owned()));
                    }
                    _ => {}
                },
                Err(failure) => {
                    self.log(
                        &plugin,
                        format!("passed over for {hook}: {}", failure.message),
                    );
                }
            }
        }
        Ok((data, changed_by))
    }

    /// Notes the events `plugin` listens to (a list of names).
    fn subscribe(&self, plugin: &str, names: &Value) {
        let names = names
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        lock(&self.subscribed).insert(plugin.to_owned(), names);
    }

    /// Gives an event to every running plugin that listens to it.
    pub fn deliver(&self, name: &str, data: &Value) {
        let listening: Vec<String> = lock(&self.subscribed)
            .iter()
            .filter(|(_, names)| names.contains(name))
            .map(|(plugin, _)| plugin.clone())
            .collect();
        for plugin in listening {
            if let Some(process) = self.process(&plugin) {
                process.notify("events.deliver", json!({ "name": name, "data": data }));
            }
        }
    }

    /// Stops every plugin, when the daemon stops.
    pub async fn shutdown(&self) {
        let running: Vec<Supervised> = lock(&self.inner).running.drain().map(|(_, s)| s).collect();
        for supervised in &running {
            let _ = supervised.stop.send(true);
        }
        for supervised in running {
            let _ = supervised.task.await;
        }
    }
}

/// Counts a crash at `now`: the pause before starting again (1, 2, 4…
/// up to 30 seconds), or `None` when it crashed too often to try again.
fn after_crash(
    crashes: &mut VecDeque<tokio::time::Instant>,
    now: tokio::time::Instant,
) -> Option<Duration> {
    crashes.push_back(now);
    while crashes
        .front()
        .is_some_and(|at| now.duration_since(*at) > CRASH_WINDOW)
    {
        crashes.pop_front();
    }
    if crashes.len() >= CRASHES {
        return None;
    }
    Some(Duration::from_secs(1 << (crashes.len() - 1).min(5)).min(BACKOFF_MAX))
}

/// `arg` as one word for a POSIX shell.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@%+-,".contains(c));
    if plain {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// How one run of an entry ended.
enum Ended {
    Stopped,
    Crashed(String),
    /// It cannot run here, and starting it again will not help.
    Refused(String),
}

/// Whether a daemon entry's `protocol` is one this Ahab speaks: the same
/// version, or a compatible one (the same minor before 1.0, the same major
/// from then).
fn speaks(protocol: &Value) -> Result<(), String> {
    let offered = semver::Version::parse(PLUGIN_API).map_err(|error| error.to_string())?;
    let Some(text) = protocol.as_str() else {
        return Err(format!(
            "it does not say which plugin protocol it speaks; this Ahab speaks {PLUGIN_API}"
        ));
    };
    let compatible = semver::Version::parse(text).is_ok_and(|theirs| {
        theirs.major == offered.major && (offered.major > 0 || theirs.minor == offered.minor)
    });
    if compatible {
        Ok(())
    } else {
        Err(format!(
            "it speaks plugin protocol {text}, and this Ahab speaks {PLUGIN_API}: build it with an SDK for this Ahab"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use serde_json::json;

    use super::{after_crash, major, shell_quote, speaks};

    #[test]
    fn entries_speaking_another_protocol_are_refused() {
        assert!(speaks(&json!("0.1.0")).is_ok());
        assert!(speaks(&json!("0.1.7")).is_ok());
        assert!(speaks(&json!("0.2.0")).is_err());
        assert!(speaks(&json!("1.0.0")).is_err());
        assert!(speaks(&json!(null)).is_err());
        assert!(speaks(&json!("soon")).is_err());
    }

    #[test]
    fn arguments_are_quoted_for_the_shell() {
        assert_eq!(shell_quote("logs"), "logs");
        assert_eq!(shell_quote("app=web"), "app=web");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn crashes_back_off_then_give_up() {
        let start = tokio::time::Instant::now();
        let mut crashes = VecDeque::new();
        let pauses: Vec<Option<u64>> = (0..5)
            .map(|i| after_crash(&mut crashes, start + Duration::from_secs(i)).map(|p| p.as_secs()))
            .collect();
        assert_eq!(pauses, [Some(1), Some(2), Some(4), Some(8), None]);
        // Crashes long ago no longer count.
        let mut old = VecDeque::new();
        for i in 0..4 {
            after_crash(&mut old, start + Duration::from_secs(i));
        }
        assert_eq!(
            after_crash(&mut old, start + Duration::from_secs(300)),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn reads_node_versions() {
        assert_eq!(major("v24.18.0\n"), Some(24));
        assert_eq!(major("v26.13.0"), Some(26));
        assert_eq!(major("nonsense"), None);
    }
}
