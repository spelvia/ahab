//! The daemon's side of sessions: running agents through Stubb in a
//! project's folder, and forwarding their events to the server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::modules::machines::Harness;
use serde_json::{Value, json};
use stubb::Provider;
use stubb::process::Environment;
use stubb::session::{Event, Settings, Spec};
use uuid::Uuid;

use crate::modules::machines::harness;
use crate::modules::sessions::chat::api::{AddedLaunch, ChatEvent, DecisionDto, Started};
use crate::modules::sessions::chat::model::{permissions, settings};
use crate::shared::error::{DaemonError, fail};
use crate::shared::sync::lock;

/// Sends one session event to the server; `true` for events that are kept.
pub type Emit = Arc<dyn Fn(Value, bool) + Send + Sync>;

fn stubb_error(error: &stubb::Error) -> DaemonError {
    fail("agent_error", error.to_string())
}

/// An observation read's error: `unsupported`, said as `what`, when the agent
/// cannot; `not_found` for an unknown task or subagent.
fn observation_error(error: &stubb::Error, what: &str) -> DaemonError {
    match error {
        stubb::Error::Unsupported { .. } => fail("unsupported", what),
        stubb::Error::AgentNotFound { .. } | stubb::Error::TaskNotFound { .. } => {
            fail("not_found", error.to_string())
        }
        _ => stubb_error(error),
    }
}

const PROVIDER_WARM: Duration = Duration::from_mins(5);

struct CachedProvider {
    provider: Arc<dyn Provider>,
    unused_since: Option<Instant>,
}

struct Live {
    prompt_echo: Arc<Mutex<Option<(String, String)>>>,
    prompt_connections: Arc<crate::integrations::mcp_prompts::Connections>,
    session: stubb::Session,
    // This lease prevents retiring the provider while it owns this chat.
    _provider: Arc<dyn Provider>,
    /// Stubb's own harness; `None` for an ACP agent.
    harness: Option<Harness>,
    /// The running turn, to interrupt it.
    turn: Arc<Mutex<Option<String>>>,
}

/// Every live session on this machine.
#[derive(Default)]
pub struct ChatHost {
    providers: tokio::sync::Mutex<HashMap<Harness, CachedProvider>>,
    /// ACP agents' providers, by agent and how they start.
    acp: tokio::sync::Mutex<
        HashMap<
            String,
            (
                stubb::providers::acp::Launch,
                Arc<stubb::providers::acp::AcpProvider>,
            ),
        >,
    >,
    live: Arc<Mutex<HashMap<Uuid, Live>>>,
}

/// The host's environment, with the agent's own folder first on `PATH`, so
/// an agent found through the login shell (fnm, nvm) finds its runtime.
fn environment(executable: &Path) -> Environment {
    let mut paths: Vec<PathBuf> = executable
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    let environment = Environment::inherit();
    match std::env::join_paths(paths) {
        Ok(joined) => environment.set("PATH", joined),
        Err(_) => environment,
    }
}

impl ChatHost {
    fn live(&self) -> MutexGuard<'_, HashMap<Uuid, Live>> {
        lock(&self.live)
    }

    /// The provider for `harness`, made on first use with the executable the
    /// probe finds.
    async fn provider(&self, harness: Harness) -> Result<Arc<dyn Provider>, DaemonError> {
        let mut providers = self.providers.lock().await;
        if let Some(cached) = providers.get_mut(&harness) {
            cached.unused_since = None;
            return Ok(Arc::clone(&cached.provider));
        }
        let report = stubb::discovery::probe(harness.cli(), None).await;
        let Some(executable) = report.path else {
            return Err(fail(
                "agent_not_installed",
                format!("{} is not installed on this machine.", harness.program()),
            ));
        };
        let environment = environment(&executable);
        let provider: Arc<dyn Provider> = match harness {
            Harness::Claude => Arc::new(stubb::providers::claude::Claude::new(
                stubb::providers::claude::ClaudeConfig {
                    executable,
                    environment,
                    ..stubb::providers::claude::ClaudeConfig::default()
                },
            )),
            Harness::Codex => Arc::new(stubb::providers::codex::Codex::new(
                stubb::providers::codex::CodexConfig {
                    executable,
                    environment,
                    ..stubb::providers::codex::CodexConfig::default()
                },
            )),
            Harness::Pi => Arc::new(stubb::providers::pi::Pi::new(
                stubb::providers::pi::PiConfig {
                    executable,
                    environment,
                    ..stubb::providers::pi::PiConfig::default()
                },
            )),
        };
        providers.insert(
            harness,
            CachedProvider {
                provider: Arc::clone(&provider),
                unused_since: None,
            },
        );
        Ok(provider)
    }

    /// The provider for an added (ACP) agent, made again when how it starts
    /// changed.
    async fn acp_provider(
        &self,
        added: &AddedLaunch,
    ) -> Result<Arc<stubb::providers::acp::AcpProvider>, DaemonError> {
        let launch = launch_of(added).await?;
        let mut providers = self.acp.lock().await;
        if let Some((known, provider)) = providers.get(&added.agent)
            && *known == launch
        {
            return Ok(Arc::clone(provider));
        }
        let provider = Arc::new(stubb::providers::acp::AcpProvider::new(
            stubb::providers::acp::AcpConfig::new(
                added.agent.clone(),
                added.name.clone(),
                launch.clone(),
            ),
        ));
        providers.insert(added.agent.clone(), (launch, Arc::clone(&provider)));
        Ok(provider)
    }

    /// The provider for `agent`: Stubb's own harness, or the added agent's.
    async fn provider_for(
        &self,
        agent: &str,
        added: Option<&AddedLaunch>,
    ) -> Result<(Arc<dyn Provider>, Option<Harness>), DaemonError> {
        if let Some(harness) = harness(agent) {
            return Ok((self.provider(harness).await?, Some(harness)));
        }
        let added =
            added.ok_or_else(|| fail("agent_unknown", format!("Ahab cannot run {agent}.")))?;
        let provider: Arc<dyn Provider> = self.acp_provider(added).await?;
        Ok((provider, None))
    }

    /// The models `agent` offers here, with their reasoning efforts.
    pub async fn catalog(
        &self,
        agent: &str,
        added: Option<&AddedLaunch>,
    ) -> Result<stubb::Catalog, DaemonError> {
        let (provider, _) = self.provider_for(agent, added).await?;
        provider
            .catalog()
            .await
            .map_err(|error| stubb_error(&error))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the session's whole start, from the server"
    )]
    pub async fn start(
        &self,
        id: Uuid,
        agent: &str,
        cwd: PathBuf,
        read_only: bool,
        message: stubb::turn::Input,
        command: Option<&super::api::CommandInvocation>,
        model: Option<String>,
        effort: Option<String>,
        mode: Option<String>,
        fast: Option<bool>,
        readable: Vec<PathBuf>,
        added: Option<&AddedLaunch>,
        emit: Emit,
    ) -> Result<Started, DaemonError> {
        let (provider, harness) = self.provider_for(agent, added).await?;
        let session = provider
            .start_session(Spec {
                settings: Settings {
                    model,
                    effort,
                    fast_mode: fast,
                    additional_directories: readable,
                    ..settings_for(harness, &cwd, read_only, mode.as_deref())
                },
                cwd,
            })
            .await
            .map_err(|error| stubb_error(&error))?;
        let started = self.attach(id, harness, provider, &session, 0, emit);
        if let Err(error) = self.send(id, message, command).await {
            let _ = session.close().await;
            return Err(error);
        }
        Ok(started)
    }

    /// Keeps a running agent session as chat `id` and forwards its events,
    /// numbering the kept ones after `after`.
    fn attach(
        &self,
        id: Uuid,
        harness: Option<Harness>,
        provider: Arc<dyn Provider>,
        session: &stubb::Session,
        after: i64,
        emit: Emit,
    ) -> Started {
        let mut events = session.subscribe();
        let turn = Arc::new(Mutex::new(None::<String>));
        let started = Started {
            native_id: session.id().to_owned(),
            model: Some(session.info().model.clone()).filter(|model| !model.is_empty()),
            effort: session.info().effort.clone(),
            unrestricted: harness == Some(Harness::Pi),
            mode_unavailable: None,
            mode: None,
        };
        let prompt_echo = Arc::new(Mutex::new(None));
        self.live().insert(
            id,
            Live {
                prompt_echo: Arc::clone(&prompt_echo),
                prompt_connections: Arc::default(),
                session: session.clone(),
                _provider: provider,
                harness,
                turn: Arc::clone(&turn),
            },
        );

        observe(id, session, Arc::clone(&emit));

        // Forward every event, numbering the kept ones.
        let forward_turn = Arc::clone(&turn);
        let live = Arc::clone(&self.live);
        tokio::spawn(async move {
            let mut seq: i64 = after;
            loop {
                let mut event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(session = %id, missed, "a session's events came too fast; some were skipped");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                if let Event::ItemStart { item, .. } | Event::ItemEnd { item, .. } = &mut event
                    && let stubb::turn::Item::UserMessage { content, .. } = item
                    && let Some((wire, display)) = lock(&prompt_echo).as_ref()
                {
                    for part in content {
                        if let stubb::turn::UserContent::Text { text } = part
                            && text == wire
                        {
                            text.clone_from(display);
                        }
                    }
                }
                match &event {
                    Event::TurnStart { turn_id } => {
                        *lock(&forward_turn) = Some(turn_id.clone());
                    }
                    Event::TurnEnd { .. } => {
                        *lock(&prompt_echo) = None;
                        *lock(&forward_turn) = None;
                    }
                    _ => {}
                }
                let closed = matches!(event, Event::Closed { .. });
                // Streamed text and provider traffic Stubb does not map are
                // only shown live, never kept.
                let lasting = !matches!(event, Event::ItemDelta { .. } | Event::Raw { .. });
                let Ok(value) = serde_json::to_value(&event) else {
                    continue;
                };
                let numbered = lasting.then(|| {
                    seq += 1;
                    seq
                });
                let data = ChatEvent {
                    chat: id,
                    seq: numbered,
                    event: value,
                };
                if let Ok(data) = serde_json::to_value(data) {
                    emit(data, lasting);
                }
                if closed {
                    break;
                }
            }
            // The agent ended it, or its process exited.
            let mut live = lock(&live);
            if live
                .get(&id)
                .is_some_and(|entry| Arc::ptr_eq(&entry.turn, &forward_turn))
            {
                live.remove(&id);
            }
        });

        started
    }

    /// A page of a background task's output: from the live session while the
    /// chat runs, else from the agent's stored session `native`.
    pub async fn task_output(
        &self,
        chat: Uuid,
        agent: &str,
        native: Option<&str>,
        task: &str,
        query: stubb::observation::task::OutputQuery,
    ) -> Result<stubb::observation::task::OutputPage, DaemonError> {
        let live = self.live().get(&chat).map(|live| live.session.clone());
        if let Some(session) = live {
            return session.task_output(task, query).await.map_err(|error| {
                observation_error(&error, "This agent cannot show a task's output.")
            });
        }
        let (Some(harness), Some(native)) = (harness(agent), native) else {
            return Err(fail(
                "unsupported",
                "This agent cannot show a task's output.",
            ));
        };
        self.provider(harness)
            .await?
            .task_output(native, task, query)
            .await
            .map_err(|error| observation_error(&error, "This agent cannot show a task's output."))
    }

    /// A subagent's conversation so far, as chat events, oldest first: what
    /// the agent's stored session `native` keeps of it (at most `MAX_TURNS`).
    pub async fn agent_history(
        &self,
        chat: Uuid,
        agent: &str,
        native: Option<&str>,
        subagent: &str,
    ) -> Result<Vec<Value>, DaemonError> {
        const MAX_TURNS: usize = 500;
        let unsupported = "This agent cannot show its subagents' conversations.";
        let native = self
            .live()
            .get(&chat)
            .map(|live| live.session.id().to_owned())
            .or_else(|| native.map(str::to_owned))
            .ok_or_else(|| fail("unsupported", unsupported))?;
        let harness = harness(agent).ok_or_else(|| fail("unsupported", unsupported))?;
        let provider = self.provider(harness).await?;
        let mut turns = Vec::new();
        let mut cursor = None;
        loop {
            let page = provider
                .list_agent_turns(
                    &native,
                    subagent,
                    stubb::turn::Query {
                        limit: Some(100),
                        cursor,
                    },
                )
                .await
                .map_err(|error| observation_error(&error, unsupported))?;
            turns.extend(page.turns.into_iter().map(|owned| owned.turn));
            cursor = page.next_cursor;
            if cursor.is_none() || turns.len() >= MAX_TURNS {
                break;
            }
        }
        Ok(turns
            .into_iter()
            .flat_map(turn_events)
            .filter_map(|event| serde_json::to_value(event).ok())
            .collect())
    }

    pub async fn fork(
        &self,
        agent: &str,
        native: &str,
        turn: String,
        cwd: &std::path::Path,
    ) -> Result<String, DaemonError> {
        let harness = harness(agent)
            .ok_or_else(|| fail("fork_unsupported", "This agent cannot fork conversations."))?;
        let provider = self.provider(harness).await?;
        let source = provider
            .read_session(native)
            .await
            .map_err(|error| stubb_error(&error))?;
        if source.cwd != cwd {
            return Err(fail(
                "project_mismatch",
                "The conversation belongs to another project.",
            ));
        }
        let fork = provider
            .fork_session(
                native,
                stubb::session::Fork {
                    at_turn: Some(turn),
                },
            )
            .await
            .map_err(|error| stubb_error(&error))?;
        Ok(fork.id)
    }

    /// Continues a chat whose agent stopped: loads the agent's stored
    /// session (`native_id`) and sends `message`. Its events are numbered
    /// after `after`, the last the server kept.
    #[expect(
        clippy::too_many_arguments,
        reason = "the chat's whole resume, from the server"
    )]
    pub async fn resume(
        &self,
        id: Uuid,
        agent: &str,
        native_id: &str,
        cwd: PathBuf,
        read_only: bool,
        message: stubb::turn::Input,
        model: Option<String>,
        effort: Option<String>,
        mode: Option<String>,
        fast: Option<bool>,
        readable: Vec<PathBuf>,
        after: i64,
        command: Option<&super::api::CommandInvocation>,
        added: Option<&AddedLaunch>,
        emit: Emit,
    ) -> Result<Started, DaemonError> {
        if self.live().contains_key(&id) {
            return Err(fail("chat_live", "This chat is already running."));
        }
        let settings = |harness: Option<Harness>| Settings {
            model: model.clone(),
            effort: effort.clone(),
            fast_mode: fast,
            additional_directories: readable.clone(),
            ..settings_for(harness, &cwd, read_only, mode.as_deref())
        };
        let (session, harness, provider): (_, _, Arc<dyn Provider>) =
            if let Some(harness) = harness(agent) {
                let provider = self.provider(harness).await?;
                let session = provider
                    .resume_session(native_id, settings(Some(harness)))
                    .await
                    .map_err(|error| stubb_error(&error))?;
                (session, Some(harness), provider)
            } else {
                let added = added
                    .ok_or_else(|| fail("agent_unknown", format!("Ahab cannot run {agent}.")))?;
                // ACP agents may not list their sessions: resume by ID, in the
                // project's folder.
                let provider = self.acp_provider(added).await?;
                let session = provider
                    .resume_session_in(native_id, &cwd, settings(None))
                    .await
                    .map_err(|error| stubb_error(&error))?;
                (session, None, provider)
            };
        // An ACP agent's modes are its own: one it no longer offers is no
        // reason to fail the resume. It carries on in its current mode, the
        // chat says so, and the server stores that mode.
        let mut after = after;
        let mut unavailable = None;
        if harness.is_none()
            && !read_only
            && let Some(mode) = mode.as_deref()
            && let Err(error) = session.set_mode(mode).await
        {
            let current = session.modes().ok().map(|modes| modes.current);
            tracing::warn!(
                chat = %id,
                agent,
                mode,
                current = current.as_deref().unwrap_or("unknown"),
                %error,
                "the agent rejected the chat's saved mode; it continues in its current one"
            );
            after += 1;
            if let Ok(data) = serde_json::to_value(ChatEvent {
                chat: id,
                seq: Some(after),
                event: mode_unavailable(mode, current.as_deref()),
            }) {
                emit(data, true);
            }
            unavailable = Some((mode.to_owned(), current));
        }
        let mut started = self.attach(id, harness, provider, &session, after, emit);
        if let Some((saved, current)) = unavailable {
            started.mode_unavailable = Some(saved);
            started.mode = current;
        }
        if let Err(error) = self.send(id, message, command).await {
            let _ = session.close().await;
            return Err(error);
        }
        Ok(started)
    }

    fn session(&self, id: Uuid) -> Result<(stubb::Session, Option<String>), DaemonError> {
        let live = self.live();
        let entry = live.get(&id).ok_or_else(|| {
            fail(
                "chat_not_live",
                "This session is not running on this machine.",
            )
        })?;
        let turn = lock(&entry.turn).clone();
        Ok((entry.session.clone(), turn))
    }

    /// Changes what a live chat's agent may do without asking.
    pub async fn set_mode(
        &self,
        id: Uuid,
        cwd: &Path,
        read_only: bool,
        mode: &str,
    ) -> Result<(), DaemonError> {
        let (session, harness) = {
            let live = self.live();
            let entry = live.get(&id).ok_or_else(|| {
                fail(
                    "chat_not_live",
                    "This session is not running on this machine.",
                )
            })?;
            (entry.session.clone(), entry.harness)
        };
        let Some(harness) = harness else {
            // An ACP agent's modes are its own, by name.
            return session
                .set_mode(mode)
                .await
                .map_err(|error| stubb_error(&error));
        };
        let Some(configuration) = permissions(harness, cwd, read_only, Some(mode)) else {
            return Err(fail(
                "invalid_params",
                "This agent has no permission modes.",
            ));
        };
        session
            .set_permissions(configuration)
            .await
            .map_err(|error| stubb_error(&error))
    }

    pub fn modes(&self, id: Uuid) -> Result<stubb::session::Modes, DaemonError> {
        self.session(id)?
            .0
            .modes()
            .map_err(|error| observation_error(&error, "Named session modes are unavailable."))
    }

    pub async fn commands(&self, id: Uuid) -> Result<Vec<super::api::AgentCommand>, DaemonError> {
        let (session, _) = self.session(id)?;
        let connections = self.prompt_connections(id)?;
        Self::catalog_for_session(&session, &connections).await
    }

    pub async fn preview_commands(
        &self,
        agent: &str,
        cwd: PathBuf,
        read_only: bool,
        added: Option<&AddedLaunch>,
    ) -> Result<Vec<super::api::AgentCommand>, DaemonError> {
        let (provider, harness) = self.provider_for(agent, added).await?;
        let settings = settings_for(harness, &cwd, read_only, None);
        let session = provider
            .start_session(Spec { cwd, settings })
            .await
            .map_err(|error| stubb_error(&error))?;
        let result = Self::catalog_for_session(
            &session,
            &crate::integrations::mcp_prompts::Connections::default(),
        )
        .await;
        let _ = session.close().await;
        result
    }

    async fn catalog_for_session(
        session: &stubb::Session,
        connections: &crate::integrations::mcp_prompts::Connections,
    ) -> Result<Vec<super::api::AgentCommand>, DaemonError> {
        let skills = match session.skills().await {
            Ok(skills) => skills,
            Err(stubb::Error::Unsupported { .. }) => Vec::new(),
            Err(error) => return Err(stubb_error(&error)),
        };
        let mut items: Vec<_> = skills
            .into_iter()
            .map(|s| super::api::AgentCommand {
                id: s.id,
                name: s.name,
                description: s.description,
                kind: super::api::CommandKind::Skill,
                server: None,
                arguments: None,
            })
            .collect();
        let native = match session.mcp_prompts().await {
            Ok(entries) => entries,
            Err(stubb::Error::Unsupported { .. }) => Vec::new(),
            Err(e) => return Err(stubb_error(&e)),
        };
        let host = connections.list(&session.info().cwd).await?;
        for (owned, prompt) in native
            .into_iter()
            .map(|p| (false, p))
            .chain(host.into_iter().map(|p| (true, p)))
        {
            items.push(super::api::AgentCommand {
                id: json!([owned, prompt.server, prompt.name]).to_string(),
                name: prompt.name,
                server: Some(prompt.server),
                description: prompt.description,
                kind: super::api::CommandKind::McpPrompt,
                arguments: prompt.arguments.map(|args| {
                    args.into_iter()
                        .map(|a| super::api::PromptArgument {
                            name: a.name,
                            description: a.description,
                            required: a.required,
                        })
                        .collect()
                }),
            });
        }
        Ok(items)
    }

    fn prompt_connections(
        &self,
        id: Uuid,
    ) -> Result<Arc<crate::integrations::mcp_prompts::Connections>, DaemonError> {
        self.live()
            .get(&id)
            .map(|l| Arc::clone(&l.prompt_connections))
            .ok_or_else(|| fail("chat_not_live", "This chat has ended."))
    }

    pub async fn send(
        &self,
        id: Uuid,
        mut input: stubb::turn::Input,
        command: Option<&super::api::CommandInvocation>,
    ) -> Result<(), DaemonError> {
        let (session, _) = self.session(id)?;
        let result = match command {
            None => session.send(input).await,
            Some(cmd) if cmd.kind == super::api::CommandKind::Skill => {
                session.invoke_skill(&cmd.id, input).await
            }
            Some(cmd) => {
                let (owned, server, name): (bool, String, String) =
                    serde_json::from_str(&cmd.id)
                        .map_err(|_| fail("command_unavailable", "Invalid MCP prompt ID."))?;
                let echo = self
                    .live()
                    .get(&id)
                    .map(|entry| Arc::clone(&entry.prompt_echo))
                    .ok_or_else(|| fail("chat_not_live", "This chat has ended."))?;
                let mut display = input.text.clone();
                for (key, value) in &cmd.arguments {
                    display.push_str(&format!("\n{key} = {value}"));
                }
                if owned {
                    let expanded = self
                        .prompt_connections(id)?
                        .expand(&session.info().cwd, &server, &name, &cmd.arguments)
                        .await?;
                    let instructions = input
                        .text
                        .split_once(char::is_whitespace)
                        .map_or("", |(_, rest)| rest);
                    input.text =
                        format!("Use MCP prompt {server}/{name}.\n{expanded}\n{instructions}");
                    *lock(&echo) = Some((input.text.clone(), display));
                    session.send(input).await
                } else {
                    if !cmd.arguments.is_empty() {
                        return Err(fail(
                            "command_unavailable",
                            "This provider accepts positional prompt arguments.",
                        ));
                    }
                    input.text = input
                        .text
                        .split_once(char::is_whitespace)
                        .map_or("", |(_, args)| args)
                        .to_owned();
                    *lock(&echo) = Some((
                        format!("/mcp__{server}__{name} {}", input.text)
                            .trim_end()
                            .to_owned(),
                        display,
                    ));
                    session.invoke_mcp_prompt(&server, &name, input).await
                }
            }
        };
        result.map(|_| ()).map_err(|error| match error {
            stubb::Error::InvalidSetting { .. } | stubb::Error::Unsupported { .. }
                if command.is_some() =>
            {
                fail("command_unavailable", error.to_string())
            }
            _ => stubb_error(&error),
        })
    }

    pub async fn stop_work(
        &self,
        id: Uuid,
        target: super::api::ChatStopTarget,
    ) -> Result<stubb::observation::control::Receipt, DaemonError> {
        let (session, _) = self.session(id)?;
        let snapshot = session
            .snapshot()
            .await
            .map_err(|error| observation_error(&error, "Observation unavailable."))?;
        let support = match &target {
            super::api::ChatStopTarget::Agent { .. } => snapshot.capabilities.stop_agent.support,
            super::api::ChatStopTarget::Task { .. } => snapshot.capabilities.stop_task.support,
        };
        if !matches!(support, stubb::observation::Support::Supported) {
            return Err(fail(
                "unsupported",
                "The agent does not offer this stop action.",
            ));
        }
        match target {
            super::api::ChatStopTarget::Agent { id }
                if id != snapshot.root_agent_id
                    && snapshot.agents.iter().any(|agent| agent.id == id) =>
            {
                session.stop_agent(&id).await
            }
            super::api::ChatStopTarget::Task { id }
                if snapshot.tasks.iter().any(|task| task.id == id) =>
            {
                session.stop_task(&id).await
            }
            _ => {
                return Err(fail(
                    "not_found",
                    "This chat no longer contains the selected background work.",
                ));
            }
        }
        .map_err(|error| observation_error(&error, "Stopping this work is unavailable."))
    }

    pub async fn interrupt(&self, id: Uuid, turn: Option<String>) -> Result<(), DaemonError> {
        let (session, running) = self.session(id)?;
        let Some(turn) = turn.or(running) else {
            return Ok(());
        };
        session
            .interrupt(&turn)
            .await
            .map_err(|error| stubb_error(&error))
    }

    /// `chats.respond`: an answer as servers without `chats.decide` send it.
    pub async fn respond(
        &self,
        id: Uuid,
        request: &str,
        decision: DecisionDto,
    ) -> Result<(), DaemonError> {
        let decision = match decision {
            DecisionDto::Allow => stubb::request::Decision::Allow,
            DecisionDto::AllowForSession => stubb::request::Decision::AllowForSession,
            DecisionDto::Deny => stubb::request::Decision::Deny { message: None },
            DecisionDto::Cancel => stubb::request::Decision::Cancel,
            DecisionDto::Answer | DecisionDto::Select | DecisionDto::Submit => {
                return Err(fail("invalid_params", "This decision needs chats.decide."));
            }
        };
        self.decide(id, request, decision).await
    }

    /// Answers a pending request. A decision that does not answer it is
    /// `invalid_decision` and leaves it pending; one no longer pending is
    /// `request_not_found`.
    pub async fn decide(
        &self,
        id: Uuid,
        request: &str,
        decision: stubb::request::Decision,
    ) -> Result<(), DaemonError> {
        let (session, _) = self.session(id)?;
        session
            .respond(request, decision)
            .await
            .map_err(|error| match error {
                stubb::Error::InvalidDecision { reason, .. } => fail("invalid_decision", reason),
                stubb::Error::RequestNotFound { .. } => {
                    fail("request_not_found", error.to_string())
                }
                _ => stubb_error(&error),
            })
    }

    pub async fn close(&self, id: Uuid) -> Result<(), DaemonError> {
        let entry = self.live().remove(&id);
        if let Some(entry) = entry {
            entry
                .session
                .close()
                .await
                .map_err(|error| stubb_error(&error))?;
        }
        Ok(())
    }

    /// The sessions running here.
    pub fn live_ids(&self) -> Vec<Uuid> {
        self.live().keys().copied().collect()
    }

    /// Retires cached native providers whose last chat and API user are gone.
    pub async fn retire_unused_providers(&self) {
        let retiring = {
            let mut providers = self.providers.lock().await;
            let now = Instant::now();
            let mut unused = Vec::new();
            for (&harness, cached) in &mut *providers {
                if Arc::strong_count(&cached.provider) > 1 {
                    cached.unused_since = None;
                } else if now.duration_since(*cached.unused_since.get_or_insert(now))
                    >= PROVIDER_WARM
                {
                    unused.push(harness);
                }
            }
            unused
                .into_iter()
                .filter_map(|harness| providers.remove(&harness))
                .collect::<Vec<_>>()
        };
        for cached in retiring {
            cached.provider.shutdown().await;
        }
    }

    /// Closes every session and stops the agents, when the daemon stops.
    pub async fn shutdown(&self) {
        let ids = self.live_ids();
        for id in ids {
            let _ = self.close(id).await;
        }
        let providers = std::mem::take(&mut *self.providers.lock().await);
        for cached in providers.into_values() {
            cached.provider.shutdown().await;
        }
    }
}

/// The kept chat event telling that the agent did not take the chat's saved
/// `mode` when it resumed, and runs in `current` (if it says) instead.
fn mode_unavailable(mode: &str, current: Option<&str>) -> Value {
    json!({ "type": "modeUnavailable", "mode": mode, "current": current })
}

/// Settings for a session: Stubb's own harnesses as the model says; ACP
/// agents ask before acting, and their requests come to Ahab.
/// How long records gather before a new snapshot is sent: a busy agent's
/// subagents and tasks are told a few times a second at most.
const OBSERVE_SETTLE: std::time::Duration = std::time::Duration::from_millis(300);

/// Follows a chat's subagents and background tasks (Stubb's observation),
/// where its agent tells them: after any change, the whole picture (every
/// agent and task, with its state) goes to the server as a live-only
/// `observation` event; never kept, the latest replaces the last.
fn observe(id: Uuid, session: &stubb::Session, emit: Emit) {
    // Subscribed before the first snapshot, as Stubb asks.
    let Ok(mut records) = session.subscribe_records() else {
        return;
    };
    let session = session.clone();
    tokio::spawn(async move {
        let send = |event: Value| {
            if let Ok(data) = serde_json::to_value(ChatEvent {
                chat: id,
                seq: None,
                event,
            }) {
                emit(data, false);
            }
        };
        let tell = |snapshot: &stubb::observation::Snapshot| {
            send(json!({
                "type": "observation",
                "rootAgentId": snapshot.root_agent_id,
                "canStopAgent": matches!(snapshot.capabilities.stop_agent.support, stubb::observation::Support::Supported),
                "canStopTask": matches!(snapshot.capabilities.stop_task.support, stubb::observation::Support::Supported),
                "agents": snapshot.agents,
                "tasks": snapshot.tasks,
            }));
        };
        // A subagent's own activity (its turns, items, streamed text), shown
        // live in its view; the chat's own agent's comes on the session's events.
        let forward = |record: &stubb::observation::Record, root: &str| {
            let (Some(agent), stubb::observation::Event::Activity { event, .. }) =
                (&record.agent_id, &record.event)
            else {
                return;
            };
            if agent == root {
                return;
            }
            if let Ok(event) = serde_json::to_value(event) {
                send(json!({ "type": "agentActivity", "agentId": agent, "event": event }));
            }
        };
        let mut root = String::new();
        if let Ok(snapshot) = session.snapshot().await {
            root.clone_from(&snapshot.root_agent_id);
            tell(&snapshot);
        }
        loop {
            match records.recv().await {
                Ok(record) => forward(&record, &root),
                // Lagging behind: the next snapshot catches up anyway.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
            // Settle before the next snapshot, forwarding activity meanwhile.
            let settled = tokio::time::Instant::now() + OBSERVE_SETTLE;
            loop {
                match tokio::time::timeout_at(settled, records.recv()).await {
                    Ok(Ok(record)) => forward(&record, &root),
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => return,
                    Err(_) => break,
                }
            }
            match session.snapshot().await {
                Ok(snapshot) => {
                    root.clone_from(&snapshot.root_agent_id);
                    tell(&snapshot);
                }
                Err(error) => {
                    tracing::debug!(chat = %id, %error, "no snapshot of the chat's agents and tasks");
                    return;
                }
            }
        }
    });
}

/// A stored turn as the events that would have shown it live: its start,
/// each finished item, its end. Subagents' histories show like chats.
fn turn_events(turn: stubb::turn::Turn) -> Vec<Event> {
    let mut events = vec![Event::TurnStart {
        turn_id: turn.id.clone(),
    }];
    events.extend(turn.items.iter().cloned().map(|item| Event::ItemEnd {
        turn_id: turn.id.clone(),
        item,
    }));
    if turn.status != stubb::turn::Status::InProgress {
        events.push(Event::TurnEnd { turn });
    }
    events
}

fn settings_for(
    harness: Option<Harness>,
    cwd: &Path,
    read_only: bool,
    mode: Option<&str>,
) -> Settings {
    match harness {
        Some(harness) => settings(harness, cwd, read_only, mode),
        None => Settings {
            forward_requests: true,
            ..Settings::default()
        },
    }
}

/// How an added agent starts: its custom command, or its registry entry
/// resolved for this computer (npx, uvx).
async fn launch_of(added: &AddedLaunch) -> Result<stubb::providers::acp::Launch, DaemonError> {
    if let Some(custom) = &added.launch {
        let mut launch =
            stubb::providers::acp::Launch::new(&custom.program).args(custom.args.clone());
        for (name, value) in &custom.env {
            launch = launch.env(name, value);
        }
        return Ok(launch);
    }
    let entry = added.entry.clone().ok_or_else(|| {
        fail(
            "agent_unknown",
            format!("Ahab knows no way to start {}.", added.name),
        )
    })?;
    let entry: stubb::providers::acp::registry::Entry =
        serde_json::from_value(entry).map_err(|error| {
            fail(
                "agent_unknown",
                format!("{}'s registry entry is unreadable: {error}", added.name),
            )
        })?;
    entry
        .resolve(None)
        .await
        .map_err(|error| fail("agent_not_installed", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_provider_is_retired_only_after_its_last_user_and_warm_period() {
        let host = ChatHost::default();
        let provider: Arc<dyn Provider> = Arc::new(stubb::providers::pi::Pi::new(
            stubb::providers::pi::PiConfig::default(),
        ));
        host.providers.lock().await.insert(
            Harness::Pi,
            CachedProvider {
                provider: Arc::clone(&provider),
                unused_since: Some(Instant::now().checked_sub(PROVIDER_WARM).unwrap()),
            },
        );
        host.retire_unused_providers().await;
        assert!(
            host.providers.lock().await[&Harness::Pi]
                .unused_since
                .is_none()
        );
        drop(provider);
        host.retire_unused_providers().await;
        let mut providers = host.providers.lock().await;
        let cached = providers.get_mut(&Harness::Pi).unwrap();
        assert!(cached.unused_since.is_some());
        cached.unused_since = Some(Instant::now().checked_sub(PROVIDER_WARM).unwrap());
        drop(providers);
        host.retire_unused_providers().await;
        assert!(host.providers.lock().await.is_empty());
    }

    #[test]
    fn a_stored_turn_shows_as_its_live_events() {
        let turn: stubb::turn::Turn = serde_json::from_value(json!({
            "id": "t1",
            "status": "completed",
            "items": [{"type": "agentMessage", "id": "m1", "text": "Done."}],
            "startedAt": null,
            "completedAt": null,
            "durationMs": null,
            "error": null,
        }))
        .unwrap();
        let types: Vec<Value> = turn_events(turn)
            .into_iter()
            .map(|event| serde_json::to_value(event).unwrap()["type"].clone())
            .collect();
        assert_eq!(
            types,
            vec![json!("turnStart"), json!("itemEnd"), json!("turnEnd")]
        );

        let running: stubb::turn::Turn = serde_json::from_value(json!({
            "id": "t2", "status": "inProgress", "items": [],
            "startedAt": null, "completedAt": null, "durationMs": null, "error": null,
        }))
        .unwrap();
        assert_eq!(
            turn_events(running).len(),
            1,
            "a running turn has not ended"
        );
    }

    #[test]
    fn a_rejected_saved_mode_is_told_with_the_agents_own() {
        assert_eq!(
            mode_unavailable("retired", Some("ask")),
            json!({ "type": "modeUnavailable", "mode": "retired", "current": "ask" })
        );
        assert_eq!(mode_unavailable("retired", None)["current"], Value::Null);
        let started: Started = serde_json::from_value(
            json!({ "nativeId": "n", "model": null, "modeUnavailable": "retired", "mode": "ask" }),
        )
        .unwrap();
        assert_eq!(started.mode_unavailable.as_deref(), Some("retired"));
        assert_eq!(started.mode.as_deref(), Some("ask"));
    }

    #[test]
    fn each_agent_gets_its_permission_rules() {
        let cwd = Path::new("/work/app");
        assert!(settings(Harness::Claude, cwd, false, None).forward_requests);
        let codex = settings(Harness::Codex, cwd, false, None);
        assert!(codex.forward_requests, "Codex asks through Ahab");
        let Some(Ok(Some(permissions))) = codex
            .permissions
            .map(|p| p.get::<stubb::providers::codex::Permissions>())
        else {
            panic!("codex has its sandbox");
        };
        assert_eq!(
            permissions.approval_policy,
            Some(stubb::providers::codex::ApprovalPolicy::OnRequest),
            "it asks to go beyond its sandbox"
        );
        assert!(matches!(
            permissions.sandbox,
            Some(stubb::providers::codex::Sandbox::WorkspaceWrite {
                network_access: Some(false),
                ..
            })
        ));
        let Some(Ok(Some(read_only))) = settings(Harness::Codex, cwd, true, Some("fullAccess"))
            .permissions
            .map(|p| p.get::<stubb::providers::codex::Permissions>())
        else {
            panic!("codex has its sandbox");
        };
        assert_eq!(
            read_only.sandbox,
            Some(stubb::providers::codex::Sandbox::ReadOnly)
        );
        assert_eq!(
            read_only.approval_policy,
            Some(stubb::providers::codex::ApprovalPolicy::Never),
            "a read-only project never asks to write"
        );
        assert_eq!(settings(Harness::Pi, cwd, false, None), Settings::default());
    }

    #[test]
    fn modes_become_native_settings() {
        use stubb::providers::claude::PermissionMode;
        let cwd = Path::new("/work/app");
        let claude = |mode, read_only| match permissions(Harness::Claude, cwd, read_only, mode) {
            Some(p) => p
                .get::<stubb::providers::claude::Permissions>()
                .unwrap()
                .and_then(|p| p.mode),
            None => None,
        };
        assert_eq!(claude(None, false), Some(PermissionMode::Default));
        assert_eq!(claude(Some("ask"), false), Some(PermissionMode::Default));
        assert_eq!(claude(Some("plan"), false), Some(PermissionMode::Plan));
        assert_eq!(
            claude(Some("acceptEdits"), false),
            Some(PermissionMode::AcceptEdits)
        );
        assert_eq!(
            claude(Some("bypass"), false),
            Some(PermissionMode::BypassPermissions)
        );
        assert_eq!(
            claude(Some("bypass"), true),
            Some(PermissionMode::Plan),
            "read-only plans"
        );
        let codex = |mode| match permissions(Harness::Codex, cwd, false, mode) {
            Some(p) => p
                .get::<stubb::providers::codex::Permissions>()
                .unwrap()
                .and_then(|p| p.sandbox),
            None => None,
        };
        assert_eq!(
            codex(Some("readOnly")),
            Some(stubb::providers::codex::Sandbox::ReadOnly)
        );
        assert_eq!(
            codex(Some("fullAccess")),
            Some(stubb::providers::codex::Sandbox::DangerFullAccess)
        );
        let policy = |mode| {
            permissions(Harness::Codex, cwd, false, mode)
                .unwrap()
                .get::<stubb::providers::codex::Permissions>()
                .unwrap()
                .and_then(|p| p.approval_policy)
        };
        assert_eq!(
            policy(Some("ask")),
            Some(stubb::providers::codex::ApprovalPolicy::Untrusted),
            "ask asks"
        );
        assert_eq!(
            policy(Some("readOnly")),
            Some(stubb::providers::codex::ApprovalPolicy::OnRequest)
        );
        assert_eq!(
            policy(Some("workspace")),
            Some(stubb::providers::codex::ApprovalPolicy::OnRequest)
        );
        assert_eq!(
            policy(Some("fullAccess")),
            Some(stubb::providers::codex::ApprovalPolicy::Never)
        );
        assert!(matches!(
            codex(Some("ask")),
            Some(stubb::providers::codex::Sandbox::WorkspaceWrite { .. })
        ));
        assert!(permissions(Harness::Pi, cwd, false, Some("ask")).is_none());
    }

    #[test]
    fn the_agent_folder_comes_first_on_path() {
        let environment = environment(Path::new("/opt/node/bin/codex"));
        assert!(environment.inherits());
    }
}
