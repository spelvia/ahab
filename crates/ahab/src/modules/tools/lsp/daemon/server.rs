//! One language server process: one per root and tool on the machine,
//! shared by every window and project under that root. The daemon is its
//! only client: it keeps the open documents and answers what the server
//! asks of a client.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::modules::tools::lsp::daemon::places::{Target, uri_of};
use crate::modules::tools::lsp::daemon::rpc::{self, Message};
use crate::modules::tools::lsp::daemon::text::{self, Change};
use crate::modules::tools::registry::daemon::Launch;
use crate::modules::tools::registry::model::{ToolRecord, lsp_language_id};
use crate::modules::tools::setting::model::merge_json;
use crate::shared::sync::lock;

/// The log lines kept per server.
const LOG_LINES: usize = 2000;
const LOG_BYTES: usize = 512 * 1024;
const LOG_LINE_BYTES: usize = 8 * 1024;
/// How long `initialize` may take (jdtls and rust-analyzer can be slow).
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a request may take.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a server is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Starting,
    Running,
    /// It stopped and will not be started again by itself: why.
    Failed(String),
    Stopped,
}

impl State {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed(_) => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// What a server tells the windows that use it.
#[derive(Debug, Clone)]
pub enum Event {
    /// A file's diagnostics, replacing the server's earlier ones for it.
    Diagnostics {
        source: String,
        path: PathBuf,
        items: Value,
    },
    /// Its state or progress changed.
    Status,
    /// It wants an edit made (after a command a window ran).
    ApplyEdit { window: u64, edit: Value },
}

/// An open document, as the server has it.
struct Document {
    version: i32,
    text: String,
    language_id: String,
    /// The windows that have it open.
    holders: HashSet<u64>,
}

/// An answer, or the server's error.
pub type Answer = Result<Value, Value>;

pub struct Server {
    pub id: Uuid,
    pub record: ToolRecord,
    pub root: PathBuf,
    launch: Mutex<Launch>,
    /// What `workspace/configuration` answers from.
    settings: Mutex<Value>,
    initialization_options: Mutex<Option<Value>>,
    log_traffic: bool,
    state: Mutex<State>,
    capabilities: Mutex<Value>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    child: tokio::sync::Mutex<Option<Child>>,
    pid: Mutex<Option<u32>>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Answer>>>>,
    next_id: AtomicI64,
    documents: Mutex<HashMap<PathBuf, Document>>,
    document_updates: tokio::sync::Mutex<()>,
    diagnostics: Mutex<HashMap<PathBuf, Value>>,
    listeners: Mutex<HashMap<u64, mpsc::UnboundedSender<Event>>>,
    grants: Mutex<HashMap<String, Target>>,
    log: Mutex<(VecDeque<String>, usize)>,
    progress: Mutex<BTreeMap<String, String>>,
    /// When windows last heard of its progress.
    progress_told: Mutex<Option<Instant>>,
    pub started_at: time::OffsetDateTime,
    last_used: Mutex<Instant>,
    /// When it last crashed, for the backoff.
    crashes: Mutex<Vec<Instant>>,
    /// The window that last ran a command, for `workspace/applyEdit`.
    commanding: Mutex<Option<u64>>,
    /// Asked to stop: its end is not a crash.
    stopping: Mutex<bool>,
}

#[cfg(unix)]
fn terminate_descendants(pid: u32) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    // Each language server gets its own group at spawn, separate from Ahab.
    if let Ok(pid) = i32::try_from(pid) {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
}

#[cfg(windows)]
fn find_descendants(pid: u32) -> Vec<(sysinfo::Pid, u64)> {
    let Ok(mut system) = sysinfo::System::new() else {
        return Vec::new();
    };
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    let root = sysinfo::Pid::from_u32(pid);
    let mut descendants = Vec::new();
    for process in system.processes().values() {
        let mut parent = process.parent();
        for _ in 0..64 {
            let Some(pid) = parent else { break };
            if pid == root {
                descendants.push((process.pid(), process.start_time()));
                break;
            }
            parent = system.process(pid).and_then(sysinfo::Process::parent);
        }
    }
    descendants
}

#[cfg(windows)]
fn kill_descendants(descendants: Vec<(sysinfo::Pid, u64)>) {
    if descendants.is_empty() {
        return;
    }
    let Ok(mut system) = sysinfo::System::new() else {
        return;
    };
    let pids: Vec<_> = descendants.iter().map(|(pid, _)| *pid).collect();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&pids),
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    for (pid, start) in descendants {
        if let Some(process) = system.process(pid)
            && process.start_time() == start
        {
            process.kill();
        }
    }
}

#[cfg(windows)]
fn terminate_descendants(pid: u32) {
    kill_descendants(find_descendants(pid));
}

#[cfg(not(any(unix, windows)))]
fn terminate_descendants(_pid: u32) {}

/// Windows hear of a server's progress at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

/// Crashes within this long count together.
const CRASH_WINDOW: Duration = Duration::from_secs(300);
/// After this many crashes in the window it gives up.
const CRASHES_ALLOWED: usize = 3;

impl Server {
    pub fn new(
        record: ToolRecord,
        launch: Launch,
        settings: Value,
        log_traffic: bool,
    ) -> Arc<Self> {
        let initialization_options = record.run.initialization_options.clone();
        let mut configuration = record.run.settings.clone().unwrap_or(Value::Null);
        // A tool choice's settings go over the record's; `init` goes to
        // `initializationOptions`.
        let mut init = initialization_options;
        if let Value::Object(mut chosen) = settings {
            if let Some(extra) = chosen.remove("init") {
                let mut merged = init.unwrap_or(Value::Null);
                merge_json(&mut merged, &extra);
                init = Some(merged);
            }
            merge_json(&mut configuration, &Value::Object(chosen));
        }
        let root_uri = uri_of(&launch.root);
        let root_name = launch
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let fill = |value: &mut Value| fill_variables(value, &root_uri, &root_name, &launch.root);
        fill(&mut configuration);
        if let Some(init) = &mut init {
            fill(init);
        }
        Arc::new(Self {
            id: Uuid::now_v7(),
            root: launch.root.clone(),
            record,
            launch: Mutex::new(launch),
            settings: Mutex::new(configuration),
            initialization_options: Mutex::new(init),
            log_traffic,
            state: Mutex::new(State::Starting),
            capabilities: Mutex::new(Value::Null),
            stdin: tokio::sync::Mutex::new(None),
            child: tokio::sync::Mutex::new(None),
            pid: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicI64::new(1),
            documents: Mutex::new(HashMap::new()),
            document_updates: tokio::sync::Mutex::new(()),
            diagnostics: Mutex::new(HashMap::new()),
            listeners: Mutex::new(HashMap::new()),
            grants: Mutex::new(HashMap::new()),
            log: Mutex::new((VecDeque::new(), 0)),
            progress: Mutex::new(BTreeMap::new()),
            progress_told: Mutex::new(None),
            started_at: time::OffsetDateTime::now_utc(),
            last_used: Mutex::new(Instant::now()),
            crashes: Mutex::new(Vec::new()),
            commanding: Mutex::new(None),
            stopping: Mutex::new(false),
        })
    }

    pub fn state(&self) -> State {
        lock(&self.state).clone()
    }

    fn set_state(&self, state: State) {
        *lock(&self.state) = state;
        self.tell(&Event::Status);
    }

    pub fn capabilities(&self) -> Value {
        lock(&self.capabilities).clone()
    }

    pub fn pid(&self) -> Option<u32> {
        *lock(&self.pid)
    }

    /// What it is busy with ("indexing 40%").
    pub fn progress(&self) -> Option<String> {
        lock(&self.progress).values().last().cloned()
    }

    pub fn touch(&self) {
        *lock(&self.last_used) = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        lock(&self.last_used).elapsed()
    }

    /// Whether any window uses it.
    pub fn in_use(&self) -> bool {
        !lock(&self.listeners).is_empty() || !lock(&self.documents).is_empty()
    }

    pub fn document_count(&self) -> u32 {
        u32::try_from(lock(&self.documents).len()).unwrap_or(u32::MAX)
    }

    pub fn release_closed_windows(&self) {
        let closed: Vec<_> = lock(&self.listeners)
            .iter()
            .filter(|(_, sender)| sender.is_closed())
            .map(|(id, _)| *id)
            .collect();
        for id in closed {
            self.unlisten(id);
        }
    }

    pub fn log_lines(&self) -> Vec<String> {
        lock(&self.log).0.iter().cloned().collect()
    }

    pub fn log(&self, line: impl Into<String>) {
        let mut log = lock(&self.log);
        let (lines, bytes) = &mut *log;
        let stamp = time::OffsetDateTime::now_utc()
            .format(&time::macros::format_description!(
                "[hour]:[minute]:[second]"
            ))
            .unwrap_or_default();
        let line = line.into();
        let short = &line[..line.floor_char_boundary(LOG_LINE_BYTES)];
        let line = format!("{stamp} {short}");
        *bytes += line.len();
        lines.push_back(line);
        while lines.len() > LOG_LINES || *bytes > LOG_BYTES {
            if let Some(line) = lines.pop_front() {
                *bytes -= line.len();
            }
        }
    }

    pub fn listen(&self, window: u64, sender: mpsc::UnboundedSender<Event>) {
        lock(&self.listeners).insert(window, sender);
        self.touch();
    }

    pub fn unlisten(&self, window: u64) {
        lock(&self.listeners).remove(&window);
        let closed: Vec<PathBuf> = {
            let mut documents = lock(&self.documents);
            let mut closed = Vec::new();
            for (path, document) in documents.iter_mut() {
                if document.holders.remove(&window) && document.holders.is_empty() {
                    closed.push(path.clone());
                }
            }
            for path in &closed {
                documents.remove(path);
            }
            closed
        };
        for path in closed {
            self.notify_soon(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri_of(&path) } }),
            );
        }
        self.touch();
    }

    fn tell(&self, event: &Event) {
        for sender in lock(&self.listeners).values() {
            let _ = sender.send(event.clone());
        }
    }

    /// The latest diagnostics it published, per file.
    pub fn diagnostics(&self) -> Vec<(PathBuf, Value)> {
        lock(&self.diagnostics)
            .iter()
            .map(|(path, items)| (path.clone(), items.clone()))
            .collect()
    }

    /// Grants `target` for this server's lifetime: a token for it.
    pub fn grant(&self, target: Target) -> String {
        let mut grants = lock(&self.grants);
        if let Some((token, _)) = grants.iter().find(|(_, known)| **known == target) {
            return token.clone();
        }
        let token = Uuid::now_v7().simple().to_string();
        grants.insert(token.clone(), target);
        token
    }

    pub fn granted(&self, token: &str) -> Option<Target> {
        lock(&self.grants).get(token).cloned()
    }

    // --- Starting and stopping -------------------------------------------

    /// Starts the process and initializes it; documents still open are
    /// opened again (after a restart).
    pub async fn start(self: &Arc<Self>, on_exit: mpsc::UnboundedSender<(Uuid, Duration)>) {
        let mut slot = self.child.lock().await;
        if self.is_stopping() || slot.is_some() {
            return;
        }
        *lock(&self.state) = State::Starting;
        self.tell(&Event::Status);
        let launch = lock(&self.launch).clone();
        self.log(format!(
            "starting {} {} in {}",
            launch.program.display(),
            launch.args.join(" "),
            launch.root.display()
        ));
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&launch.root)
            .env_clear()
            .envs(&launch.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let reason = format!("cannot start it: {error}");
                self.log(&reason);
                self.set_state(State::Failed(reason));
                return;
            }
        };
        let Some(pid) = child.id() else {
            self.set_state(State::Failed("its process ID is missing".to_owned()));
            return;
        };
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            terminate_descendants(pid);
            self.set_state(State::Failed("its streams are missing".to_owned()));
            return;
        };
        *self.stdin.lock().await = Some(stdin);
        *slot = Some(child);
        *lock(&self.pid) = Some(pid);
        drop(slot);
        let reader = Arc::clone(self);
        tokio::spawn(async move {
            let mut stdout = BufReader::new(stdout);
            loop {
                match rpc::read(&mut stdout).await {
                    Ok(Some(message)) => reader.receive(message).await,
                    Ok(None) => break,
                    Err(error) => {
                        reader.log(format!("unreadable output: {error}"));
                        break;
                    }
                }
            }
            reader.exited(pid, on_exit).await;
        });
        let logger = Arc::clone(self);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                logger.log(format!("stderr: {line}"));
            }
        });
        let initialized = tokio::time::timeout(INITIALIZE_TIMEOUT, self.initialize()).await;
        if self.pid() != Some(pid) || self.is_stopping() {
            return;
        }
        match initialized {
            Ok(Ok(capabilities)) => {
                *lock(&self.capabilities) = capabilities;
                self.notify("initialized", json!({})).await;
                let settings = lock(&self.settings).clone();
                if !settings.is_null() {
                    self.notify(
                        "workspace/didChangeConfiguration",
                        json!({ "settings": settings }),
                    )
                    .await;
                }
                let _updates = self.document_updates.lock().await;
                let documents: Vec<(PathBuf, String, i32, String)> = lock(&self.documents)
                    .iter()
                    .map(|(path, d)| {
                        (
                            path.clone(),
                            d.language_id.clone(),
                            d.version,
                            d.text.clone(),
                        )
                    })
                    .collect();
                for (path, language_id, version, text) in documents {
                    self.did_open(&path, &language_id, version, &text).await;
                }
                let _slot = self.child.lock().await;
                if self.pid() == Some(pid) && !self.is_stopping() {
                    self.log("running");
                    self.set_state(State::Running);
                }
            }
            Ok(Err(error)) => {
                let reason = format!("it did not initialize: {error}");
                self.log(&reason);
                self.failed_start(pid, reason).await;
            }
            Err(_) => {
                let reason = "it did not initialize in time".to_owned();
                self.log(&reason);
                self.failed_start(pid, reason).await;
            }
        }
    }

    async fn initialize(&self) -> Result<Value, String> {
        let root_uri = uri_of(&self.root);
        let name = self
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "Ahab", "version": env!("CARGO_PKG_VERSION") },
            "rootPath": self.root,
            "rootUri": root_uri,
            "workspaceFolders": [{ "uri": root_uri, "name": name }],
            "capabilities": client_capabilities(),
            "initializationOptions": lock(&self.initialization_options).clone(),
            "trace": "off",
        });
        match self
            .request_raw("initialize", params, INITIALIZE_TIMEOUT)
            .await
        {
            Ok(answer) => Ok(answer.get("capabilities").cloned().unwrap_or(Value::Null)),
            Err(error) => Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no answer")
                .to_owned()),
        }
    }

    /// The process ended: a crash, unless it was asked to stop.
    async fn exited(&self, pid: u32, on_exit: mpsc::UnboundedSender<(Uuid, Duration)>) {
        let mut slot = self.child.lock().await;
        if self.pid() != Some(pid) {
            return;
        }
        let status = self.finish_process(&mut slot, pid).await;
        if self.is_stopping() || matches!(self.state(), State::Failed(_)) {
            return;
        }
        self.log(format!(
            "it exited{}",
            status.map(|s| format!(" ({s})")).unwrap_or_default()
        ));
        let crashes = {
            let mut crashes = lock(&self.crashes);
            crashes.retain(|at| at.elapsed() < CRASH_WINDOW);
            crashes.push(Instant::now());
            crashes.len()
        };
        if crashes > CRASHES_ALLOWED {
            self.set_state(State::Failed(format!(
                "it crashed {crashes} times in five minutes; Ahab stopped restarting it"
            )));
            return;
        }
        self.set_state(State::Starting);
        // Backoff: 1, 2, 4 seconds; the manager starts it again.
        let wait = Duration::from_secs(1 << (crashes - 1).min(4));
        let _ = on_exit.send((self.id, wait));
    }

    /// Whether it was asked to stop.
    pub fn is_stopping(&self) -> bool {
        *lock(&self.stopping)
    }

    /// Gives the server time to finish its LSP handshake before terminating
    /// the process tree. Hold the slot until the old child is reaped.
    pub async fn stop(&self) {
        self.stop_process(State::Stopped).await;
    }

    async fn stop_process(&self, next: State) {
        *lock(&self.stopping) = true;
        let mut slot = self.child.lock().await;
        if let Some(pid) = self.pid() {
            #[cfg(windows)]
            let descendants = find_descendants(pid);
            if self.state() == State::Running {
                let _ = tokio::time::timeout(Duration::from_secs(4), async {
                    let _ = self
                        .request_raw("shutdown", Value::Null, Duration::from_secs(3))
                        .await;
                    self.notify("exit", Value::Null).await;
                    if let Some(child) = slot.as_mut() {
                        let _ =
                            tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
                    }
                })
                .await;
            }
            #[cfg(windows)]
            kill_descendants(descendants);
            self.finish_process(&mut slot, pid).await;
        }
        self.set_state(next);
    }

    async fn failed_start(&self, pid: u32, reason: String) {
        let mut slot = self.child.lock().await;
        if self.pid() == Some(pid) && !self.is_stopping() {
            self.set_state(State::Failed(reason));
            self.finish_process(&mut slot, pid).await;
        }
    }

    async fn finish_process(&self, slot: &mut Option<Child>, pid: u32) -> Option<String> {
        terminate_descendants(pid);
        let mut child = slot.take()?;
        let _ = child.start_kill();
        let status = child.wait().await.ok().map(|status| status.to_string());
        *lock(&self.pid) = None;
        // Take the pipe after terminating the child so a blocked write returns.
        *self.stdin.lock().await = None;
        for (_, waiter) in lock(&self.pending).drain() {
            let _ = waiter.send(Err(
                json!({ "code": -32099, "message": "the server stopped" }),
            ));
        }
        status
    }

    /// Runs it again with the same documents (after an update, or asked).
    pub async fn prepare_restart(&self) {
        self.stop_process(State::Starting).await;
        lock(&self.crashes).clear();
        self.touch();
        *lock(&self.stopping) = false;
    }

    // --- Talking to it ------------------------------------------------------

    async fn send(&self, message: Value) -> bool {
        if self.log_traffic {
            let method = message["method"].as_str().unwrap_or("response");
            self.log(format!("→ {method} {}", short(&message)));
        }
        let mut stdin = self.stdin.lock().await;
        let Some(stdin) = stdin.as_mut() else {
            return false;
        };
        let framed = rpc::frame(&message);
        stdin.write_all(&framed).await.is_ok() && stdin.flush().await.is_ok()
    }

    pub async fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await;
    }

    /// Sends a notification from a context that cannot wait.
    fn notify_soon(&self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        if let Ok(mut stdin) = self.stdin.try_lock()
            && let Some(stdin) = stdin.as_mut()
        {
            let framed = rpc::frame(&message);
            // Small and rare (didClose): a short blocking write is fine.
            let _ = futures_util::FutureExt::now_or_never(stdin.write_all(&framed));
        }
    }

    async fn request_raw(&self, method: &str, params: Value, timeout: Duration) -> Answer {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        lock(&self.pending).insert(id, sender);
        let sent = self
            .send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        if !sent {
            lock(&self.pending).remove(&id);
            return Err(json!({ "code": -32099, "message": "the server is not running" }));
        }
        // Dropped while waiting (a window cancelled): the server stops too.
        let mut guard = Cancel {
            server: self,
            id: Some(id),
        };
        let answer = tokio::time::timeout(timeout, receiver).await;
        match answer {
            Ok(Ok(answer)) => {
                guard.id = None;
                answer
            }
            Ok(Err(_)) => {
                guard.id = None;
                Err(json!({ "code": -32099, "message": "the server stopped" }))
            }
            Err(_) => {
                Err(json!({ "code": -32803, "message": "the server did not answer in time" }))
            }
        }
    }

    /// A request, once it runs.
    pub async fn request(&self, method: &str, params: Value) -> Answer {
        if self.state() != State::Running {
            return Err(json!({ "code": -32002, "message": "the server is not ready" }));
        }
        self.touch();
        self.request_raw(method, params, REQUEST_TIMEOUT).await
    }

    /// Marks `window` as the one whose command runs next.
    pub fn commanded_by(&self, window: u64) {
        *lock(&self.commanding) = Some(window);
    }

    // --- Documents ----------------------------------------------------------

    async fn did_open(&self, path: &Path, language_id: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri_of(path), "languageId": language_id, "version": version, "text": text
            }}),
        )
        .await;
    }

    /// `window` opened `path`; the server sees it once.
    pub async fn open(&self, window: u64, path: &Path, language: &str, version: i32, text: &str) {
        let _updates = self.document_updates.lock().await;
        self.touch();
        let language_id = lsp_language_id(&self.record, language);
        let action = {
            let mut documents = lock(&self.documents);
            if let Some(document) = documents.get_mut(path) {
                document.holders.insert(window);
                if document.text == text {
                    None
                } else {
                    document.version = document.version.max(version) + 1;
                    text.clone_into(&mut document.text);
                    Some(false)
                }
            } else {
                documents.insert(
                    path.to_path_buf(),
                    Document {
                        version,
                        text: text.to_owned(),
                        language_id: language_id.clone(),
                        holders: HashSet::from([window]),
                    },
                );
                Some(true)
            }
        };
        if self.state() != State::Running {
            // Opened when it initializes.
            return;
        }
        match action {
            Some(true) => self.did_open(path, &language_id, version, text).await,
            Some(false) => {
                let version = lock(&self.documents)
                    .get(path)
                    .map_or(version, |d| d.version);
                self.notify(
                    "textDocument/didChange",
                    json!({ "textDocument": { "uri": uri_of(path), "version": version },
                            "contentChanges": [{ "text": text }] }),
                )
                .await;
            }
            None => {}
        }
    }

    /// Edits to `path`.
    pub async fn change(&self, path: &Path, version: i32, changes: &[Change]) {
        let _updates = self.document_updates.lock().await;
        self.touch();
        let full = {
            let mut documents = lock(&self.documents);
            let Some(document) = documents.get_mut(path) else {
                return;
            };
            text::apply(&mut document.text, changes);
            document.version = version.max(document.version + 1);
            (!self.incremental()).then(|| document.text.clone())
        };
        if self.state() != State::Running {
            return;
        }
        let version = lock(&self.documents)
            .get(path)
            .map_or(version, |d| d.version);
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri_of(path), "version": version },
                    "contentChanges": text::changes_value(changes, full.as_deref()) }),
        )
        .await;
    }

    /// Whether it takes changes as ranges (`TextDocumentSyncKind.Incremental`).
    fn incremental(&self) -> bool {
        let capabilities = lock(&self.capabilities);
        let sync = &capabilities["textDocumentSync"];
        let kind = sync.as_u64().or_else(|| sync["change"].as_u64());
        kind == Some(2)
    }

    pub async fn save(&self, path: &Path) {
        if self.state() != State::Running || !lock(&self.documents).contains_key(path) {
            return;
        }
        let include = lock(&self.capabilities)["textDocumentSync"]["save"]["includeText"]
            .as_bool()
            .unwrap_or(false);
        let mut params = json!({ "textDocument": { "uri": uri_of(path) } });
        if include && let Some(document) = lock(&self.documents).get(path) {
            params["text"] = Value::String(document.text.clone());
        }
        self.notify("textDocument/didSave", params).await;
    }

    pub async fn close(&self, window: u64, path: &Path) {
        let _updates = self.document_updates.lock().await;
        let closed = {
            let mut documents = lock(&self.documents);
            match documents.get_mut(path) {
                Some(document) => {
                    document.holders.remove(&window);
                    if document.holders.is_empty() {
                        documents.remove(path);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        };
        if closed && self.state() == State::Running {
            self.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri_of(path) } }),
            )
            .await;
        }
        self.touch();
    }

    /// Whether `path` is open in it.
    pub fn has_open(&self, path: &Path) -> bool {
        lock(&self.documents).contains_key(path)
    }

    pub fn document_version(&self, path: &Path) -> Option<i32> {
        lock(&self.documents).get(path).map(|d| d.version)
    }

    // --- What the server says ------------------------------------------------

    async fn receive(&self, message: Value) {
        if self.log_traffic {
            let method = message["method"].as_str().unwrap_or("response");
            self.log(format!("← {method} {}", short(&message)));
        }
        let Some(message) = rpc::classify(message) else {
            return;
        };
        match message {
            Message::Response { id, result } => {
                if let Some(waiter) = lock(&self.pending).remove(&id) {
                    let _ = waiter.send(result);
                }
            }
            Message::Notification { method, params } => self.notified(&method, params),
            Message::Request { id, method, params } => {
                let result = self.asked(&method, &params);
                let reply = match result {
                    Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                    Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
                };
                self.send(reply).await;
            }
        }
    }

    fn notified(&self, method: &str, params: Value) {
        match method {
            "textDocument/publishDiagnostics" => {
                let Some(path) = params["uri"].as_str().and_then(super::places::path_of) else {
                    return;
                };
                let items = params.get("diagnostics").cloned().unwrap_or(json!([]));
                lock(&self.diagnostics).insert(path.clone(), items.clone());
                self.tell(&Event::Diagnostics {
                    source: self.record.id.clone(),
                    path,
                    items,
                });
            }
            "window/logMessage" | "window/showMessage" => {
                let text = params["message"].as_str().unwrap_or_default();
                for line in text.lines() {
                    self.log(line.to_owned());
                }
            }
            "$/progress" => {
                let token = params["token"].to_string();
                let value = &params["value"];
                let mut progress = lock(&self.progress);
                match value["kind"].as_str() {
                    Some("begin" | "report") => {
                        let title = value["title"]
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| {
                                progress
                                    .get(&token)
                                    .map(|t| t.split(" · ").next().unwrap_or_default().to_owned())
                            })
                            .unwrap_or_default();
                        let detail = value["message"].as_str().unwrap_or_default();
                        let percent = value["percentage"]
                            .as_u64()
                            .map(|p| format!(" {p}%"))
                            .unwrap_or_default();
                        let text = if detail.is_empty() {
                            format!("{title}{percent}")
                        } else {
                            format!("{title} · {detail}{percent}")
                        };
                        progress.insert(token, text);
                    }
                    Some("end") => {
                        progress.remove(&token);
                    }
                    _ => return,
                }
                let ended = progress.is_empty();
                drop(progress);
                // Progress comes often: windows hear of it a few times a second at most.
                let mut told = lock(&self.progress_told);
                if ended || told.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                    *told = Some(Instant::now());
                    drop(told);
                    self.tell(&Event::Status);
                }
            }
            _ => {}
        }
    }

    /// What a client answers.
    fn asked(&self, method: &str, params: &Value) -> Result<Value, Value> {
        match method {
            "workspace/configuration" => {
                let settings = lock(&self.settings).clone();
                let items = params["items"].as_array().cloned().unwrap_or_default();
                Ok(Value::Array(
                    items
                        .iter()
                        .map(|item| section(&settings, item["section"].as_str()))
                        .collect(),
                ))
            }
            "workspace/workspaceFolders" => {
                let name = self
                    .root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok(json!([{ "uri": uri_of(&self.root), "name": name }]))
            }
            "workspace/applyEdit" => {
                let window = *lock(&self.commanding);
                match window {
                    Some(window) => {
                        self.tell(&Event::ApplyEdit {
                            window,
                            edit: params["edit"].clone(),
                        });
                        Ok(json!({ "applied": true }))
                    }
                    None => {
                        Ok(json!({ "applied": false, "failureReason": "no window asked for it" }))
                    }
                }
            }
            "client/registerCapability"
            | "client/unregisterCapability"
            | "window/workDoneProgress/create"
            | "window/showMessageRequest"
            | "workspace/semanticTokens/refresh"
            | "workspace/inlayHint/refresh"
            | "workspace/codeLens/refresh"
            | "workspace/diagnostic/refresh"
            | "workspace/inlineValue/refresh" => Ok(Value::Null),
            "window/showDocument" => Ok(json!({ "success": false })),
            _ => {
                Err(json!({ "code": -32601, "message": format!("Ahab does not handle {method}") }))
            }
        }
    }
}

/// A request still waiting: dropped (a window cancelled it), the server
/// is told to stop working on it.
struct Cancel<'a> {
    server: &'a Server,
    id: Option<i64>,
}

impl Drop for Cancel<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            lock(&self.server.pending).remove(&id);
            self.server
                .notify_soon("$/cancelRequest", json!({ "id": id }));
        }
    }
}

/// The part of `settings` a `workspace/configuration` item names: by
/// dotted section, the whole when it names none.
pub fn section(settings: &Value, section: Option<&str>) -> Value {
    let Some(section) = section.filter(|s| !s.is_empty()) else {
        return settings.clone();
    };
    let mut current = settings;
    for part in section.split('.') {
        match current.get(part) {
            Some(next) => current = next,
            None => return Value::Null,
        }
    }
    current.clone()
}

/// `${rootUri}`, `${rootName}` and `${root}` in settings' strings.
fn fill_variables(value: &mut Value, root_uri: &str, root_name: &str, root: &Path) {
    match value {
        Value::String(text) if text.contains("${") => {
            *text = text
                .replace("${rootUri}", root_uri)
                .replace("${rootName}", root_name)
                .replace("${root}", &root.display().to_string());
        }
        Value::Array(items) => {
            for item in items {
                fill_variables(item, root_uri, root_name, root);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                fill_variables(item, root_uri, root_name, root);
            }
        }
        _ => {}
    }
}

/// A message shortened for the log.
fn short(message: &Value) -> String {
    let text = message.to_string();
    if text.len() > 400 {
        let mut end = 400;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    } else {
        text
    }
}

/// What Ahab's editor can do, as LSP 3.17 capabilities (lsp-types' shape).
pub fn client_capabilities() -> Value {
    use lsp_types::{
        ClientCapabilities, CodeActionClientCapabilities, CodeActionKindLiteralSupport,
        CodeActionLiteralSupport, CompletionClientCapabilities, CompletionItemCapability,
        DiagnosticClientCapabilities, DocumentSymbolClientCapabilities, GeneralClientCapabilities,
        GotoCapability, HoverClientCapabilities, MarkupKind, PositionEncodingKind,
        PublishDiagnosticsClientCapabilities, RenameClientCapabilities,
        SignatureHelpClientCapabilities, TextDocumentClientCapabilities,
        TextDocumentSyncClientCapabilities, WindowClientCapabilities, WorkspaceClientCapabilities,
        WorkspaceEditClientCapabilities,
    };
    let goto = Some(GotoCapability {
        dynamic_registration: Some(false),
        link_support: Some(true),
    });
    let capabilities = ClientCapabilities {
        workspace: Some(WorkspaceClientCapabilities {
            apply_edit: Some(true),
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                document_changes: Some(false),
                ..Default::default()
            }),
            configuration: Some(true),
            workspace_folders: Some(true),
            symbol: Some(lsp_types::WorkspaceSymbolClientCapabilities::default()),
            execute_command: Some(lsp_types::DynamicRegistrationClientCapabilities::default()),
            ..Default::default()
        }),
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                did_save: Some(true),
                ..Default::default()
            }),
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    snippet_support: Some(false),
                    documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..Default::default()
                }),
                context_support: Some(true),
                ..Default::default()
            }),
            hover: Some(HoverClientCapabilities {
                content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                ..Default::default()
            }),
            signature_help: Some(SignatureHelpClientCapabilities::default()),
            references: Some(lsp_types::DynamicRegistrationClientCapabilities::default()),
            document_highlight: Some(lsp_types::DynamicRegistrationClientCapabilities::default()),
            document_symbol: Some(DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
            }),
            formatting: Some(lsp_types::DynamicRegistrationClientCapabilities::default()),
            range_formatting: Some(lsp_types::DynamicRegistrationClientCapabilities::default()),
            definition: goto,
            type_definition: goto,
            implementation: goto,
            declaration: goto,
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: [
                            "",
                            "quickfix",
                            "refactor",
                            "refactor.extract",
                            "refactor.inline",
                            "refactor.rewrite",
                            "source",
                            "source.organizeImports",
                            "source.fixAll",
                        ]
                        .iter()
                        .map(|&kind| kind.to_owned())
                        .collect(),
                    },
                }),
                is_preferred_support: Some(true),
                resolve_support: Some(lsp_types::CodeActionCapabilityResolveSupport {
                    properties: vec!["edit".to_owned()],
                }),
                data_support: Some(true),
                ..Default::default()
            }),
            rename: Some(RenameClientCapabilities {
                prepare_support: Some(true),
                ..Default::default()
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                related_information: Some(true),
                ..Default::default()
            }),
            diagnostic: Some(DiagnosticClientCapabilities {
                related_document_support: Some(false),
                ..Default::default()
            }),
            inlay_hint: Some(lsp_types::InlayHintClientCapabilities::default()),
            ..Default::default()
        }),
        window: Some(WindowClientCapabilities {
            work_done_progress: Some(true),
            ..Default::default()
        }),
        general: Some(GeneralClientCapabilities {
            position_encodings: Some(vec![PositionEncodingKind::UTF16]),
            ..Default::default()
        }),
        ..Default::default()
    };
    serde_json::to_value(capabilities).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_answers_by_section() {
        let settings =
            json!({ "rust-analyzer": { "check": { "command": "clippy" } }, "validate": "on" });
        assert_eq!(
            section(&settings, Some("rust-analyzer.check")),
            json!({ "command": "clippy" })
        );
        assert_eq!(section(&settings, Some("missing")), Value::Null);
        assert_eq!(section(&settings, Some("")), settings);
        assert_eq!(section(&settings, None), settings);
    }

    #[test]
    fn the_client_says_what_it_supports() {
        let capabilities = client_capabilities();
        assert_eq!(
            capabilities["textDocument"]["definition"]["linkSupport"],
            true
        );
        assert_eq!(capabilities["general"]["positionEncodings"][0], "utf-16");
        assert_eq!(
            capabilities["textDocument"]["documentSymbol"]["hierarchicalDocumentSymbolSupport"],
            true
        );
    }
}

#[cfg(all(test, unix))]
mod process_tests {
    use super::*;

    #[tokio::test]
    async fn stopping_the_process_group_reclaims_every_member() {
        let mut leader = Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = leader.id().unwrap();
        let mut other = Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .process_group(i32::try_from(pid).unwrap())
            .spawn()
            .unwrap();
        terminate_descendants(pid);
        for child in [&mut leader, &mut other] {
            let status = tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .unwrap()
                .unwrap();
            assert!(!status.success());
        }
    }
}
