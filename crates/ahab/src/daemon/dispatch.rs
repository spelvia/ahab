//! Answering the server's requests. Feature logic stays in the modules'
//! `daemon` files; this only routes methods to them.

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::daemon::channels::{Channels, DaemonChannel};
use crate::daemon::outbox::Outbox;
use crate::modules::machines::agents::api::{DetectRequest, UpdateAgent};
use crate::modules::machines::agents::daemon as agents;
use crate::modules::machines::metrics::daemon as metrics;
use crate::modules::machines::presence::api::WireError;
use crate::modules::machines::usage::daemon as usage;
use crate::modules::plugins::host::daemon::{
    ChannelOut, DiagnosticsWiring, PluginDiagnostics, PluginHost,
};
use crate::modules::plugins::host::model::{
    self as plugin_model, ChannelRequest, CommandRequest, PluginRef, ReceiveRequest, SyncRequest,
    decode_file,
};
use crate::modules::projects::files::api::{
    CreateParams, DeleteParams, MoveParams, PickerParams, ProjectPathParams, RenameParams,
    SyncProjects, WatchParams, WriteParams,
};
use crate::modules::projects::files::daemon::{self as files, Grants};
use crate::modules::projects::git::api::GitRequest;
use crate::modules::projects::git::daemon::{self as git, Git};
use crate::modules::projects::search::api::{
    IndexRequest, SearchFrameDto, SearchQueryDto, SearchRequest, StreamSearchRequest,
};
use crate::modules::projects::search::daemon::{self as search, Indexes};
use crate::modules::sessions::attachment::api::{AttachmentData, GetAttachment, PutAttachment};
use crate::modules::sessions::attachment::daemon::{self as attachments, Attachments};
use crate::modules::sessions::chat::api::{
    AgentHistoryRequest, CatalogRequest, ChangeMode, Control, Decide, ForkChatRequest, LiveChats,
    ResumeChat, StartChat, TaskOutputRequest,
};
use crate::modules::sessions::chat::daemon::{ChatHost, Emit};
use crate::modules::sessions::terminal::daemon::{
    Attached, Spec, TerminalHost, read_frame, write_frame,
};
use crate::modules::tools::lsp::api::{
    ConnectRequest, PinsRequest, ServerLogDto, ServerRef, ServersRequest,
};
use crate::modules::tools::lsp::daemon::LanguageServers;
use crate::modules::tools::problems::api::{CheckRequest, DiagnosticsEvent};
use crate::modules::tools::problems::daemon as checks;
use crate::modules::tools::registry::api::{InstallRequest, StatusRequest};
use crate::modules::tools::registry::daemon::install::Installer;
use crate::modules::tools::registry::daemon::{ToolHost, ToolOptions};
use crate::shared::error::DaemonError;

/// What the daemon keeps for as long as it runs, across connections: the
/// projects it may serve, its live sessions, events not yet stored, and
/// where attachments go.
pub struct DaemonState {
    pub grants: Arc<Grants>,
    pub chats: ChatHost,
    pub outbox: Arc<Outbox>,
    pub attachments: Attachments,
    pub git: Git,
    pub indexes: Indexes,
    pub channels: Channels,
    pub terminals: TerminalHost,
    /// The server's address, for terminals' `AHAB_SERVER`.
    pub server: String,
    /// The plugins this machine runs.
    pub plugins: Arc<PluginHost>,
    /// Language tools: finding and installing them.
    pub tools: Arc<ToolHost>,
    /// The language servers this machine runs.
    pub language_servers: Arc<LanguageServers>,
    /// Where GitHub releases are downloaded from.
    github: String,
}

impl DaemonState {
    pub fn new(
        data_dir: &std::path::Path,
        executable: Option<std::path::PathBuf>,
        server: &str,
        plugin_node: Option<std::path::PathBuf>,
        tools: &ToolOptions,
    ) -> Self {
        let outbox = Arc::new(Outbox::open(data_dir));
        let reporting_tools = Arc::clone(&outbox);
        let git = Git::default();
        let discovery = Arc::clone(&git.discovery);
        let tool_host = Arc::new(ToolHost::new(
            data_dir,
            tools,
            plugin_node.clone(),
            Arc::new(move |kind, data| reporting_tools.emit_live(kind, data)),
            Arc::new(move |folder| discovery.checkouts(folder)),
        ));
        let reporting = Arc::clone(&outbox);
        let plugins = PluginHost::new(
            data_dir,
            plugin_node,
            server,
            Arc::new(move |kind, data, lasting| {
                if lasting {
                    reporting.emit(kind, data);
                } else {
                    reporting.emit_live(kind, data);
                }
            }),
        );
        let grants = Arc::new(Grants::default());
        let language_servers = LanguageServers::new(Arc::clone(&tool_host));
        plugins.wire_diagnostics(plugin_diagnostics(&grants, &language_servers, &outbox));
        Self {
            language_servers,
            tools: tool_host,
            github: tools.github.clone(),
            plugins,
            grants,
            chats: ChatHost::default(),
            outbox,
            attachments: Attachments::new(data_dir),
            git,
            indexes: Indexes::default(),
            channels: Channels::default(),
            terminals: TerminalHost::new(data_dir, executable),
            server: server.to_owned(),
        }
    }

    /// Stops every agent, when the daemon stops.
    pub async fn shutdown(&self) {
        self.language_servers.shutdown().await;
        self.plugins.shutdown().await;
        self.chats.shutdown().await;
    }
}

/// Plugins' diagnostics (plugin API 0.1.3): confined to the projects this
/// machine serves, sent to the server's Problems and to the windows that
/// have the files open.
fn plugin_diagnostics(
    grants: &Arc<Grants>,
    language_servers: &Arc<LanguageServers>,
    outbox: &Arc<Outbox>,
) -> DiagnosticsWiring {
    let grants = Arc::clone(grants);
    let language_servers = Arc::clone(language_servers);
    let outbox = Arc::clone(outbox);
    DiagnosticsWiring {
        folder_of: Arc::new(move |project| grants.real_root(project).ok()),
        out: Arc::new(move |found: PluginDiagnostics| {
            language_servers.publish(
                found.project,
                &found.source,
                &found.files,
                found.replace_all,
            );
            let files = found
                .files
                .into_iter()
                .map(|(path, items)| {
                    let parsed = items
                        .into_iter()
                        .filter_map(|item| serde_json::from_value(item).ok())
                        .collect();
                    (path, parsed)
                })
                .collect();
            let event = DiagnosticsEvent {
                project: found.project,
                source: found.source,
                files,
                replace_all: found.replace_all,
                within: None,
            };
            if let Ok(data) = serde_json::to_value(&event) {
                outbox.emit_live("tools.diagnostics", data);
            }
        }),
    }
}

/// Every method [`handle`] answers: the daemon's capabilities, sent in its
/// hello. A test checks it against `handle`'s arms both ways, and that it
/// keeps every method of `BASELINE_V1`.
pub const METHODS: &[&str] = &[
    "ping",
    "agents.detect",
    "agents.updates",
    "agents.update",
    "agents.usage",
    "machine.metrics",
    "projects.sync",
    "fs.home",
    "fs.listDirectories",
    "fs.checkFolder",
    "fs.list",
    "fs.read",
    "fs.image",
    "fs.write",
    "fs.create",
    "fs.rename",
    "fs.delete",
    "fs.watch",
    "fs.move",
    "git.run",
    "git.watch",
    "search.run",
    "plugins.sync",
    "plugins.receive",
    "plugins.command",
    "plugins.channel",
    "plugins.deliver",
    "plugins.logs",
    "plugins.intercept",
    "plugins.notify",
    "search.stream",
    "files.index",
    "attachments.put",
    "attachments.get",
    "terminals.create",
    "terminals.list",
    "terminals.kill",
    "terminals.restart",
    "terminals.remove",
    "terminals.attach",
    "chats.start",
    "chats.resume",
    "chats.fork",
    "chats.commands",
    "chats.modes",
    "chats.previewCommands",
    "chats.startCommand",
    "chats.invoke",
    "chats.resumeCommand",
    "chats.taskOutput",
    "chats.stopWork",
    "chats.agentHistory",
    "chats.mode",
    "chats.send",
    "chats.interrupt",
    "chats.respond",
    "chats.decide",
    "chats.close",
    "agents.catalog",
    "chats.live",
    "lsp.connect",
    "tools.status",
    "tools.servers",
    "tools.restart",
    "tools.log",
    "tools.install",
    "tools.check",
    "tools.checkRoots",
    "tools.pins",
];

/// [`METHODS`], as the hello's `capabilities`.
pub fn capabilities() -> Vec<String> {
    METHODS.iter().map(|&method| method.to_owned()).collect()
}

/// Runs `method` with `params`; one of [`METHODS`], else `unknown_method`.
pub async fn handle(
    state: &Arc<DaemonState>,
    method: &str,
    params: Value,
) -> Result<Value, WireError> {
    match method {
        "ping" => Ok(json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") })),
        "agents.detect" => {
            let request: DetectRequest = if params.is_null() || params == json!({}) {
                DetectRequest::default()
            } else {
                parse(params)?
            };
            to_value(agents::detect(request).await)
        }
        "agents.updates" => to_value(agents::updates().await),
        "agents.update" => {
            let request: UpdateAgent = parse(params)?;
            to_value(
                agents::update(&request.agent)
                    .await
                    .map_err(WireError::from)?,
            )
        }
        "agents.usage" => to_value(usage::usage().await),
        "machine.metrics" => to_value(metrics::metrics().await),
        "projects.sync" => {
            let list: SyncProjects = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || Ok(state.grants.sync(list))).await
        }
        "fs.home" => blocking(files::home).await,
        "fs.listDirectories" => {
            let params: PickerParams = parse(params)?;
            blocking(move || files::list_directories(&params.path, params.hidden)).await
        }
        "fs.checkFolder" => {
            let params: PickerParams = parse(params)?;
            blocking(move || files::check_folder(&params.path)).await
        }
        "fs.list" => {
            let params: ProjectPathParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || state.grants.list(params.project, &params.path)).await
        }
        "fs.image" => {
            let params: ProjectPathParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || state.grants.image(params.project, &params.path)).await
        }
        "fs.read" => {
            let params: ProjectPathParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state
                    .grants
                    .read(params.project, &params.path, params.partial)
            })
            .await
        }
        "fs.write" => {
            let params: WriteParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state.grants.write(
                    params.project,
                    &params.path,
                    &params.content,
                    params.expected_version.as_deref(),
                )
            })
            .await
        }
        "fs.create" => {
            let params: CreateParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state.grants.create(
                    params.project,
                    &params.path,
                    params.kind,
                    params.content.as_deref(),
                )
            })
            .await
        }
        "fs.rename" => {
            let params: RenameParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state
                    .grants
                    .rename(params.project, &params.from, &params.to)
            })
            .await
        }
        "fs.move" => {
            let params: MoveParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state.grants.move_entry(
                    params.project,
                    &params.from,
                    &params.to,
                    params.copy,
                    params.replace,
                )
            })
            .await
        }
        "fs.delete" => {
            let params: DeleteParams = parse(params)?;
            let state = Arc::clone(state);
            blocking(move || {
                state
                    .grants
                    .delete(
                        params.project,
                        &params.path,
                        params.expected_version.as_deref(),
                    )
                    .map(|()| json!({}))
            })
            .await
        }
        "fs.watch" => {
            let params: WatchParams = parse(params)?;
            let root = state
                .grants
                .real_root(params.project)
                .map_err(WireError::from)?;
            let channel = state.channels.open(params.channel);
            tokio::spawn(watch_files(channel, root, params.paths));
            Ok(json!({}))
        }
        "git.run" => {
            let request: GitRequest = parse(params)?;
            let (folder, read_only) = state
                .grants
                .project(request.project)
                .map_err(WireError::from)?;
            state
                .git
                .run(
                    &folder,
                    request.repository.as_deref(),
                    read_only,
                    request.op,
                )
                .await
                .map_err(WireError::from)
        }
        "git.watch" => {
            let params: WatchGit = parse(params)?;
            let (folder, _) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            let places = git::places(&folder, params.repository.as_deref())
                .await
                .map_err(WireError::from)?;
            let channel = state.channels.open(params.channel);
            tokio::spawn(watch_git(channel, places));
            Ok(json!({}))
        }
        "search.run" => {
            let request: SearchRequest = parse(params)?;
            let (folder, _) = state
                .grants
                .project(request.project)
                .map_err(WireError::from)?;
            on_disk(move || search::search(&folder, &request.query).map_err(WireError::from)).await
        }
        "plugins.sync" => {
            let request: SyncRequest = parse(params)?;
            to_value(state.plugins.sync(request))
        }
        "plugins.receive" => {
            let request: ReceiveRequest = parse(params)?;
            // Registered first: the server writes as soon as it hears back.
            let channel = state.channels.open(request.channel);
            let host = Arc::clone(&state.plugins);
            tokio::spawn(receive_plugin(host, channel, request));
            Ok(json!({}))
        }
        "plugins.command" => {
            let request: CommandRequest = parse(params)?;
            let project = match request.project {
                Some(id) => {
                    let (folder, _) = state.grants.project(id).map_err(WireError::from)?;
                    Some((id, folder))
                }
                None => None,
            };
            state
                .plugins
                .command(
                    &request.plugin,
                    &request.name,
                    request.input,
                    project,
                    request.settings,
                )
                .await
                .map_err(|failure| WireError::new(&failure.code, failure.message))
        }
        "plugins.channel" => {
            let request: ChannelRequest = parse(params)?;
            // Registered first: the plugin may answer as soon as it opens.
            let channel = state.channels.open(request.channel);
            let from_plugin = state
                .plugins
                .open_channel(
                    &request.plugin,
                    &request.name,
                    request.params,
                    request.channel,
                )
                .await
                .map_err(|failure| WireError::new(&failure.code, failure.message))?;
            tokio::spawn(bridge_plugin(
                Arc::clone(&state.plugins),
                request.plugin,
                request.channel,
                channel,
                from_plugin,
            ));
            Ok(json!({}))
        }
        "plugins.intercept" => {
            // Only the hooks the server asks about.
            let hook = params["hook"].as_str().unwrap_or_default();
            if hook != "notifications.send" {
                return Err(WireError::new(
                    "invalid_params",
                    format!("{hook} is not asked about by the server"),
                ));
            }
            let only: Vec<String> = params["plugins"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            match state
                .plugins
                .before_only(hook, params["data"].clone(), Some(&only))
                .await
            {
                Ok((data, changed_by)) => {
                    Ok(json!({ "action": "allow", "data": data, "changedBy": changed_by }))
                }
                Err((plugin, reason)) => {
                    Ok(json!({ "action": "deny", "plugin": plugin, "reason": reason }))
                }
            }
        }
        "plugins.notify" => {
            let plugin = params["plugin"].as_str().unwrap_or_default().to_owned();
            let destination = params["destination"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            state
                .plugins
                .notify(
                    &plugin,
                    &destination,
                    params["notification"].clone(),
                    params["settings"].clone(),
                )
                .await
                .map_err(|failure| WireError::new(&failure.code, failure.message))
        }
        "plugins.deliver" => {
            let name = params["name"].as_str().unwrap_or_default().to_owned();
            state.plugins.deliver(&name, &params["data"]);
            Ok(json!({}))
        }
        "plugins.logs" => {
            let request: PluginRef = parse(params)?;
            to_value(state.plugins.logs(&request.plugin))
        }
        "search.stream" => {
            let request: StreamSearchRequest = parse(params)?;
            let (folder, _) = state
                .grants
                .project(request.project)
                .map_err(WireError::from)?;
            // A pattern that does not parse is this request's error, not
            // the stream's.
            search::check(&folder, &request.query).map_err(WireError::from)?;
            let channel = state.channels.open(request.channel);
            tokio::spawn(stream_search(channel, folder, request.query));
            Ok(json!({}))
        }
        "files.index" => {
            let request: IndexRequest = parse(params)?;
            let (folder, _) = state
                .grants
                .project(request.project)
                .map_err(WireError::from)?;
            let state = Arc::clone(state);
            on_disk(move || Ok(state.indexes.get(&folder))).await
        }
        "attachments.put" => {
            let params: PutAttachment = parse(params)?;
            let state = Arc::clone(state);
            on_disk(move || state.attachments.put(&params).map_err(WireError::from)).await
        }
        "attachments.get" => {
            use base64::Engine as _;
            let params: GetAttachment = parse(params)?;
            let state = Arc::clone(state);
            on_disk(move || {
                let bytes = state.attachments.get(&params).map_err(WireError::from)?;
                Ok(AttachmentData {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                })
            })
            .await
        }
        "terminals.create" => {
            let params: CreateTerminal = parse(params)?;
            let (cwd, _) = state.grants.project(params.project)?;
            let env = [
                ("AHAB_PROJECT", params.project.to_string()),
                ("AHAB_SESSION", params.session.to_string()),
                ("AHAB_SERVER", state.server.clone()),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
            let (spec, hooked_by) =
                intercept_terminal(state, cwd, params.command, env, params.context).await?;
            state
                .terminals
                .create(params.terminal, spec, params.cols, params.rows)
                .await?;
            Ok(serde_json::json!({ "hookedBy": hooked_by }))
        }
        "terminals.list" => {
            let terminals = state.terminals.list().await?;
            Ok(serde_json::json!({ "terminals": terminals }))
        }
        "terminals.kill" | "terminals.restart" | "terminals.remove" => {
            let params: TerminalRef = parse(params)?;
            match method {
                "terminals.kill" => state.terminals.kill(params.terminal).await?,
                "terminals.restart" => state.terminals.restart(params.terminal).await?,
                _ => state.terminals.remove(params.terminal).await?,
            }
            Ok(serde_json::json!({}))
        }
        "terminals.attach" => {
            let params: AttachTerminal = parse(params)?;
            // Registered first: the server may write before it hears back.
            let channel = state.channels.open(params.channel);
            let stream = state.terminals.attach(params.terminal).await?;
            tokio::spawn(bridge(channel, stream));
            Ok(serde_json::json!({}))
        }
        "chats.previewCommands" => {
            #[derive(serde::Deserialize)]
            struct Preview {
                project: uuid::Uuid,
                #[serde(flatten)]
                catalog: CatalogRequest,
            }
            let params: Preview = parse(params)?;
            let (cwd, read_only) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            to_value(
                state
                    .chats
                    .preview_commands(
                        &params.catalog.agent,
                        cwd,
                        read_only,
                        params.catalog.added.as_ref(),
                    )
                    .await
                    .map_err(WireError::from)?,
            )
        }
        "chats.start" | "chats.startCommand" => {
            let params: StartChat = parse(params)?;
            let message = message(state, &params.message, params.attachments.clone()).await?;
            let (cwd, read_only) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            let emit = chat_emit(state);
            let started = state
                .chats
                .start(
                    params.chat,
                    &params.agent,
                    cwd,
                    read_only,
                    message,
                    params.command.as_ref(),
                    params.model,
                    params.effort,
                    params.mode,
                    params.fast,
                    // Attached files are read without asking.
                    vec![state.attachments.readable_root()],
                    params.added.as_ref(),
                    emit,
                )
                .await
                .map_err(WireError::from)?;
            to_value(started)
        }
        "chats.fork" => {
            let params: ForkChatRequest = parse(params)?;
            let (cwd, _) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            let native = state
                .chats
                .fork(&params.agent, &params.native_id, params.turn, &cwd)
                .await
                .map_err(WireError::from)?;
            to_value(native)
        }
        "chats.stopWork" => {
            let params: crate::modules::sessions::chat::api::StopWorkRequest = parse(params)?;
            to_value(
                state
                    .chats
                    .stop_work(params.chat, params.target)
                    .await
                    .map_err(WireError::from)?,
            )
        }
        "chats.taskOutput" => {
            let params: TaskOutputRequest = parse(params)?;
            let page = state
                .chats
                .task_output(
                    params.chat,
                    &params.agent,
                    params.native_id.as_deref(),
                    &params.task,
                    stubb::observation::task::OutputQuery {
                        limit: params.limit,
                        cursor: params.cursor,
                    },
                )
                .await
                .map_err(WireError::from)?;
            to_value(page)
        }
        "chats.agentHistory" => {
            let params: AgentHistoryRequest = parse(params)?;
            let events = state
                .chats
                .agent_history(
                    params.chat,
                    &params.agent,
                    params.native_id.as_deref(),
                    &params.subagent,
                )
                .await
                .map_err(WireError::from)?;
            to_value(events)
        }
        "chats.resume" | "chats.resumeCommand" => {
            let params: ResumeChat = parse(params)?;
            let message = message(state, &params.message, params.attachments.clone()).await?;
            let (cwd, read_only) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            let started = state
                .chats
                .resume(
                    params.chat,
                    &params.agent,
                    &params.native_id,
                    cwd,
                    read_only,
                    message,
                    params.model,
                    params.effort,
                    params.mode,
                    params.fast,
                    vec![state.attachments.readable_root()],
                    params.after,
                    params.command.as_ref(),
                    params.added.as_ref(),
                    chat_emit(state),
                )
                .await
                .map_err(WireError::from)?;
            to_value(started)
        }
        "chats.mode" => {
            let params: ChangeMode = parse(params)?;
            let (cwd, read_only) = state
                .grants
                .project(params.project)
                .map_err(WireError::from)?;
            state
                .chats
                .set_mode(params.chat, &cwd, read_only, &params.mode)
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "chats.modes" => {
            let control: Control = parse(params)?;
            to_value(state.chats.modes(control.chat).map_err(WireError::from)?)
        }
        "chats.commands" => {
            let control: Control = parse(params)?;
            to_value(
                state
                    .chats
                    .commands(control.chat)
                    .await
                    .map_err(WireError::from)?,
            )
        }
        "chats.send" | "chats.invoke" => {
            let control: Control = parse(params)?;
            let text = control.text.unwrap_or_default();
            let input = message(state, &text, control.attachments).await?;
            state
                .chats
                .send(control.chat, input, control.command.as_ref())
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "chats.interrupt" => {
            let control: Control = parse(params)?;
            state
                .chats
                .interrupt(control.chat, control.turn)
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "chats.respond" => {
            let control: Control = parse(params)?;
            let (Some(request), Some(decision)) = (control.request, control.decision) else {
                return Err(WireError::new(
                    "invalid_params",
                    "A request and a decision are needed.",
                ));
            };
            state
                .chats
                .respond(control.chat, &request, decision)
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "chats.decide" => {
            let decide: Decide = parse(params)?;
            state
                .chats
                .decide(decide.chat, &decide.request, decide.decision)
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "chats.close" => {
            let control: Control = parse(params)?;
            state
                .chats
                .close(control.chat)
                .await
                .map_err(WireError::from)?;
            Ok(json!({}))
        }
        "agents.catalog" => {
            let params: CatalogRequest = parse(params)?;
            to_value(
                state
                    .chats
                    .catalog(&params.agent, params.added.as_ref())
                    .await
                    .map_err(WireError::from)?,
            )
        }
        "chats.live" => to_value(LiveChats {
            chats: state.chats.live_ids(),
        }),
        "lsp.connect" => {
            let request: ConnectRequest = parse(params)?;
            let folder = state
                .grants
                .real_root(request.project)
                .map_err(WireError::from)?;
            // Registered first: the window may write as soon as it hears back.
            let channel = state.channels.open(request.channel);
            let outbox = Arc::clone(&state.outbox);
            let report = Arc::new(move |event: DiagnosticsEvent| {
                if let Ok(data) = serde_json::to_value(&event) {
                    outbox.emit_live("tools.diagnostics", data);
                }
            });
            if let Some(pinned) = request.pinned {
                state.language_servers.set_pins(request.project, pinned);
            }
            let (to_servers, from_window) = tokio::sync::mpsc::channel(256);
            let (to_window, from_servers) = tokio::sync::mpsc::channel(256);
            tokio::spawn(bridge_window(channel, to_servers, from_servers));
            tokio::spawn(Arc::clone(&state.language_servers).serve(
                from_window,
                to_window,
                request.project,
                folder,
                request.context,
                report,
            ));
            Ok(json!({}))
        }
        "tools.status" => {
            let rescan = params["rescan"].as_bool().unwrap_or(false);
            let request: StatusRequest = parse(params)?;
            if rescan {
                state.tools.forget().await;
            }
            to_value(state.tools.status(&request.context).await)
        }
        "tools.servers" => {
            let request: ServersRequest = if params.is_null() {
                ServersRequest::default()
            } else {
                parse(params)?
            };
            if request.memory_budget.is_some() {
                state.language_servers.set_budget(request.memory_budget);
            }
            to_value(state.language_servers.list())
        }
        "tools.pins" => {
            let request: PinsRequest = parse(params)?;
            state
                .language_servers
                .set_pins(request.project, request.pinned);
            Ok(json!({}))
        }
        "tools.restart" => {
            let request: ServerRef = parse(params)?;
            if state.language_servers.restart(request.server).await {
                Ok(json!({}))
            } else {
                Err(WireError::new(
                    "server_not_found",
                    "No such language server runs here.",
                ))
            }
        }
        "tools.log" => {
            let request: ServerRef = parse(params)?;
            match state.language_servers.log(request.server) {
                Some(lines) => to_value(ServerLogDto { lines }),
                None => Err(WireError::new(
                    "server_not_found",
                    "No such language server runs here.",
                )),
            }
        }
        "tools.install" => {
            let request: InstallRequest = parse(params)?;
            install(state, request)
        }
        "tools.check" => {
            let request: CheckRequest = parse(params)?;
            let folder = state
                .grants
                .real_root(request.project)
                .map_err(WireError::from)?;
            let scope = checks::Scope {
                paths: request.paths,
                checkouts: request.checkouts,
            };
            // Plugins that check on demand check the same repositories,
            // alongside; not when only some checks were named.
            let found = state.tools.checkouts(&folder).await;
            let roots = checks::scoped_checkouts(&found, &scope);
            let plugins = async {
                if request.tools.is_none() {
                    state
                        .plugins
                        .check(request.project, &folder, &roots, &request.trigger)
                        .await;
                }
            };
            let (answer, ()) = tokio::join!(
                checks::check(
                    &state.tools,
                    &request.context,
                    request.project,
                    &folder,
                    &request.trigger,
                    request.tools.as_deref(),
                    scope.clone(),
                ),
                plugins
            );
            to_value(answer)
        }
        "tools.checkRoots" => {
            let request: CheckRequest = parse(params)?;
            let folder = state
                .grants
                .real_root(request.project)
                .map_err(WireError::from)?;
            to_value(checks::roots(&state.tools, &request.context, &folder, &request.trigger).await)
        }
        other => Err(WireError::new(
            "unknown_method",
            format!("This daemon has no method {other}."),
        )),
    }
}

/// Starts installing, updating or removing a tool; its progress goes to the
/// server as `tools.job` events.
fn install(state: &Arc<DaemonState>, request: InstallRequest) -> Result<Value, WireError> {
    let tool = request.record.id.clone();
    if !state.tools.installs.begin(&tool, &request.action) {
        return Err(WireError::new(
            "tool_busy",
            format!("{tool} is being installed or removed already."),
        ));
    }
    let job = state
        .tools
        .installs
        .jobs()
        .into_iter()
        .find(|job| job.tool == tool);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let host = Arc::clone(&state.tools);
        if request.action == "remove" {
            state.language_servers.replace_tool(&tool).await;
        }
        let vars = host.environments.machine().await;
        let report = Arc::clone(&host.report);
        let progress = move |job: &crate::modules::tools::registry::api::ToolJobDto| {
            if let Ok(data) = serde_json::to_value(job) {
                report("tools.job", data);
            }
        };
        let installer = Installer {
            record: &request.record,
            vars: &vars,
            plugin_node: host.plugin_node.as_deref(),
            github: &state.github,
            progress: &progress,
        };
        let result = host.installs.run(&request.action, &installer).await;
        if result.is_ok() && request.action != "remove" {
            // Running servers start again on the new version.
            state.language_servers.replace_tool(&tool).await;
        }
        host.forget().await;
    });
    to_value(job)
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, WireError> {
    serde_json::from_value(params)
        .map_err(|error| WireError::new("invalid_params", error.to_string()))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, WireError> {
    serde_json::to_value(value).map_err(|error| WireError::new("internal", error.to_string()))
}

/// Runs file work off the async threads.
async fn blocking<T, F>(work: F) -> Result<Value, WireError>
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce() -> Result<T, DaemonError> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(value)) => to_value(value),
        Ok(Err(error)) => Err(error.into()),
        Err(error) => Err(WireError::new("internal", error.to_string())),
    }
}

/// Runs file work off the async threads; its errors are already wire errors.
async fn on_disk<T, F>(work: F) -> Result<Value, WireError>
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce() -> Result<T, WireError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| WireError::new("internal", error.to_string()))?
        .and_then(to_value)
}

/// A message for an agent: the text, with its attachments resolved.
async fn message(
    state: &Arc<DaemonState>,
    text: &str,
    ids: Vec<uuid::Uuid>,
) -> Result<stubb::turn::Input, WireError> {
    if ids.is_empty() {
        return Ok(stubb::turn::Input::text(text));
    }
    let state = Arc::clone(state);
    let files = tokio::task::spawn_blocking(move || state.attachments.resolve(&ids))
        .await
        .map_err(|error| WireError::new("internal", error.to_string()))?
        .map_err(WireError::from)?;
    Ok(attachments::input(text, &files))
}

/// Sends a chat's events to the server: kept ones through the outbox.
fn chat_emit(state: &Arc<DaemonState>) -> Emit {
    let outbox = Arc::clone(&state.outbox);
    Arc::new(move |data, lasting| {
        if lasting {
            outbox.emit("chat.event", data);
        } else {
            outbox.emit_live("chat.event", data);
        }
    })
}

#[derive(serde::Deserialize)]
struct CreateTerminal {
    terminal: uuid::Uuid,
    project: uuid::Uuid,
    session: uuid::Uuid,
    command: Option<String>,
    cols: u16,
    rows: u16,
    /// What it is for, by plugin; the `terminals.create` hook reads it.
    #[serde(default)]
    context: Option<Value>,
}

#[derive(serde::Deserialize)]
struct TerminalRef {
    terminal: uuid::Uuid,
}

#[derive(serde::Deserialize)]
struct WatchGit {
    project: uuid::Uuid,
    repository: Option<String>,
    channel: uuid::Uuid,
}

#[derive(serde::Deserialize)]
struct AttachTerminal {
    terminal: uuid::Uuid,
    channel: uuid::Uuid,
}

/// Carries an attached terminal both ways: its frames to the server as
/// channel payloads (the kind, then the bytes), and the server's input and
/// resizes to the terminal host; until either side ends.
async fn bridge(channel: DaemonChannel, stream: Attached) {
    let (mut from_server, to_server) = channel.split();
    let (mut from_host, mut to_host) = stream.into_split();
    let up = to_server.clone();
    let outward = tokio::spawn(async move {
        while let Ok(Some((kind, payload))) = read_frame(&mut from_host).await {
            let mut message = Vec::with_capacity(payload.len() + 1);
            message.push(kind);
            message.extend_from_slice(&payload);
            if !up.send(&message).await {
                break;
            }
        }
        up.close();
    });
    while let Some(message) = from_server.recv().await {
        let Some((&kind, payload)) = message.split_first() else {
            continue;
        };
        if write_frame(&mut to_host, kind, payload).await.is_err() {
            break;
        }
    }
    outward.abort();
    to_server.close();
}

/// Carries a window's language channel both ways, until either side ends.
async fn bridge_window(
    channel: DaemonChannel,
    to_servers: tokio::sync::mpsc::Sender<Vec<u8>>,
    mut from_servers: tokio::sync::mpsc::Receiver<Vec<u8>>,
) {
    let (mut from_server, to_server) = channel.split();
    loop {
        tokio::select! {
            frame = from_server.recv() => {
                let Some(frame) = frame else { break };
                if to_servers.send(frame).await.is_err() {
                    break;
                }
            }
            frame = from_servers.recv() => {
                let Some(frame) = frame else { break };
                if !to_server.send(&frame).await {
                    break;
                }
            }
        }
    }
    to_server.close();
}

/// Files sent together, at most this often or this many at once: few
/// frames for a big search, and the first results without waiting.
const SEARCH_BATCH: std::time::Duration = std::time::Duration::from_millis(100);
const SEARCH_BATCH_FILES: usize = 50;

/// Runs a search, sending its results on `channel` as they are found, then
/// how it ended; stops once the server closes the channel (the search is no
/// longer wanted).
async fn stream_search(channel: DaemonChannel, folder: std::path::PathBuf, query: SearchQueryDto) {
    let (reader, writer) = channel.split();
    let handle = tokio::runtime::Handle::current();
    let send = writer.clone();
    let worked = tokio::task::spawn_blocking(move || {
        let push = |frame: &SearchFrameDto| {
            let Ok(bytes) = serde_json::to_vec(frame) else {
                return false;
            };
            handle.block_on(send.send(&bytes))
        };
        let mut batch = Vec::new();
        let mut since = std::time::Instant::now();
        let mut open = true;
        let ended = search::search_each(&folder, &query, |file| {
            batch.push(file);
            if batch.len() >= SEARCH_BATCH_FILES || since.elapsed() >= SEARCH_BATCH {
                open = push(&SearchFrameDto::Files {
                    files: std::mem::take(&mut batch),
                });
                since = std::time::Instant::now();
            }
            open
        });
        if !open {
            return;
        }
        if !batch.is_empty() && !push(&SearchFrameDto::Files { files: batch }) {
            return;
        }
        let (total, cut) = ended.unwrap_or((0, None));
        push(&SearchFrameDto::Done { total, cut });
    })
    .await;
    if let Err(error) = worked {
        tracing::warn!(%error, "a streamed search failed");
    }
    writer.close();
    drop(reader);
}

/// Watches a checkout, telling its changes on `channel` until it closes.
/// Tells a file watch's frames on its channel until the server closes it.
async fn watch_files(channel: DaemonChannel, root: std::path::PathBuf, paths: Vec<String>) {
    let (tell, mut told) = tokio::sync::mpsc::channel(64);
    let watching = tokio::spawn(files::watch::watch(tell, root, paths));
    let (mut from_server, to_server) = channel.split();
    loop {
        tokio::select! {
            frame = told.recv() => {
                let Some(frame) = frame else { break };
                let Ok(bytes) = serde_json::to_vec(&frame) else { break };
                if !to_server.send(&bytes).await {
                    break;
                }
            }
            // The server sends nothing; `None` is the channel closing.
            message = from_server.recv() => if message.is_none() { break },
        }
    }
    drop(told);
    let _ = watching.await;
    to_server.close();
}

async fn watch_git(channel: DaemonChannel, places: git::Places) {
    let (tell, mut told) = tokio::sync::mpsc::channel(16);
    let watching = tokio::spawn(git::watch::watch(tell, places));
    let (mut from_server, to_server) = channel.split();
    loop {
        tokio::select! {
            frame = told.recv() => {
                let Some(frame) = frame else { break };
                let Ok(bytes) = serde_json::to_vec(&frame) else { break };
                if !to_server.send(&bytes).await {
                    break;
                }
            }
            // The server sends nothing; `None` is the channel closing.
            message = from_server.recv() => if message.is_none() { break },
        }
    }
    // Nobody listens any more: the watch ends.
    drop(told);
    let _ = watching.await;
    to_server.close();
}

/// Takes a plugin's build from the server, a file per frame, and stores it
/// once the server closes the channel.
async fn receive_plugin(host: Arc<PluginHost>, channel: DaemonChannel, request: ReceiveRequest) {
    let (mut from_server, _to_server) = channel.split();
    let mut files = Vec::new();
    while let Some(frame) = from_server.recv().await {
        let Some(file) = decode_file(&frame) else {
            host.log(&request.id, "a build arrived damaged");
            return;
        };
        files.push(file);
    }
    if let Err(error) = host.receive(&request.id, &request.digest, files).await {
        host.log(&request.id, format!("cannot store the build: {error}"));
    }
}

/// Carries a plugin's channel both ways: the server's bytes to the plugin,
/// the plugin's to the server (marked data or end), until either closes.
async fn bridge_plugin(
    host: Arc<PluginHost>,
    plugin: String,
    id: uuid::Uuid,
    channel: DaemonChannel,
    mut from_plugin: tokio::sync::mpsc::Receiver<ChannelOut>,
) {
    let (mut from_server, to_server) = channel.split();
    loop {
        tokio::select! {
            bytes = from_server.recv() => {
                let Some(bytes) = bytes else {
                    host.close_channel(&plugin, id);
                    break;
                };
                host.write_channel(&plugin, id, bytes);
            }
            out = from_plugin.recv() => {
                match out {
                    Some(ChannelOut::Data(bytes)) => {
                        if !to_server.send(&[&[plugin_model::DATA][..], &bytes].concat()).await {
                            host.close_channel(&plugin, id);
                            break;
                        }
                    }
                    Some(ChannelOut::End(reason)) => {
                        let reason = reason.unwrap_or_default();
                        let _ = to_server.send(&[&[plugin_model::END][..], reason.as_bytes()].concat()).await;
                        break;
                    }
                    None => break,
                }
            }
        }
    }
    to_server.close();
}

/// A terminal about to start, as plugins' `terminals.create` hooks leave it:
/// its spec, and the plugins that changed it; refused when one
/// says no. Ahab's own variables stay as Ahab set them.
async fn intercept_terminal(
    state: &Arc<DaemonState>,
    cwd: std::path::PathBuf,
    command: Option<String>,
    env: std::collections::HashMap<String, String>,
    context: Option<Value>,
) -> Result<(Spec, Vec<String>), WireError> {
    let data = json!({
        "command": command,
        "env": {},
        "folder": cwd,
        "context": context.unwrap_or_else(|| json!({})),
    });
    let (data, hooked_by) = state
        .plugins
        .before("terminals.create", data)
        .await
        .map_err(|(plugin, reason)| {
            WireError::new(
                "terminal_denied",
                format!("The plugin {plugin} refused: {reason}"),
            )
        })?;
    let mut env = env;
    if let Some(added) = data["env"].as_object() {
        for (key, value) in added {
            if let Some(value) = value.as_str()
                && !key.starts_with("AHAB_")
            {
                env.insert(key.clone(), value.to_owned());
            }
        }
    }
    let folder = data["folder"]
        .as_str()
        .map(std::path::PathBuf::from)
        .filter(|folder| folder.is_dir())
        .unwrap_or(cwd);
    let spec = Spec {
        cwd: folder,
        command: data["command"].as_str().map(str::to_owned),
        env,
    };
    Ok((spec, hooked_by))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::modules::machines::presence::api::BASELINE_V1;

    /// The method names `handle` matches on, read from its source: the
    /// string literals that start a match arm (`"a" =>`, `"a" | "b" =>`).
    fn handled() -> BTreeSet<String> {
        let source = include_str!("dispatch.rs");
        let start = source.find("pub async fn handle(").unwrap();
        let end = start + source[start..].find("\n}\n").unwrap();
        let mut methods = BTreeSet::new();
        for line in source[start..end].lines() {
            let line = line.trim();
            let Some(pattern) = line
                .strip_suffix("=> {")
                .or_else(|| line.split_once(" => ").map(|(pattern, _)| pattern))
            else {
                continue;
            };
            let pattern = pattern.trim();
            if !pattern.starts_with('"') {
                continue;
            }
            for name in pattern.split('|') {
                methods.insert(name.trim().trim_matches('"').to_owned());
            }
        }
        methods
    }

    #[test]
    fn lists_exactly_the_methods_it_handles() {
        let listed: BTreeSet<String> = METHODS.iter().map(|&m| m.to_owned()).collect();
        assert_eq!(listed.len(), METHODS.len(), "a method is listed twice");
        let handled = handled();
        assert!(
            handled.len() > 30,
            "the source scan found too little: {handled:?}"
        );
        assert_eq!(
            listed.difference(&handled).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "listed in METHODS but not handled"
        );
        assert_eq!(
            handled.difference(&listed).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "handled but missing from METHODS"
        );
    }

    #[test]
    fn keeps_every_protocol_1_method() {
        let missing: Vec<&&str> = BASELINE_V1
            .iter()
            .filter(|method| !METHODS.contains(method))
            .collect();
        assert!(
            missing.is_empty(),
            "protocol-1 methods dropped: {missing:?}"
        );
    }
}
