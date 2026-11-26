//! The daemon's language servers: started the first time a file
//! of their language opens under a root, one per root and tool, shared by
//! every window; stopped a few minutes after the last window under them
//! closes; restarted after a crash (with backoff, then given up); at most
//! a setting's number at once, the least recently used stopping first.
//! Windows reach them through a channel each (`lsp.connect`).

pub mod places;
pub mod rpc;
pub mod server;
pub mod text;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::modules::projects::Checkout;
use crate::modules::tools::lsp::api::{LanguageServerDto, LanguageServerListDto};
use crate::modules::tools::lsp::daemon::places::{EXTERNAL, Mapper, Target, describe, uri_of};
use crate::modules::tools::lsp::daemon::server::{Event, Server, State};
use crate::modules::tools::lsp::daemon::text::Change;
use crate::modules::tools::problems::api::DiagnosticsEvent;
use crate::modules::tools::problems::daemon::{self as checks, relative};
use crate::modules::tools::problems::model::Diagnostic;
use crate::modules::tools::registry::api::ToolContext;
use crate::modules::tools::registry::daemon::find::version_of;
use crate::modules::tools::registry::daemon::plan::{
    Effective, applies, config_file, find_root, repository_settings,
};
use crate::modules::tools::registry::daemon::{Launch, Readiness, ToolHost};
use crate::modules::tools::registry::model::{ToolKind, ToolRecord};
use crate::modules::tools::setting::model::{ServerStart, ToolSettings};
use crate::shared::sync::lock;

/// A server no window uses stops after this.
const IDLE_STOP: Duration = Duration::from_secs(180);
/// How often idle servers are looked for.
const IDLE_CHECK: Duration = Duration::from_secs(30);
/// The largest external file served.
const EXTERNAL_LIMIT: u64 = 2 << 20;

type Key = (PathBuf, String);

/// What a server of a tool never seen running is taken to need.
const NEW_SERVER_ESTIMATE: u64 = 256 << 20;

/// This machine's memory, in bytes (0 when it cannot be read).
pub fn machine_memory() -> u64 {
    static TOTAL: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(|| {
        sysinfo::System::new().map_or(0, |mut system| {
            system.refresh_memory();
            system.total_memory()
        })
    })
}

/// A running server, as the memory budget sees it.
#[derive(Debug, Clone)]
struct Candidate {
    id: Uuid,
    memory: u64,
    in_use: bool,
    idle: Duration,
}

/// The servers to stop, in order, so that those left and a new one taking
/// `estimate` fit `budget`: unused ones first, then the least recently
/// used. None when they fit, or when the budget is unknown.
fn over_budget(running: &[Candidate], budget: u64, estimate: u64) -> Vec<Uuid> {
    if budget == 0 {
        return Vec::new();
    }
    let mut order: Vec<&Candidate> = running.iter().filter(|c| !c.in_use).collect();
    order.sort_by_key(|c| std::cmp::Reverse(c.idle));
    let mut total: u64 = running.iter().map(|c| c.memory).sum();
    let mut stop = Vec::new();
    for candidate in order {
        if total.saturating_add(estimate) <= budget {
            break;
        }
        total = total.saturating_sub(candidate.memory);
        stop.push(candidate.id);
    }
    stop
}

/// Every language server on this machine.
pub struct LanguageServers {
    pub host: Arc<ToolHost>,
    servers: Mutex<HashMap<Key, Arc<Server>>>,
    lifecycle: tokio::sync::Mutex<()>,
    exits: mpsc::UnboundedSender<(Uuid, Duration)>,
    /// The memory budget last asked for, in bytes (0: half the machine's).
    budget: AtomicU64,
    /// What each tool's servers last took, in bytes: a new one's estimate.
    footprint: Mutex<HashMap<String, u64>>,
    /// Each project's pinned repositories and worktrees (their folders).
    pins: Mutex<HashMap<Uuid, Vec<String>>>,
    /// Problems from elsewhere (plugins' collections), by project and
    /// source, then file: windows get them as they get servers'.
    extra: Mutex<HashMap<(Uuid, String), BTreeMap<String, Value>>>,
    extra_changes: tokio::sync::broadcast::Sender<Arc<ExtraChange>>,
    next_window: AtomicU64,
}

/// A file's problems from elsewhere changed.
#[derive(Debug)]
pub struct ExtraChange {
    project: Uuid,
    source: String,
    path: String,
    items: Value,
}

impl LanguageServers {
    pub fn new(host: Arc<ToolHost>) -> Arc<Self> {
        let (exits, mut exited) = mpsc::unbounded_channel::<(Uuid, Duration)>();
        let servers = Arc::new(Self {
            host,
            servers: Mutex::new(HashMap::new()),
            lifecycle: tokio::sync::Mutex::new(()),
            exits,
            budget: AtomicU64::new(0),
            footprint: Mutex::new(HashMap::new()),
            pins: Mutex::new(HashMap::new()),
            extra: Mutex::new(HashMap::new()),
            extra_changes: tokio::sync::broadcast::channel(256).0,
            next_window: AtomicU64::new(1),
        });
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            // Crashed servers start again after their backoff.
            let weak = Arc::downgrade(&servers);
            runtime.spawn(async move {
                while let Some((id, wait)) = exited.recv().await {
                    let weak = weak.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(wait).await;
                        if let Some(manager) = weak.upgrade() {
                            manager.retry(id).await;
                        }
                    });
                }
            });
            // Servers no window uses stop.
            let weak = Arc::downgrade(&servers);
            runtime.spawn(async move {
                loop {
                    tokio::time::sleep(IDLE_CHECK).await;
                    let Some(manager) = weak.upgrade() else {
                        return;
                    };
                    manager.stop_idle().await;
                }
            });
        }
        servers
    }

    /// Remembers the machine's memory budget (MiB), when one is given.
    pub fn set_budget(&self, mib: Option<u64>) {
        self.budget.store(
            mib.map_or(0, |mib| mib.saturating_mul(1 << 20)),
            Ordering::Relaxed,
        );
    }

    /// The memory budget in bytes: the setting, else half the machine's.
    pub fn budget(&self) -> u64 {
        match self.budget.load(Ordering::Relaxed) {
            0 => machine_memory() / 2,
            set => set,
        }
    }

    /// What each server takes now, in bytes, by ID; remembered per tool.
    fn memory(&self, servers: &[Arc<Server>]) -> HashMap<Uuid, u64> {
        let mut taken = HashMap::new();
        let roots: HashMap<_, _> = servers
            .iter()
            .filter_map(|server| Some((sysinfo::Pid::from_u32(server.pid()?), server)))
            .collect();
        if roots.is_empty() {
            return taken;
        }
        let Ok(mut system) = sysinfo::System::new() else {
            return taken;
        };
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
        for process in system.processes().values() {
            let mut pid = Some(process.pid());
            // Bound the walk even if a process disappears and a PID is reused.
            for _ in 0..64 {
                let Some(current) = pid else { break };
                if let Some(server) = roots.get(&current) {
                    *taken.entry(server.id).or_insert(0) += process.memory();
                    break;
                }
                pid = system.process(current).and_then(sysinfo::Process::parent);
            }
        }
        let mut footprint = lock(&self.footprint);
        for server in roots.values() {
            if let Some(memory) = taken.get(&server.id) {
                let known = footprint.entry(server.record.id.clone()).or_default();
                *known = (*known).max(*memory);
            }
        }
        taken
    }

    /// Before a server of `tool` starts: the least recently used servers
    /// stop while the running ones and the new one (as much as the tool
    /// took before) would pass the budget.
    async fn keep_to_budget(&self, tool: Option<&str>) {
        let servers: Vec<Arc<Server>> = lock(&self.servers).values().cloned().collect();
        if servers.is_empty() {
            return;
        }
        let taken = self.memory(&servers);
        let estimate = tool.map_or(0, |tool| {
            lock(&self.footprint)
                .get(tool)
                .copied()
                .unwrap_or(NEW_SERVER_ESTIMATE)
        });
        let candidates: Vec<Candidate> = servers
            .iter()
            .map(|server| Candidate {
                id: server.id,
                memory: taken.get(&server.id).copied().unwrap_or_default(),
                in_use: server.in_use(),
                idle: server.idle_for(),
            })
            .collect();
        let budget = self.budget();
        let retiring = {
            let mut all = lock(&self.servers);
            over_budget(&candidates, budget, estimate)
                .into_iter()
                .filter_map(|id| {
                    let key = all
                        .iter()
                        .find(|(_, s)| s.id == id && !s.in_use())
                        .map(|(key, _)| key.clone())?;
                    all.remove(&key)
                })
                .collect::<Vec<_>>()
        };
        futures_util::future::join_all(retiring.iter().map(|server| async move {
            server.log(format!(
                "this machine exceeds its language server memory budget ({} MiB): stopping an unused server",
                budget >> 20
            ));
            server.stop().await;
        }))
        .await;
    }

    /// Problems from elsewhere (a plugin's collection) for files of a
    /// project: kept, and sent to the windows that have those files open.
    pub fn publish(
        &self,
        project: Uuid,
        source: &str,
        files: &BTreeMap<String, Vec<Value>>,
        replace_all: bool,
    ) {
        let mut changes = Vec::new();
        {
            let mut extra = lock(&self.extra);
            let held = extra.entry((project, source.to_owned())).or_default();
            if replace_all {
                for path in std::mem::take(held).into_keys() {
                    if !files.contains_key(&path) {
                        changes.push((path, json!([])));
                    }
                }
            }
            for (path, items) in files {
                if items.is_empty() {
                    held.remove(path);
                } else {
                    held.insert(path.clone(), json!(items));
                }
                changes.push((path.clone(), json!(items)));
            }
            if held.is_empty() {
                extra.remove(&(project, source.to_owned()));
            }
        }
        for (path, items) in changes {
            let _ = self.extra_changes.send(Arc::new(ExtraChange {
                project,
                source: source.to_owned(),
                path,
                items,
            }));
        }
    }

    /// The problems from elsewhere for a file.
    fn extra_for(&self, project: Uuid, path: &str) -> Vec<(String, Value)> {
        lock(&self.extra)
            .iter()
            .filter(|((w, _), _)| *w == project)
            .filter_map(|((_, source), files)| Some((source.clone(), files.get(path)?.clone())))
            .collect()
    }

    /// A project's pinned repositories and worktrees, as the server says.
    pub fn set_pins(&self, project: Uuid, pinned: Vec<String>) {
        lock(&self.pins).insert(project, pinned);
    }

    fn pinned(&self, project: Uuid, checkout: &Checkout) -> bool {
        lock(&self.pins).get(&project).is_some_and(|pins| {
            pins.iter().any(|pin| {
                pin == &checkout.path
                    || (!checkout.repository.is_empty() && pin == &checkout.repository)
            })
        })
    }

    fn by_id(&self, id: Uuid) -> Option<Arc<Server>> {
        lock(&self.servers).values().find(|s| s.id == id).cloned()
    }

    async fn retry(&self, id: Uuid) {
        let _lifecycle = self.lifecycle.lock().await;
        let Some(server) = self.by_id(id) else {
            return;
        };
        server.release_closed_windows();
        if server.in_use() {
            let exits = self.exits.clone();
            tokio::spawn(async move { server.start(exits).await });
        } else {
            lock(&self.servers).retain(|_, current| current.id != id);
            server.stop().await;
        }
    }

    async fn stop_idle(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let idle = {
            let mut servers = lock(&self.servers);
            let keys: Vec<_> = servers
                .iter()
                .filter(|(_, server)| {
                    server.release_closed_windows();
                    !server.in_use() && server.idle_for() >= IDLE_STOP
                })
                .map(|(key, _)| key.clone())
                .collect();
            keys.iter()
                .filter_map(|key| servers.remove(key))
                .collect::<Vec<_>>()
        };
        futures_util::future::join_all(idle.iter().map(|server| async move {
            server.log("no window uses it: stopping");
            server.stop().await;
        }))
        .await;
        self.keep_to_budget(None).await;
    }

    /// The server for `record` at `launch`'s root, started when there is
    /// none; unused servers make room when the soft limit is reached.
    async fn server_for(
        &self,
        record: &ToolRecord,
        launch: &Launch,
        settings: &ToolSettings,
        listener: (u64, mpsc::UnboundedSender<Event>),
    ) -> Arc<Server> {
        let _lifecycle = self.lifecycle.lock().await;
        let max = settings.max_servers();
        self.set_budget(settings.memory_budget);
        let key = (launch.root.clone(), record.id.clone());
        if let Some(server) = lock(&self.servers).get(&key).cloned()
            && server.state() != State::Stopped
        {
            server.listen(listener.0, listener.1);
            return server;
        }
        let evicted = {
            let mut servers = lock(&self.servers);
            servers.remove(&key);
            let mut evicted = Vec::new();
            while servers.len() >= max as usize {
                // Unused first, then the least recently used.
                let Some(oldest) = servers
                    .iter()
                    .filter(|(_, s)| !s.in_use())
                    .max_by_key(|(_, s)| s.idle_for())
                    .map(|(key, _)| key.clone())
                else {
                    break;
                };
                if let Some(server) = servers.remove(&oldest) {
                    evicted.push(server);
                }
            }
            evicted
        };
        futures_util::future::join_all(evicted.iter().map(|server| async move {
            server.log(format!(
                "more than {max} servers would run on this machine: this one, used least recently, stops"
            ));
            server.stop().await;
        }))
        .await;
        self.keep_to_budget(Some(&record.id)).await;
        let options = settings
            .tools
            .get(&record.id)
            .and_then(|choice| choice.settings.clone())
            .unwrap_or(Value::Null);
        let server = Server::new(
            record.clone(),
            launch.clone(),
            options,
            settings.log_traffic == Some(true),
        );
        server.listen(listener.0, listener.1);
        lock(&self.servers).insert(key, Arc::clone(&server));
        let starting = Arc::clone(&server);
        let exits = self.exits.clone();
        tokio::spawn(async move { starting.start(exits).await });
        server
    }

    /// The running servers, for the machine's LSPs tab.
    pub fn list(&self) -> LanguageServerListDto {
        let servers: Vec<Arc<Server>> = lock(&self.servers).values().cloned().collect();
        let taken = self.memory(&servers);
        let mut items: Vec<LanguageServerDto> = servers
            .iter()
            .map(|server| {
                let state = server.state();
                LanguageServerDto {
                    id: server.id,
                    tool: server.record.id.clone(),
                    name: server.record.name.clone(),
                    root: server.root.display().to_string(),
                    languages: server.record.languages.clone(),
                    state: state.name().to_owned(),
                    detail: match state {
                        State::Failed(reason) => Some(reason),
                        _ => server.progress(),
                    },
                    pid: server.pid(),
                    memory: taken.get(&server.id).copied(),
                    started_at: server.started_at,
                    documents: server.document_count(),
                }
            })
            .collect();
        items.sort_by(|a, b| (&a.root, &a.tool).cmp(&(&b.root, &b.tool)));
        let memory: u64 = items.iter().filter_map(|item| item.memory).sum();
        LanguageServerListDto {
            items,
            memory: Some(memory),
            memory_budget: Some(self.budget()),
            machine_memory: Some(machine_memory()),
        }
    }

    /// Restarts a server; false when there is none.
    pub async fn restart(&self, id: Uuid) -> bool {
        let lifecycle = self.lifecycle.lock().await;
        let Some(server) = self.by_id(id) else {
            return false;
        };
        server.log("restart asked");
        server.prepare_restart().await;
        drop(lifecycle);
        server.start(self.exits.clone()).await;
        true
    }

    pub fn log(&self, id: Uuid) -> Option<Vec<String>> {
        self.by_id(id).map(|server| server.log_lines())
    }

    /// Stops `tool`'s servers (it was updated or removed); windows using
    /// them start them again as they now find the tool.
    pub async fn replace_tool(&self, tool: &str) {
        let _lifecycle = self.lifecycle.lock().await;
        let servers: Vec<Arc<Server>> = {
            let mut servers = lock(&self.servers);
            let keys: Vec<Key> = servers
                .keys()
                .filter(|(_, id)| id == tool)
                .cloned()
                .collect();
            keys.iter().filter_map(|key| servers.remove(key)).collect()
        };
        for server in servers {
            server.log("its tool changed: stopping");
            server.stop().await;
        }
    }

    /// Stops every server, when the daemon stops.
    pub async fn shutdown(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let servers: Vec<Arc<Server>> = lock(&self.servers).drain().map(|(_, s)| s).collect();
        futures_util::future::join_all(servers.iter().map(|server| server.stop())).await;
    }

    /// Serves one window's channel until it closes: frames from the window
    /// come on `from_window`, frames to it go on `to_window`.
    pub async fn serve(
        self: Arc<Self>,
        mut from_window: mpsc::Receiver<Vec<u8>>,
        to_window: Writer,
        project: Uuid,
        folder: PathBuf,
        context: ToolContext,
        report: Arc<dyn Fn(DiagnosticsEvent) + Send + Sync>,
    ) {
        let id = self.next_window.fetch_add(1, Ordering::Relaxed);
        let (events, mut heard) = mpsc::unbounded_channel();
        let mut extras = self.extra_changes.subscribe();
        let mut window = Window {
            id,
            manager: Arc::clone(&self),
            project,
            folder,
            context,
            writer: to_window.clone(),
            events,
            documents: HashMap::new(),
            in_flight: HashMap::new(),
            checks_running: Arc::new(Mutex::new(HashSet::new())),
            report,
            reported: HashSet::new(),
        };
        loop {
            tokio::select! {
                frame = from_window.recv() => {
                    let Some(frame) = frame else { break };
                    let Ok(frame) = serde_json::from_slice::<Value>(&frame) else { continue };
                    window.receive(frame).await;
                }
                event = heard.recv() => {
                    let Some(event) = event else { break };
                    window.heard(event).await;
                }
                change = extras.recv() => {
                    match change {
                        Ok(change) => window.extra(&change).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        window.close();
    }
}

/// A file open in a window.
struct Document {
    abs: PathBuf,
    language: String,
    version: i32,
    text: String,
    effective: Arc<Effective>,
    /// Its tools, in order of preference, and whether each can start.
    tools: Vec<(ToolRecord, PathBuf, Readiness)>,
    servers: Vec<Arc<Server>>,
    /// An external file: the server that granted it, and what it is.
    external: Option<(Arc<Server>, Target)>,
    /// The repository or worktree it is in (its folder): roots stop there.
    boundary: PathBuf,
    /// Why its language servers were not started (the project's setting).
    held: Option<String>,
}

struct Window {
    id: u64,
    manager: Arc<LanguageServers>,
    project: Uuid,
    folder: PathBuf,
    context: ToolContext,
    writer: Writer,
    events: mpsc::UnboundedSender<Event>,
    documents: HashMap<String, Document>,
    in_flight: HashMap<i64, (String, String, tokio::task::JoinHandle<()>)>,
    checks_running: Arc<Mutex<HashSet<(String, PathBuf)>>>,
    report: Arc<dyn Fn(DiagnosticsEvent) + Send + Sync>,
    /// The repositories and worktrees whose use this window told already.
    reported: HashSet<String>,
}

/// Where a window's frames go.
pub type Writer = mpsc::Sender<Vec<u8>>;

/// Sends a frame on a window's channel.
async fn send(writer: &Writer, frame: &Value) -> bool {
    writer.send(frame.to_string().into_bytes()).await.is_ok()
}

impl Window {
    async fn receive(&mut self, frame: Value) {
        let path = frame["path"].as_str().unwrap_or_default().to_owned();
        match frame["type"].as_str().unwrap_or_default() {
            "open" => {
                let language = frame["language"].as_str().unwrap_or_default().to_owned();
                let version = frame["version"]
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(1);
                let text = frame["text"].as_str().unwrap_or_default().to_owned();
                self.open(&path, &language, version, text).await;
            }
            "change" => {
                let Ok(changes) = serde_json::from_value::<Vec<Change>>(frame["changes"].clone())
                else {
                    return;
                };
                let version = frame["version"]
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                if let Some(document) = self.documents.get_mut(&path) {
                    text::apply(&mut document.text, &changes);
                    document.version = version;
                    for server in &document.servers {
                        server.change(&document.abs, version, &changes).await;
                    }
                    self.pull(&path);
                }
            }
            "save" => self.saved(&path).await,
            "close" => {
                self.close_document(&path).await;
            }
            "request" => self.request(frame).await,
            "cancel" => {
                if let Some(id) = frame["id"].as_i64()
                    && let Some((_, _, task)) = self.in_flight.remove(&id)
                {
                    task.abort();
                }
            }
            _ => {}
        }
    }

    /// The absolute path of a window's path, inside the project.
    fn absolute(&self, path: &str) -> Option<PathBuf> {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return None;
        }
        Some(self.folder.join(relative))
    }

    /// A granted external file: the server that granted it and its target.
    fn external(&self, path: &str) -> Option<(Arc<Server>, Target)> {
        let token = path.strip_prefix(EXTERNAL)?;
        self.servers()
            .into_iter()
            .find_map(|server| server.granted(token).map(|target| (server, target)))
    }

    /// Every server this window uses.
    fn servers(&self) -> Vec<Arc<Server>> {
        let mut found: Vec<Arc<Server>> = Vec::new();
        for document in self.documents.values() {
            for server in &document.servers {
                if !found.iter().any(|known| known.id == server.id) {
                    found.push(Arc::clone(server));
                }
            }
        }
        found
    }

    async fn open(&mut self, path: &str, language: &str, version: i32, text: String) {
        if let Some((server, target)) = self.external(path) {
            // External files are read where the server that granted them is.
            let abs = match &target {
                Target::File(file) => file.clone(),
                Target::Address(_) => PathBuf::from(path),
            };
            if matches!(target, Target::File(_)) {
                server.listen(self.id, self.events.clone());
                server.open(self.id, &abs, language, version, &text).await;
            }
            self.documents.insert(
                path.to_owned(),
                Document {
                    abs,
                    language: language.to_owned(),
                    version,
                    text,
                    effective: Arc::new(Effective::default()),
                    tools: Vec::new(),
                    servers: vec![Arc::clone(&server)],
                    external: Some((server, target)),
                    boundary: self.folder.clone(),
                    held: None,
                },
            );
            self.send_status(path).await;
            return;
        }
        let Some(abs) = self.absolute(path) else {
            return;
        };
        let checkouts = self.manager.host.checkouts(&self.folder).await;
        let checkout = checkouts.of(path);
        self.used(&checkout);
        let boundary = self.folder.join(&checkout.path);
        let repository = repository_settings(abs.parent().unwrap_or(&self.folder), &self.folder);
        let effective = Arc::new(Effective::new(&self.context, repository));
        let held = match effective.settings.servers() {
            ServerStart::Off => Some(
                "Language servers are turned off for this project (Project settings → LSPs)."
                    .to_owned(),
            ),
            ServerStart::Pinned if !self.manager.pinned(self.project, &checkout) => Some(
                "Language servers start only in pinned repositories here; pin this one to start them."
                    .to_owned(),
            ),
            _ => None,
        };
        let mut tools = Vec::new();
        for record in effective.tools_for(language) {
            if !applies(record, &abs, &boundary) {
                continue;
            }
            let root = find_root(&abs, &boundary, record);
            let readiness = self
                .manager
                .host
                .prepare(record, &root, &self.folder, &effective, Some(&abs))
                .await;
            tools.push((record.clone(), root, readiness));
        }
        let mut servers = Vec::new();
        for (record, _, readiness) in &tools {
            if record.kind != ToolKind::LanguageServer || held.is_some() {
                continue;
            }
            if let Readiness::Ready(launch) = readiness {
                let server = self
                    .manager
                    .server_for(
                        record,
                        launch,
                        &effective.settings,
                        (self.id, self.events.clone()),
                    )
                    .await;
                server.open(self.id, &abs, language, version, &text).await;
                // Diagnostics it already has for the file.
                for (file, items) in server.diagnostics() {
                    if file == abs {
                        self.diagnostics(&server.record.id, &file, items);
                    }
                }
                servers.push(server);
            }
        }
        let linters: Vec<(ToolRecord, Launch)> = tools
            .iter()
            .filter(|(record, _, _)| record.kind == ToolKind::Linter)
            .filter_map(|(record, _, readiness)| match readiness {
                Readiness::Ready(launch) => Some((record.clone(), (**launch).clone())),
                _ => None,
            })
            .collect();
        self.documents.insert(
            path.to_owned(),
            Document {
                abs,
                language: language.to_owned(),
                version,
                text,
                effective,
                tools,
                servers,
                external: None,
                boundary,
                held,
            },
        );
        self.send_status(path).await;
        for (source, items) in self.manager.extra_for(self.project, path) {
            let off = self
                .documents
                .get(path)
                .is_some_and(|d| d.effective.settings.is_off(&source));
            if !off {
                send(
                    &self.writer,
                    &json!({ "type": "diagnostics", "path": path, "source": source, "items": items }),
                )
                .await;
            }
        }
        self.pull(path);
        for (record, launch) in linters {
            self.lint(record, launch);
        }
    }

    /// A file's problems from elsewhere changed: sent when it is open here
    /// and the project has not turned their source off.
    async fn extra(&self, change: &ExtraChange) {
        if change.project != self.project {
            return;
        }
        let Some(document) = self.documents.get(&change.path) else {
            return;
        };
        if document.effective.settings.is_off(&change.source) {
            return;
        }
        send(
            &self.writer,
            &json!({ "type": "diagnostics", "path": change.path,
                     "source": change.source, "items": change.items }),
        )
        .await;
    }

    /// Tells the server a repository or worktree was used (a file in it
    /// opened), once per window.
    fn used(&mut self, checkout: &Checkout) {
        if !self.reported.insert(checkout.path.clone()) {
            return;
        }
        (self.manager.host.report)(
            "tools.activity",
            json!({
                "project": self.project,
                "repository": checkout.repository,
                "worktree": checkout.worktree(),
            }),
        );
    }

    /// A file's language status: its tools and their states.
    async fn status(&self, path: &str) -> Value {
        let Some(document) = self.documents.get(path) else {
            return Value::Null;
        };
        if let Some((server, _)) = &document.external {
            return json!({
                "type": "status", "path": path, "language": document.language,
                "external": true, "tools": [{
                    "id": server.record.id, "name": server.record.name, "kind": "languageServer",
                    "state": server.state().name(),
                }],
            });
        }
        let mut tools = Vec::new();
        let mut environment = Value::Null;
        let mut root = Value::Null;
        for (record, tool_root, readiness) in &document.tools {
            let shown_root = tool_root
                .strip_prefix(&self.folder)
                .map(|rel| rel.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut entry = json!({
                "id": record.id, "name": record.name, "kind": record.kind.as_str(),
                "root": shown_root,
            });
            if record.kind == ToolKind::Check {
                entry["on"] = json!(document.effective.triggers(record));
            }
            match readiness {
                Readiness::Ready(launch) => {
                    let args = record
                        .run
                        .version_args
                        .clone()
                        .unwrap_or_else(|| vec!["--version".to_owned()]);
                    entry["version"] =
                        json!(version_of(&launch.found.path, &args, &launch.env).await);
                    entry["detail"] = json!(launch.found.detail);
                    if environment.is_null() {
                        environment = environment_value(&launch.environment, &self.folder);
                    }
                    if let Some(file) = config_file(record, &document.abs, &document.boundary)
                        .or_else(|| {
                            record
                                .run
                                .settings_files
                                .iter()
                                .map(|name| tool_root.join(name))
                                .find(|file| file.is_file())
                        })
                    {
                        entry["config"] = json!(relative_display(&file, &self.folder));
                    }
                    let server = document.servers.iter().find(|s| s.record.id == record.id);
                    match server {
                        Some(server) => {
                            let state = server.state();
                            entry["state"] = json!(state.name());
                            entry["server"] = json!(server.id);
                            if let State::Failed(reason) = state {
                                entry["reason"] = json!(reason);
                            } else if let Some(progress) = server.progress() {
                                entry["progress"] = json!(progress);
                            }
                            if root.is_null() {
                                root = json!({
                                    "path": shown_root,
                                    "marker": record.run.root_markers.iter()
                                        .find(|marker| tool_root.join(marker).exists()),
                                });
                            }
                        }
                        None if record.kind == ToolKind::LanguageServer
                            && document.held.is_some() =>
                        {
                            entry["state"] = json!("held");
                            entry["reason"] = json!(document.held);
                        }
                        None => entry["state"] = json!("ready"),
                    }
                }
                Readiness::Missing {
                    reason,
                    installable,
                    environment: found,
                } => {
                    entry["state"] = json!("missing");
                    entry["reason"] = json!(reason);
                    entry["installable"] = json!(installable);
                    if environment.is_null() {
                        environment = environment_value(found, &self.folder);
                    }
                }
                Readiness::CannotRun {
                    reason,
                    environment: found,
                } => {
                    entry["state"] = json!("cannotRun");
                    entry["reason"] = json!(reason);
                    if environment.is_null() {
                        environment = environment_value(found, &self.folder);
                    }
                }
            }
            tools.push(entry);
        }
        let configured = document
            .effective
            .languages
            .contains_key(&document.language)
            || document
                .effective
                .settings
                .languages
                .contains_key(&document.language);
        json!({
            "type": "status",
            "path": path,
            "language": document.language,
            "root": root,
            "environment": environment,
            "repository": document.effective.repository.as_ref().map(|file| relative_display(file, &self.folder)),
            "checkout": relative_display(&document.boundary, &self.folder),
            "tools": tools,
            "configured": configured,
            "off": document.effective.language_off(&document.language),
            "formatOnSave": document.effective.format_on_save(&document.language),
            "formatter": Self::formatter(document).map(|(record, _)| record.id.clone()),
        })
    }

    async fn send_status(&self, path: &str) {
        let status = self.status(path).await;
        if !status.is_null() {
            send(&self.writer, &status).await;
        }
    }

    /// What the servers said.
    async fn heard(&mut self, event: Event) {
        match event {
            Event::Diagnostics {
                source,
                path,
                items,
            } => self.diagnostics(&source, &path, items),
            Event::Status => {
                // A server stopped by an update or by the limit starts
                // again for the files that need it.
                let stale: Vec<String> = self
                    .documents
                    .iter()
                    .filter(|(_, d)| {
                        d.external.is_none()
                            && d.servers.iter().any(|s| s.state() == State::Stopped)
                    })
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in stale {
                    if let Some(document) = self.close_document(&path).await {
                        self.open(&path, &document.language, document.version, document.text)
                            .await;
                    }
                }
                let paths: Vec<String> = self.documents.keys().cloned().collect();
                for path in paths {
                    self.send_status(&path).await;
                }
            }
            Event::ApplyEdit { window, edit } => {
                if window != self.id {
                    return;
                }
                let mut edit = edit;
                let externals = self.map_answer(None, &mut edit);
                send(
                    &self.writer,
                    &json!({ "type": "applyEdit", "edit": edit, "externals": externals }),
                )
                .await;
            }
        }
    }

    /// Diagnostics for a file: to the window, and the server's snapshot.
    fn diagnostics(&self, source: &str, path: &Path, items: Value) {
        let Ok(relative) = path.strip_prefix(&self.folder) else {
            return;
        };
        let relative = relative.to_string_lossy().into_owned();
        let writer = self.writer.clone();
        let frame =
            json!({ "type": "diagnostics", "path": relative, "source": source, "items": items });
        tokio::spawn(async move { send(&writer, &frame).await });
        if let Ok(parsed) = serde_json::from_value::<Vec<Diagnostic>>(items) {
            (self.report)(DiagnosticsEvent {
                project: self.project,
                source: source.to_owned(),
                files: BTreeMap::from([(relative, parsed)]),
                replace_all: false,
                within: None,
            });
        }
    }

    /// Asks servers that pull diagnostics (LSP 3.17) for a file's.
    fn pull(&self, path: &str) {
        let Some(document) = self.documents.get(path) else {
            return;
        };
        for server in &document.servers {
            if server.capabilities()["diagnosticProvider"].is_null() {
                continue;
            }
            let server = Arc::clone(server);
            let abs = document.abs.clone();
            let version = document.version;
            let writer = self.writer.clone();
            let folder = self.folder.clone();
            let report = Arc::clone(&self.report);
            let project = self.project;
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                if server
                    .document_version(&abs)
                    .is_some_and(|v| v > version + 1)
                {
                    return;
                }
                let Ok(answer) = server
                    .request(
                        "textDocument/diagnostic",
                        json!({ "textDocument": { "uri": uri_of(&abs) } }),
                    )
                    .await
                else {
                    return;
                };
                if answer["kind"] != "full" {
                    return;
                }
                let Ok(relative) = abs.strip_prefix(&folder) else {
                    return;
                };
                let relative = relative.to_string_lossy().into_owned();
                let items = answer["items"].clone();
                send(
                    &writer,
                    &json!({ "type": "diagnostics", "path": relative,
                    "source": server.record.id, "items": items }),
                )
                .await;
                if let Ok(parsed) = serde_json::from_value::<Vec<Diagnostic>>(items) {
                    report(DiagnosticsEvent {
                        project,
                        source: server.record.id.clone(),
                        files: BTreeMap::from([(relative, parsed)]),
                        replace_all: false,
                        within: None,
                    });
                }
            });
        }
    }

    /// Runs a command-line linter on its file.
    fn lint(&self, record: ToolRecord, launch: Launch) {
        let writer = self.writer.clone();
        let folder = self.folder.clone();
        let report = Arc::clone(&self.report);
        let project = self.project;
        tokio::spawn(async move {
            let found = match checks::lint(&record, &launch).await {
                Ok(found) => found,
                Err(error) => {
                    tracing::debug!(tool = %record.id, %error, "a linter failed");
                    return;
                }
            };
            let files = relative(found, &folder);
            // A file the linter found nothing in is clean now.
            let mut files = files;
            if let Some(path) = launch_file(&launch, &folder) {
                files.entry(path).or_default();
            }
            for (path, items) in &files {
                send(
                    &writer,
                    &json!({ "type": "diagnostics", "path": path,
                    "source": record.id, "items": items }),
                )
                .await;
            }
            report(DiagnosticsEvent {
                project,
                source: record.id.clone(),
                files,
                replace_all: false,
                within: None,
            });
        });
    }

    /// A file was saved: servers hear of it, its linters and the checks
    /// that run on save run.
    async fn saved(&mut self, path: &str) {
        let Some(document) = self.documents.get(path) else {
            return;
        };
        for server in &document.servers {
            server.save(&document.abs).await;
        }
        for (record, root, readiness) in &document.tools {
            let Readiness::Ready(launch) = readiness else {
                continue;
            };
            match record.kind {
                ToolKind::Linter => self.lint(record.clone(), (**launch).clone()),
                ToolKind::Check
                    if document
                        .effective
                        .triggers(record)
                        .iter()
                        .any(|t| t == "save") =>
                {
                    let key = (record.id.clone(), root.clone());
                    if !lock(&self.checks_running).insert(key.clone()) {
                        continue;
                    }
                    let running = Arc::clone(&self.checks_running);
                    let host = Arc::clone(&self.manager.host);
                    let record = record.clone();
                    let root = root.clone();
                    let folder = self.folder.clone();
                    let effective = Arc::clone(&document.effective);
                    let writer = self.writer.clone();
                    let report = Arc::clone(&self.report);
                    let project = self.project;
                    tokio::spawn(async move {
                        // A save checks its file's root only, and waits its
                        // turn among the machine's checks.
                        let turn = host.checks.acquire(effective.settings.max_checks()).await;
                        let result =
                            checks::check_at(&host, &record, &root, &folder, &effective).await;
                        drop(turn);
                        lock(&running).remove(&key);
                        if !result.ok {
                            return;
                        }
                        for (path, items) in &result.files {
                            send(
                                &writer,
                                &json!({ "type": "diagnostics", "path": path,
                                "source": record.id, "items": items }),
                            )
                            .await;
                        }
                        send(&writer, &json!({ "type": "checked", "tool": record.id,
                            "root": result.root, "files": result.files.keys().collect::<Vec<_>>() })).await;
                        report(DiagnosticsEvent {
                            project,
                            source: record.id.clone(),
                            files: result.files,
                            replace_all: true,
                            within: Some(result.root.clone()),
                        });
                    });
                }
                _ => {}
            }
        }
    }

    /// The tool that formats a file: the one chosen, else one the project
    /// configures, else a server that formats, else a formatter.
    fn formatter(document: &Document) -> Option<(&ToolRecord, Option<Arc<Server>>)> {
        let can = |record: &ToolRecord| -> Option<Option<Arc<Server>>> {
            match record.kind {
                ToolKind::Formatter => document
                    .tools
                    .iter()
                    .any(|(r, _, readiness)| {
                        r.id == record.id && matches!(readiness, Readiness::Ready(_))
                    })
                    .then_some(None),
                ToolKind::LanguageServer => document
                    .servers
                    .iter()
                    .find(|server| {
                        server.record.id == record.id
                            && (server.state() != State::Running
                                || truthy(&server.capabilities()["documentFormattingProvider"]))
                    })
                    .map(|server| Some(Arc::clone(server))),
                _ => None,
            }
        };
        let chosen = document
            .effective
            .settings
            .languages
            .get(&document.language)
            .and_then(|choice| choice.formatter.clone());
        let records: Vec<&ToolRecord> = document.tools.iter().map(|(r, _, _)| r).collect();
        if let Some(chosen) = chosen
            && let Some(record) = records.iter().find(|r| r.id == chosen)
            && let Some(server) = can(record)
        {
            return Some((record, server));
        }
        let configured = records.iter().filter(|r| !r.run.config_files.is_empty());
        let servers = records
            .iter()
            .filter(|r| r.kind == ToolKind::LanguageServer && r.run.config_files.is_empty());
        let formatters = records
            .iter()
            .filter(|r| r.kind == ToolKind::Formatter && r.run.config_files.is_empty());
        configured
            .chain(servers)
            .chain(formatters)
            .find_map(|record| can(record).map(|server| (*record, server)))
    }

    async fn request(&mut self, frame: Value) {
        let Some(id) = frame["id"].as_i64() else {
            return;
        };
        let method = frame["method"].as_str().unwrap_or_default().to_owned();
        let path = frame["path"].as_str().unwrap_or_default().to_owned();
        let mut params = frame.get("params").cloned().unwrap_or(json!({}));
        // A newer request of the same kind for the same file cancels the older.
        let superseded: Vec<i64> = self
            .in_flight
            .iter()
            .filter(|(_, (m, p, _))| *m == method && *p == path && !method.ends_with("resolve"))
            .map(|(id, _)| *id)
            .collect();
        for old in superseded {
            if let Some((_, _, task)) = self.in_flight.remove(&old) {
                task.abort();
                send(
                    &self.writer,
                    &json!({ "type": "response", "id": old,
                    "error": { "code": "cancelled", "message": "a newer request replaced it" } }),
                )
                .await;
            }
        }
        self.in_flight.retain(|_, (_, _, task)| !task.is_finished());

        let answer: Result<(Value, Option<Arc<Server>>), Value> = match method.as_str() {
            "ahab/status" => {
                self.send_status(&path).await;
                Ok((Value::Null, None))
            }
            "ahab/externalFile" => self.external_file(&params).await.map(|value| (value, None)),
            "ahab/format" => {
                self.spawn_format(id, &path, params);
                return;
            }
            "workspace/symbol" => {
                let mut found = Vec::new();
                for server in self.servers() {
                    if server.state() != State::Running
                        || !truthy(&server.capabilities()["workspaceSymbolProvider"])
                    {
                        continue;
                    }
                    if let Ok(Value::Array(mut items)) =
                        server.request("workspace/symbol", params.clone()).await
                    {
                        let mut items = Value::Array(std::mem::take(&mut items));
                        let externals = self.map_answer(Some(&server), &mut items);
                        let _ = externals;
                        if let Value::Array(items) = items {
                            found.extend(items);
                        }
                    }
                }
                Ok((Value::Array(found), None))
            }
            _ => {
                let Some(document) = self.documents.get(&path) else {
                    send(
                        &self.writer,
                        &json!({ "type": "response", "id": id,
                        "error": { "code": "not_open", "message": "the file is not open" } }),
                    )
                    .await;
                    return;
                };
                let uri = match &document.external {
                    Some((_, Target::Address(address))) => address.clone(),
                    _ => uri_of(&document.abs),
                };
                if let Value::Object(map) = &mut params {
                    map.entry("textDocument")
                        .or_insert_with(|| json!({ "uri": uri }));
                }
                let chosen = frame["server"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok());
                let servers = document.servers.clone();
                if method == "textDocument/codeAction" && chosen.is_none() {
                    self.spawn_code_actions(id, servers, params);
                    return;
                }
                let capability = capability_of(&method);
                let server = match chosen {
                    Some(chosen) => servers.iter().find(|s| s.id == chosen).cloned(),
                    None => servers
                        .iter()
                        .find(|s| {
                            s.state() == State::Running
                                && capability.is_none_or(|key| truthy(&s.capabilities()[key]))
                        })
                        .cloned(),
                };
                match server {
                    Some(server) => {
                        if method == "workspace/executeCommand" {
                            server.commanded_by(self.id);
                        }
                        self.spawn_request(id, method.clone(), path.clone(), server, params);
                        return;
                    }
                    None if servers.iter().any(|s| s.state() == State::Starting) => Err(json!({
                        "code": "not_ready", "message": "the language server is starting"
                    })),
                    None if servers.is_empty() => Err(json!({
                        "code": "no_server", "message": "no language server runs for this file"
                    })),
                    None => Err(json!({
                        "code": "unsupported", "message": format!("no language server here answers {method}")
                    })),
                }
            }
        };
        let frame = match answer {
            Ok((result, _)) => json!({ "type": "response", "id": id, "result": result }),
            Err(error) => json!({ "type": "response", "id": id, "error": error }),
        };
        send(&self.writer, &frame).await;
    }

    /// Maps a server's answer for this window; how to show its externals.
    fn map_answer(
        &self,
        server: Option<&Arc<Server>>,
        value: &mut Value,
    ) -> BTreeMap<String, places::ExternalInfo> {
        let fallback = self.servers();
        let server = server.cloned().or_else(|| fallback.first().cloned());
        let mut grant = |target: Target| match &server {
            Some(server) => server.grant(target),
            None => String::new(),
        };
        let mut mapper = Mapper {
            folder: &self.folder,
            grant: &mut grant,
            externals: BTreeMap::new(),
        };
        mapper.map(value);
        mapper.externals
    }

    fn spawn_request(
        &mut self,
        id: i64,
        method: String,
        path: String,
        server: Arc<Server>,
        params: Value,
    ) {
        let writer = self.writer.clone();
        let folder = self.folder.clone();
        let task = tokio::spawn(async move {
            let answer = server.request(&method, params).await;
            let frame = match answer {
                Ok(mut result) => {
                    let externals = map_for(&server, &folder, &mut result);
                    json!({ "type": "response", "id": id, "result": result,
                        "server": server.id, "externals": externals })
                }
                Err(error) => {
                    json!({ "type": "response", "id": id, "error": error, "server": server.id })
                }
            };
            send(&writer, &frame).await;
        });
        self.in_flight
            .insert(id, ("request".to_owned(), path, task));
        let _ = method;
    }

    /// Code actions from every server that has them, each marked with the
    /// server that offered it (resolving goes back to it).
    fn spawn_code_actions(&mut self, id: i64, servers: Vec<Arc<Server>>, params: Value) {
        let writer = self.writer.clone();
        let folder = self.folder.clone();
        let task = tokio::spawn(async move {
            let mut actions = Vec::new();
            let mut externals = BTreeMap::new();
            for server in servers {
                if server.state() != State::Running
                    || !truthy(&server.capabilities()["codeActionProvider"])
                {
                    continue;
                }
                let Ok(mut answer) = server
                    .request("textDocument/codeAction", params.clone())
                    .await
                else {
                    continue;
                };
                externals.extend(map_for(&server, &folder, &mut answer));
                for mut action in answer.as_array().cloned().unwrap_or_default() {
                    action["ahabServer"] = json!(server.id);
                    action["ahabSource"] = json!(server.record.id);
                    actions.push(action);
                }
            }
            send(
                &writer,
                &json!({ "type": "response", "id": id, "result": actions, "externals": externals }),
            )
            .await;
        });
        self.in_flight.insert(
            id,
            ("textDocument/codeAction".to_owned(), String::new(), task),
        );
    }

    /// Formats a file: a server's edits, or a formatter's new text.
    fn spawn_format(&mut self, id: i64, path: &str, params: Value) {
        let writer = self.writer.clone();
        let Some(document) = self.documents.get(path) else {
            let frame = json!({ "type": "response", "id": id,
                "error": { "code": "not_open", "message": "the file is not open" } });
            tokio::spawn(async move { send(&writer, &frame).await });
            return;
        };
        let Some((record, server)) = Self::formatter(document) else {
            let frame = json!({ "type": "response", "id": id,
                "error": { "code": "no_formatter", "message": "nothing formats this file" } });
            tokio::spawn(async move { send(&writer, &frame).await });
            return;
        };
        let record = record.clone();
        let text = document.text.clone();
        let abs = document.abs.clone();
        let launch = document
            .tools
            .iter()
            .find(|(r, _, _)| r.id == record.id)
            .and_then(|(_, _, readiness)| match readiness {
                Readiness::Ready(launch) => Some((**launch).clone()),
                _ => None,
            });
        let options = params
            .get("options")
            .cloned()
            .unwrap_or(json!({ "tabSize": 4, "insertSpaces": true }));
        let task = tokio::spawn(async move {
            let frame = match server {
                Some(server) => {
                    match server
                        .request(
                            "textDocument/formatting",
                            json!({
                                "textDocument": { "uri": uri_of(&abs) }, "options": options
                            }),
                        )
                        .await
                    {
                        Ok(edits) => json!({ "type": "response", "id": id,
                            "result": { "tool": record.id, "edits": edits } }),
                        Err(error) => json!({ "type": "response", "id": id, "error": error }),
                    }
                }
                None => match launch {
                    Some(launch) => match checks::format(&launch, &text).await {
                        Ok(formatted) => json!({ "type": "response", "id": id,
                            "result": { "tool": record.id, "text": formatted } }),
                        Err(error) => json!({ "type": "response", "id": id,
                            "error": { "code": "format_failed", "message": format!("{} failed: {error}", record.name) } }),
                    },
                    None => json!({ "type": "response", "id": id,
                        "error": { "code": "no_formatter", "message": "the formatter cannot start" } }),
                },
            };
            send(&writer, &frame).await;
        });
        self.in_flight
            .insert(id, ("ahab/format".to_owned(), path.to_owned(), task));
    }

    /// An external file's text, by grant.
    async fn external_file(&self, params: &Value) -> Result<Value, Value> {
        let reference = params["ref"].as_str().unwrap_or_default();
        let path = format!("{EXTERNAL}{reference}");
        let Some((server, target)) = self.external(&path) else {
            return Err(json!({ "code": "not_granted",
                "message": "no language server pointed at this file in this session" }));
        };
        match target {
            Target::File(file) => {
                let metadata = std::fs::metadata(&file).map_err(
                    |error| json!({ "code": "unreadable", "message": error.to_string() }),
                )?;
                if metadata.len() > EXTERNAL_LIMIT {
                    return Err(
                        json!({ "code": "too_large", "message": "the file is over 2 MiB" }),
                    );
                }
                let bytes = std::fs::read(&file).map_err(
                    |error| json!({ "code": "unreadable", "message": error.to_string() }),
                )?;
                Ok(json!({
                    "text": String::from_utf8_lossy(&bytes),
                    "info": describe(&file),
                    "server": server.id,
                }))
            }
            Target::Address(address) => {
                let text = server
                    .request("java/classFileContents", json!({ "uri": address }))
                    .await?;
                Ok(json!({
                    "text": text.as_str().unwrap_or_default(),
                    "info": places::describe_address(&address),
                    "server": server.id,
                }))
            }
        }
    }

    async fn close_document(&mut self, path: &str) -> Option<Document> {
        let document = self.documents.remove(path)?;
        for server in &document.servers {
            let remaining = self
                .documents
                .values()
                .filter(|other| other.servers.iter().any(|current| current.id == server.id));
            if !remaining.clone().any(|other| other.abs == document.abs) {
                server.close(self.id, &document.abs).await;
            }
            if remaining.count() == 0 {
                server.unlisten(self.id);
            }
        }
        Some(document)
    }

    /// The window closed: its documents close in their servers.
    fn close(&mut self) {
        for (_, (_, _, task)) in self.in_flight.drain() {
            task.abort();
        }
        for server in self.servers() {
            server.unlisten(self.id);
        }
    }
}

/// `value` mapped for a window of `folder`, granting through `server`.
fn map_for(
    server: &Arc<Server>,
    folder: &Path,
    value: &mut Value,
) -> BTreeMap<String, places::ExternalInfo> {
    let mut grant = |target: Target| server.grant(target);
    let mut mapper = Mapper {
        folder,
        grant: &mut grant,
        externals: BTreeMap::new(),
    };
    mapper.map(value);
    mapper.externals
}

/// The file a linter's command names, relative to the project.
fn launch_file(launch: &Launch, folder: &Path) -> Option<String> {
    launch
        .args
        .iter()
        .map(Path::new)
        .find(|arg| arg.is_absolute() && arg.starts_with(folder))
        .and_then(|path| path.strip_prefix(folder).ok())
        .map(|rel| rel.to_string_lossy().into_owned())
}

fn relative_display(file: &Path, folder: &Path) -> String {
    file.strip_prefix(folder).map_or_else(
        |_| file.display().to_string(),
        |rel| rel.to_string_lossy().into_owned(),
    )
}

fn environment_value(
    environment: &crate::modules::tools::registry::daemon::environment::Environment,
    folder: &Path,
) -> Value {
    json!({
        "kind": environment.kind,
        "file": environment.file.as_ref().map(|file| relative_display(file, folder)),
        "note": environment.note,
    })
}

/// A capability that is on: `true` or an options object.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(on) => *on,
        Value::Null => false,
        _ => true,
    }
}

/// The server capability a request needs.
fn capability_of(method: &str) -> Option<&'static str> {
    Some(match method {
        "textDocument/hover" => "hoverProvider",
        "textDocument/definition" => "definitionProvider",
        "textDocument/declaration" => "declarationProvider",
        "textDocument/typeDefinition" => "typeDefinitionProvider",
        "textDocument/implementation" => "implementationProvider",
        "textDocument/references" => "referencesProvider",
        "textDocument/documentHighlight" => "documentHighlightProvider",
        "textDocument/documentSymbol" => "documentSymbolProvider",
        "textDocument/completion" | "completionItem/resolve" => "completionProvider",
        "textDocument/signatureHelp" => "signatureHelpProvider",
        "textDocument/rename" | "textDocument/prepareRename" => "renameProvider",
        "textDocument/codeAction" | "codeAction/resolve" => "codeActionProvider",
        "textDocument/formatting" => "documentFormattingProvider",
        "textDocument/rangeFormatting" => "documentRangeFormattingProvider",
        "textDocument/inlayHint" => "inlayHintProvider",
        "workspace/executeCommand" => "executeCommandProvider",
        _ => return None,
    })
}

#[cfg(test)]
mod budget {
    use super::*;

    fn server(n: u128, mib: u64, in_use: bool, idle_secs: u64) -> Candidate {
        Candidate {
            id: Uuid::from_u128(n),
            memory: mib << 20,
            in_use,
            idle: Duration::from_secs(idle_secs),
        }
    }

    #[test]
    fn the_least_recently_used_stop_until_a_new_server_fits() {
        let running = [
            server(1, 1000, true, 5),
            server(2, 1000, false, 10),
            server(3, 1000, true, 300),
            server(4, 500, false, 60),
        ];
        // 3500 MiB run; a 1000 MiB server in a 4000 MiB budget: one unused
        // goes, the one idle longest.
        assert_eq!(
            over_budget(&running, 4000 << 20, 1000 << 20),
            [Uuid::from_u128(4)]
        );
        assert_eq!(
            over_budget(&running, 5000 << 20, 1000 << 20),
            Vec::<Uuid>::new()
        );
        // Keep active roots available even when the soft budget cannot be met.
        assert_eq!(
            over_budget(&running, 2600 << 20, 1000 << 20),
            [4, 2].map(Uuid::from_u128)
        );
        assert!(over_budget(&running, 0, 1).is_empty(), "no budget known");
    }
}
