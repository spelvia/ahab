//! What can be done with sessions, and keeping the server's record of them
//! in step with their agents' events.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::modules::machines::{
    AGENTS, AgentService, AgentState, MachineEvent, Presence, agent_label,
};
use crate::modules::projects::{Availability, FileService, Project, ProjectMode, ProjectService};
use crate::modules::sessions::attachment::model::check_count;
use crate::modules::sessions::chat::api::{
    AnswerDto, ChatEvent, Decide, DecisionDto, DeliveryDto, LiveChats, MessageDto, ModelDto,
    SessionSearchHitDto, SessionSearchMatchDto, Started, TaskOutputDto,
};
use crate::modules::sessions::chat::model::{
    self, Chat, ChatState, ChatStatus, Submission, SubmissionState, Transition,
};
use crate::modules::sessions::chat::store::{ChatRepository, ModelCacheRepository, StoredEvent};
use crate::modules::sessions::reference::api::{ReferenceDto, ReferenceKindDto};
use crate::modules::sessions::reference::model as reference;
use crate::modules::sessions::session::service::SessionService;
use crate::shared::error::{FieldError, RepoError, ServiceError};
use crate::shared::sync::lock;
use crate::shared::time::Clock;

/// Starting an agent can take a while (it loads, then reads the folder).
const START_TIMEOUT: Duration = Duration::from_secs(90);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// An event as clients watching live receive it: kept ones carry their
/// number; streamed text has none.
#[derive(Debug, Clone)]
pub struct LiveEvent {
    pub chat: Uuid,
    pub seq: Option<i64>,
    pub event: Value,
}

/// A chat that changed in a way worth telling, as it is now.
#[derive(Debug, Clone)]
pub struct ChatTransition {
    pub chat: Chat,
    pub transition: Transition,
}

pub struct ChatService {
    repo: Arc<dyn ChatRepository>,
    projects: Arc<ProjectService>,
    sessions: Arc<SessionService>,
    agents: Arc<AgentService>,
    presence: Arc<Presence>,
    /// For whether context sent with a message went stale.
    files: Arc<FileService>,
    clock: Arc<dyn Clock>,
    /// Shared, not copied, by every listener (only one chat's matter to each).
    live: broadcast::Sender<Arc<LiveEvent>>,
    /// Finished, needs you, failed, for whoever notifies.
    transitions: broadcast::Sender<Arc<ChatTransition>>,
    /// Each agent's models on each machine, as last read.
    model_cache: Arc<dyn ModelCacheRepository>,
    /// Model lists being read again in the background.
    refreshing: Arc<Mutex<HashSet<(Uuid, String)>>>,
    /// Each chat's machine, as events arrive: a chat never moves, so text
    /// streamed as it is typed is checked without the database.
    machines: Mutex<HashMap<Uuid, Uuid>>,
    /// A live chat's model and effort for its next message, chosen while it
    /// ran with others; applied by resuming its agent's session with them.
    /// Kept in memory: a restart forgets the choice, never the chat.
    next: Mutex<HashMap<Uuid, NextModel>>,
    /// Chats being switched now: their agent's `closed` is kept as
    /// `modelChanged`, and tells no one it stopped.
    switching: Mutex<HashMap<Uuid, NextModel>>,
    /// Each live chat's latest picture of its subagents and background tasks
    /// (Stubb's observation, from its daemon): never kept, sent to whoever
    /// watches the chat, and forgotten when the chat stops.
    observations: Mutex<HashMap<Uuid, Value>>,
    /// Submissions given to each chat's agent, to tag the user messages it
    /// echoes with their IDs (see [`model::Echoes`]).
    echoes: Mutex<HashMap<Uuid, model::Echoes>>,
    queue_updates: tokio::sync::Mutex<()>,
    /// Chats whose turn ended with messages waiting: the background sends
    /// the next one, outside the loop that applies daemons' events.
    drain: mpsc::UnboundedSender<Uuid>,
    drained: Mutex<Option<mpsc::UnboundedReceiver<Uuid>>>,
}

/// A model and effort a chat goes on with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextModel {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Fast mode; `None` keeps what the chat has.
    pub fast: Option<bool>,
}

/// How long a switch waits for the agent to stop before resuming it.
const SWITCH_WAIT: Duration = Duration::from_secs(15);

/// How long a model list is shown before it is read again (in the background).
const MODELS_FRESH: Duration = Duration::from_hours(6);

/// Asks the agent on `machine` for its models (Stubb's catalog) and keeps
/// them.
async fn read_models(
    presence: &Presence,
    agents: &AgentService,
    cache: &Arc<dyn ModelCacheRepository>,
    clock: &Arc<dyn Clock>,
    machine: Uuid,
    agent: &str,
) -> Result<(Value, OffsetDateTime), ServiceError> {
    let added = agents.launch_for(machine, agent).await?;
    let answer = presence
        .request(
            machine,
            "agents.catalog",
            json!({ "agent": agent, "added": added }),
            START_TIMEOUT,
        )
        .await?;
    let now = clock.now();
    cache.put(machine, agent, &answer, now).await?;
    Ok((answer, now))
}

/// What the daemon's answers to task output and subagent reads mean.
fn observation_error(code: &str, message: &str) -> Option<ServiceError> {
    match code {
        "command_unavailable" => Some(ServiceError::Conflict {
            code: "command_unavailable",
            message: message.into(),
        }),
        "unsupported" => Some(ServiceError::Conflict {
            code: "unsupported",
            message: message.to_owned(),
        }),
        "not_found" => Some(ServiceError::NotFound {
            code: "not_found",
            message: message.to_owned(),
        }),
        _ => None,
    }
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "chat_not_found",
        message: "There is no such session.".to_owned(),
    }
}

fn closed() -> ServiceError {
    ServiceError::Conflict {
        code: "chat_closed",
        message: "This session has ended.".to_owned(),
    }
}

fn queued_not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "queued_not_found",
        message: "That message is no longer waiting to be sent.".to_owned(),
    }
}

fn unconfirmed() -> ServiceError {
    ServiceError::Conflict {
        code: "message_unconfirmed",
        message:
            "Ahab could not confirm delivery. Check the conversation before sending a new message."
                .to_owned(),
    }
}

fn submission_result(previous: Submission, fingerprint: &str) -> Result<(), ServiceError> {
    if previous.fingerprint != fingerprint {
        return Err(ServiceError::Conflict {
            code: "submission_changed",
            message: "That submission ID was used with a different message.".to_owned(),
        });
    }
    if previous.queued || previous.state == SubmissionState::Sent {
        return Ok(());
    }
    Err(unconfirmed())
}

/// Errors that prove the message never reached an agent. Transport timeouts,
/// disconnects and failures after a daemon responds can leave delivery unknown.
fn definitely_unsent(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Unavailable {
            code: "machine_offline",
            ..
        } | ServiceError::Conflict {
            code: "chat_not_resumable"
                | "session_archived"
                | "chat_working"
                | "command_unavailable",
            ..
        } | ServiceError::NotFound {
            code: "folder_missing" | "project_not_found",
            ..
        }
    )
}

impl ChatService {
    #[expect(
        clippy::too_many_arguments,
        reason = "the services a chat reaches, given once at startup"
    )]
    pub fn new(
        repo: Arc<dyn ChatRepository>,
        projects: Arc<ProjectService>,
        sessions: Arc<SessionService>,
        agents: Arc<AgentService>,
        presence: Arc<Presence>,
        files: Arc<FileService>,
        clock: Arc<dyn Clock>,
        model_cache: Arc<dyn ModelCacheRepository>,
    ) -> Self {
        let (drain, drained) = mpsc::unbounded_channel();
        Self {
            repo,
            projects,
            sessions,
            agents,
            presence,
            files,
            clock,
            live: broadcast::channel(1024).0,
            transitions: broadcast::channel(256).0,
            model_cache,
            refreshing: Arc::new(Mutex::new(HashSet::new())),
            machines: Mutex::new(HashMap::new()),
            next: Mutex::new(HashMap::new()),
            switching: Mutex::new(HashMap::new()),
            echoes: Mutex::new(HashMap::new()),
            observations: Mutex::new(HashMap::new()),
            queue_updates: tokio::sync::Mutex::new(()),
            drain,
            drained: Mutex::new(Some(drained)),
        }
    }

    /// A chat's current durable queue, including interrupted delivery attempts.
    pub async fn queue_event(&self, chat: Uuid) -> Result<Value, RepoError> {
        Ok(queue_value(&self.repo.queued(chat).await?))
    }

    /// Resolves attempts left in flight when the last server stopped. It
    /// cannot tell whether the daemon received them, so it never resends them.
    pub async fn recover_submissions(&self) -> Result<(), RepoError> {
        self.repo.recover_submissions().await
    }

    async fn tell_queue(&self, chat: Uuid) {
        let _updating = self.queue_updates.lock().await;
        match self.queue_event(chat).await {
            Ok(event) => {
                let _ = self.live.send(Arc::new(LiveEvent {
                    chat,
                    seq: None,
                    event,
                }));
            }
            Err(error) => tracing::warn!(%chat, %error, "cannot publish the chat queue"),
        }
    }

    /// A submission is about to reach the chat's agent: its echo gets its ID.
    fn expect_echo(&self, chat: Uuid, submission: Uuid) {
        lock(&self.echoes)
            .entry(chat)
            .or_default()
            .expect(submission);
    }

    /// The agent took the submission (`reached`), or it never got there.
    fn settle_echo(&self, chat: Uuid, submission: Uuid, reached: bool) {
        let mut echoes = lock(&self.echoes);
        if let Some(waiting) = echoes.get_mut(&chat) {
            if reached {
                waiting.delivered(submission);
            } else {
                waiting.forget(submission);
            }
            if waiting.is_empty() {
                echoes.remove(&chat);
            }
        }
    }

    fn queue_failed(&self, chat: Uuid, message: String) {
        let _ = self.live.send(Arc::new(LiveEvent {
            chat,
            seq: None,
            event: json!({ "type": "queueFailed", "message": message }),
        }));
    }

    /// Sends a waiting message next, stopping the current turn if needed.
    pub async fn send_now(self: &Arc<Self>, chat: Uuid, item: Uuid) -> Result<(), ServiceError> {
        let session = self.get(chat).await?;
        if session.state == ChatState::Closed && session.native_id.is_none() {
            return Err(closed());
        }
        if !self.repo.promote_submission(chat, item).await? {
            return Err(queued_not_found());
        }
        self.tell_queue(chat).await;
        if session.state == ChatState::Closed {
            let service = Arc::clone(self);
            return tokio::spawn(async move {
                let message = service
                    .repo
                    .submission(chat, item)
                    .await?
                    .ok_or_else(queued_not_found)?;
                service.deliver_queued(session, message, None).await
            })
            .await
            .map_err(|_| unconfirmed())?;
        }
        if matches!(session.status, ChatStatus::Working | ChatStatus::Starting) {
            self.interrupt_for_queue(&session).await;
        } else {
            let _ = self.drain.send(chat);
        }
        Ok(())
    }

    async fn interrupt_for_queue(&self, session: &Chat) {
        let id = session.id;
        if let Err(error) = self
            .control(
                session,
                "chats.interrupt",
                json!({ "chat": id, "turn": session.current_turn }),
            )
            .await
        {
            tracing::info!(chat = %id, %error, "message queued but turn could not be interrupted");
            self.queue_failed(id, format!("The message is still queued. {error}"));
        }
    }

    /// Takes a waiting message back, or dismisses an unconfirmed delivery
    /// after the person has checked their conversation.
    pub async fn unqueue(&self, chat: Uuid, item: Uuid) -> Result<(), ServiceError> {
        let message = self
            .repo
            .submission(chat, item)
            .await?
            .filter(|message| {
                message.queued
                    && matches!(
                        message.state,
                        SubmissionState::Queued | SubmissionState::Unconfirmed
                    )
            })
            .ok_or_else(queued_not_found)?;
        if !self
            .repo
            .update_submission(chat, item, message.state, SubmissionState::Withdrawn)
            .await?
        {
            return Err(queued_not_found());
        }
        self.tell_queue(chat).await;
        Ok(())
    }

    /// Puts the chat's waiting messages in a new order: `order` lists them,
    /// first to be sent first.
    pub async fn reorder_queue(&self, chat: Uuid, order: &[Uuid]) -> Result<(), ServiceError> {
        self.get(chat).await?;
        self.repo.reorder_queue(chat, order).await?;
        self.tell_queue(chat).await;
        Ok(())
    }

    /// Changes the text of a message still waiting for its turn. The files
    /// and context it carries stay with it. A command waits as it was chosen.
    pub async fn edit_queued(
        &self,
        chat: Uuid,
        item: Uuid,
        text: &str,
    ) -> Result<(), ServiceError> {
        let message = self
            .repo
            .submission(chat, item)
            .await?
            .filter(|message| message.queued && message.state == SubmissionState::Queued)
            .ok_or_else(queued_not_found)?;
        if message.command.is_some() {
            return Err(ServiceError::Conflict {
                code: "queued_command",
                message: "A queued command cannot be edited; take it back and choose it again."
                    .to_owned(),
            });
        }
        let prompt = model::edited_prompt(&message.shown, &message.prompt, text);
        if prompt.trim().is_empty() && message.attachments.is_empty() {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "text",
                "Write a message, or take it back instead.",
            )]));
        }
        if !self.repo.edit_queued(chat, item, text, &prompt).await? {
            return Err(queued_not_found());
        }
        self.tell_queue(chat).await;
        Ok(())
    }

    async fn send_next(&self, chat: Uuid) -> Result<(), ServiceError> {
        let session = self.get(chat).await?;
        if session.state != ChatState::Live
            || session.status != ChatStatus::Idle
            || !self.presence.is_online(session.machine_id)
        {
            return Ok(());
        }
        let after = self.repo.last_seq(chat).await?;
        let Some(next) = self
            .repo
            .queued(chat)
            .await?
            .into_iter()
            .find(|item| item.state == SubmissionState::Queued)
        else {
            return Ok(());
        };
        self.deliver_queued(session, next, Some(after)).await
    }

    async fn deliver_queued(
        &self,
        session: Chat,
        message: Submission,
        after: Option<i64>,
    ) -> Result<(), ServiceError> {
        let chat = session.id;
        if !self.repo.claim_submission(chat, message.id, after).await? {
            return if after.is_some() {
                Ok(())
            } else {
                Err(queued_not_found())
            };
        }
        self.tell_queue(chat).await;
        self.expect_echo(chat, message.id);
        let result = self
            .deliver(
                session,
                &message.prompt,
                &message.attachments,
                message.command.as_ref(),
            )
            .await;
        let state = match &result {
            Ok(()) => SubmissionState::Sent,
            Err(error) if definitely_unsent(error) => SubmissionState::Queued,
            Err(_) => SubmissionState::Unconfirmed,
        };
        self.settle_echo(chat, message.id, state != SubmissionState::Queued);
        self.repo
            .update_submission(chat, message.id, SubmissionState::Sending, state)
            .await?;
        self.tell_queue(chat).await;
        if let Err(error) = result {
            if state == SubmissionState::Unconfirmed {
                return Err(unconfirmed());
            }
            self.queue_failed(chat, error.to_string());
            return Err(error);
        }
        Ok(())
    }

    /// A chat's latest subagents and background tasks, if its agent tells them.
    pub fn observation(&self, chat: Uuid) -> Option<Value> {
        lock(&self.observations).get(&chat).cloned()
    }

    /// Keeps a chat's latest observation, or forgets it when the chat stops.
    fn note_observation(&self, chat: Uuid, event: &Value) {
        match event.get("type").and_then(Value::as_str) {
            Some("observation") => {
                lock(&self.observations).insert(chat, event.clone());
            }
            Some("closed") => {
                lock(&self.observations).remove(&chat);
            }
            _ => {}
        }
    }

    /// The model and effort a live chat goes on with at its next message,
    /// if they differ from what it runs with.
    pub fn next_model(&self, chat: Uuid) -> Option<NextModel> {
        lock(&self.next).get(&chat).cloned()
    }

    /// A chat's fast mode, if one was chosen.
    pub async fn fast(&self, chat: Uuid) -> Result<Option<bool>, ServiceError> {
        Ok(self.repo.fast(chat).await?)
    }

    /// Chooses the model (and effort) a chat goes on with: at once for a
    /// stopped chat (its next message resumes with them); at the next
    /// message for a live one, by resuming its agent's own session. Only the
    /// chat's own agent's models: the provider never changes.
    pub async fn set_model(
        &self,
        id: Uuid,
        model: Option<&str>,
        effort: Option<&str>,
        fast: Option<bool>,
    ) -> Result<(Chat, Option<NextModel>), ServiceError> {
        let mut chat = self.get(id).await?;
        let model = model.map(str::trim).filter(|value| !value.is_empty());
        let effort = effort.map(str::trim).filter(|value| !value.is_empty());
        if chat.agent == "pi" {
            return Err(ServiceError::Conflict {
                code: "model_fixed",
                message: "Pi's model is chosen in Pi itself.".to_owned(),
            });
        }
        let now_fast = self.repo.fast(id).await?;
        let wanted = NextModel {
            model: model.map(str::to_owned).or_else(|| chat.model.clone()),
            effort: effort.map(str::to_owned),
            fast: fast.or(now_fast),
        };
        if chat.state == ChatState::Closed {
            chat.model = wanted.model;
            chat.effort = wanted.effort;
            chat.updated_at = self.clock.now();
            self.repo.update(&chat).await?;
            self.repo.set_fast(id, wanted.fast).await?;
            lock(&self.next).remove(&id);
            return Ok((chat, None));
        }
        let mut next = lock(&self.next);
        if wanted.model == chat.model && wanted.effort == chat.effort && wanted.fast == now_fast {
            next.remove(&id);
            return Ok((chat, None));
        }
        next.insert(id, wanted.clone());
        Ok((chat, Some(wanted)))
    }

    /// Stops a live chat's agent quietly and waits until it has, for a switch.
    async fn stop_for_switch(&self, chat: &Chat, next: NextModel) -> Result<Chat, ServiceError> {
        lock(&self.switching).insert(chat.id, next);
        if let Err(error) = self
            .control(chat, "chats.close", json!({ "chat": chat.id }))
            .await
        {
            lock(&self.switching).remove(&chat.id);
            return Err(error);
        }
        let deadline = tokio::time::Instant::now() + SWITCH_WAIT;
        loop {
            let now = self.get(chat.id).await?;
            if now.state == ChatState::Closed {
                return Ok(now);
            }
            if tokio::time::Instant::now() >= deadline {
                lock(&self.switching).remove(&chat.id);
                return Err(ServiceError::Conflict {
                    code: "switch_timed_out",
                    message: "The agent did not stop to switch its model; try again.".to_owned(),
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn get(&self, id: Uuid) -> Result<Chat, ServiceError> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    pub async fn fork(&self, id: Uuid, seq: i64, new_session: bool) -> Result<Chat, ServiceError> {
        let source = self.get(id).await?;
        let session = self.sessions.get(source.session_id).await?;
        if !new_session && session.archived_at.is_some() {
            return Err(ServiceError::Conflict {
                code: "session_archived",
                message: "Unarchive the session to add a chat.".into(),
            });
        }
        let native = source
            .native_id
            .as_ref()
            .ok_or_else(|| ServiceError::Conflict {
                code: "chat_not_resumable",
                message: "This chat has no provider session.".into(),
            })?;
        let events = self.repo.events(id, seq.saturating_sub(1), 1).await?;
        let turn = events
            .first()
            .filter(|e| e.seq == seq && e.event["type"] == "turnEnd")
            .and_then(|e| e.event.pointer("/turn/id")?.as_str())
            .ok_or_else(|| {
                ServiceError::Invalid(vec![FieldError::new("seq", "Choose a finished turn.")])
            })?;
        let answer = self.presence.request(source.machine_id, "chats.fork", json!({"project": source.project_id, "agent": source.agent, "nativeId":native, "turn":turn}), START_TIMEOUT).await?;
        let fork = answer
            .as_str()
            .filter(|id| !id.is_empty() && *id != native)
            .ok_or_else(|| {
                ServiceError::Internal("The daemon returned an invalid fork ID.".into())
            })?;
        let id = self
            .repo
            .save_fork(&source, fork, seq, new_session, self.clock.now())
            .await?;
        let chat = self.get(id).await?;
        self.announce(&chat);
        Ok(chat)
    }

    pub async fn project_commands(
        &self,
        project: Uuid,
        agent: &str,
    ) -> Result<Vec<super::api::AgentCommand>, ServiceError> {
        let project = self.projects.get(project).await?;
        self.check_available(&project)?;
        let added = self.agents.launch_for(project.machine_id, agent).await?;
        self.presence
            .ask(
                project.machine_id,
                "chats.previewCommands",
                json!({"project":project.id,"agent":agent,"added":added}),
                START_TIMEOUT,
                observation_error,
                "commands_failed",
            )
            .await
    }

    pub async fn modes(&self, id: Uuid) -> Result<super::api::SessionModes, ServiceError> {
        let chat = self.live_session(id).await?;
        self.presence
            .ask(
                chat.machine_id,
                "chats.modes",
                json!({"chat":id}),
                CONTROL_TIMEOUT,
                observation_error,
                "modes_failed",
            )
            .await
    }

    pub async fn commands(&self, id: Uuid) -> Result<Vec<super::api::AgentCommand>, ServiceError> {
        let chat = self.live_session(id).await?;
        self.presence
            .ask(
                chat.machine_id,
                "chats.commands",
                json!({ "chat": id }),
                CONTROL_TIMEOUT,
                observation_error,
                "commands_failed",
            )
            .await
    }

    pub async fn stop_work(
        &self,
        id: Uuid,
        target: super::api::ChatStopTarget,
    ) -> Result<super::api::ChatStopReceipt, ServiceError> {
        let chat = self.live_session(id).await?;
        self.presence
            .ask(
                chat.machine_id,
                "chats.stopWork",
                json!({"chat":id,"target":target}),
                CONTROL_TIMEOUT,
                observation_error,
                "stop_work_failed",
            )
            .await
    }

    /// A page of one of the chat's background tasks' output, read on the
    /// chat's machine (live, or from the agent's stored session).
    pub async fn task_output(
        &self,
        id: Uuid,
        task: &str,
        cursor: Option<String>,
        limit: Option<u32>,
    ) -> Result<TaskOutputDto, ServiceError> {
        let chat = self.get(id).await?;
        self.presence
            .ask(
                chat.machine_id,
                "chats.taskOutput",
                json!({
                    "chat": chat.id,
                    "agent": chat.agent,
                    "nativeId": chat.native_id,
                    "task": task,
                    "cursor": cursor,
                    "limit": limit,
                }),
                CONTROL_TIMEOUT,
                observation_error,
                "task_output_failed",
            )
            .await
    }

    /// One of the chat's subagents' conversation so far, as events.
    pub async fn agent_history(
        &self,
        id: Uuid,
        subagent: &str,
    ) -> Result<Vec<Value>, ServiceError> {
        let chat = self.get(id).await?;
        self.presence
            .ask(
                chat.machine_id,
                "chats.agentHistory",
                json!({
                    "chat": chat.id,
                    "agent": chat.agent,
                    "nativeId": chat.native_id,
                    "subagent": subagent,
                }),
                CONTROL_TIMEOUT,
                observation_error,
                "agent_history_failed",
            )
            .await
    }

    /// A session's chats, most recently changed first.
    pub async fn list(&self, session: Uuid) -> Result<Vec<Chat>, ServiceError> {
        self.sessions.get(session).await?;
        Ok(self.repo.list_for_session(session).await?)
    }

    /// A session's kept events after `after`, at most `limit`.
    pub async fn events(
        &self,
        id: Uuid,
        after: i64,
        limit: u64,
    ) -> Result<Vec<StoredEvent>, ServiceError> {
        self.get(id).await?;
        Ok(self.repo.events(id, after, limit).await?)
    }

    /// A recent page, ending between turns when possible. A bounded
    /// request can split a long turn so opening it never scans its history.
    pub async fn history(
        &self,
        id: Uuid,
        before: i64,
        bounded: bool,
    ) -> Result<(Vec<StoredEvent>, bool), ServiceError> {
        const WINDOW: usize = 96;
        let batch_size = if bounded { WINDOW as u64 + 1 } else { 128 };
        self.get(id).await?;
        let mut cursor = if before == 0 { i64::MAX } else { before };
        let mut history = Vec::new();
        loop {
            let batch = self.repo.events_before(id, cursor, batch_size).await?;
            let full = batch.len() as u64 == batch_size;
            for event in batch {
                cursor = event.seq;
                if (history.len() >= 64 && event.event["type"] == "turnEnd")
                    || (bounded && history.len() >= WINDOW)
                {
                    history.reverse();
                    return Ok((history, true));
                }
                history.push(event);
            }
            if !full {
                history.reverse();
                return Ok((history, false));
            }
        }
    }

    /// Searches a project's sessions: their titles and the messages in
    /// their chats (yours and the agents'), for every word of `query`.
    /// Titles first, then messages, each newest first; at most `limit` of
    /// each.
    pub async fn search(
        &self,
        project: Uuid,
        query: &str,
        limit: u64,
    ) -> Result<Vec<SessionSearchHitDto>, ServiceError> {
        let terms = model::search_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let titles = self.repo.search_titles(project, &terms, limit).await?;
        let messages = self.repo.search_messages(project, &terms, limit).await?;
        let mut hits: Vec<SessionSearchHitDto> = titles
            .into_iter()
            .map(|(session, project, title, at)| SessionSearchHitDto {
                session_id: session,
                project_id: project,
                snippet: title.clone(),
                session_title: title,
                matched: SessionSearchMatchDto::Title,
                chat_id: None,
                seq: None,
                at,
            })
            .collect();
        hits.extend(messages.into_iter().map(|hit| SessionSearchHitDto {
            session_id: hit.session_id,
            project_id: hit.project_id,
            session_title: hit.session_title,
            matched: match hit.role {
                model::MessageRole::User => SessionSearchMatchDto::UserMessage,
                model::MessageRole::Agent => SessionSearchMatchDto::AgentMessage,
            },
            chat_id: Some(hit.chat_id),
            seq: Some(hit.seq),
            snippet: model::snippet(&hit.text, &terms, 180),
            at: hit.created_at,
        }));
        Ok(hits)
    }

    /// The files a session's agents edited (marked in the Git view).
    pub async fn edited_files(&self, session: Uuid) -> Result<Vec<String>, ServiceError> {
        self.sessions.get(session).await?;
        let items = self.repo.finished_items(session).await?;
        Ok(model::edited_files(&items))
    }

    /// What a chat's agent has spent so far: its tokens, and its cost.
    pub async fn usage(&self, chat: Uuid) -> Result<(Option<i64>, Option<f64>), ServiceError> {
        self.get(chat).await?;
        let events = self.repo.events_of_kind(chat, "turnAccounting").await?;
        Ok(model::usage_of(&events))
    }

    /// The chat's turns, oldest first: each message the person sent, where
    /// it is in the history and the start of its text, for jumping to it
    /// before its page is loaded.
    pub async fn turns(&self, chat: Uuid) -> Result<Vec<(i64, String)>, ServiceError> {
        self.get(chat).await?;
        Ok(self
            .repo
            .user_messages(chat)
            .await?
            .into_iter()
            .map(|(seq, text)| (seq, text.chars().take(200).collect()))
            .collect())
    }

    /// Every event from now on, for clients watching live.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<LiveEvent>> {
        self.live.subscribe()
    }

    /// The chats running on `machine` (a machine with live work
    /// that goes offline is told).
    /// How many agents are working now (starting, or running a turn).
    pub async fn working(&self) -> Result<u64, ServiceError> {
        Ok(self.repo.working().await?)
    }

    pub async fn live_on(&self, machine: Uuid) -> Result<Vec<Chat>, ServiceError> {
        Ok(self.repo.live_on_machine(machine).await?)
    }

    /// Chats that finished, need their person, or failed, from now on.
    pub fn transitions(&self) -> broadcast::Receiver<Arc<ChatTransition>> {
        self.transitions.subscribe()
    }

    fn tell(&self, before: &Chat, after: &Chat) {
        if before.state != after.state
            || before.status != after.status
            || before.open_requests != after.open_requests
        {
            self.announce(after);
        }
        if let Some(transition) = model::transition(before, after) {
            let _ = self.transitions.send(Arc::new(ChatTransition {
                chat: after.clone(),
                transition,
            }));
        }
    }

    fn announce(&self, chat: &Chat) {
        self.sessions.announce(chat.project_id, chat.session_id);
    }

    /// The models `agent` offers on `machine`, with their reasoning
    /// efforts, and when they were read. Kept by Ahab: shown at once, read
    /// again in the background once older than [`MODELS_FRESH`]; asked of
    /// the agent now only when none are kept, or on `refresh`.
    pub async fn models(
        &self,
        machine: Uuid,
        agent: &str,
        refresh: bool,
    ) -> Result<(Vec<ModelDto>, OffsetDateTime, bool), ServiceError> {
        if !refresh && let Some((value, fetched_at)) = self.model_cache.get(machine, agent).await? {
            let stale = self.clock.now() - fetched_at
                > time::Duration::try_from(MODELS_FRESH).unwrap_or(time::Duration::MAX);
            let refreshing = stale && self.refresh_models_later(machine, agent);
            return Ok((model::parse_catalog(&value), fetched_at, refreshing));
        }
        let (value, fetched_at) = read_models(
            &self.presence,
            &self.agents,
            &self.model_cache,
            &self.clock,
            machine,
            agent,
        )
        .await?;
        Ok((model::parse_catalog(&value), fetched_at, false))
    }

    /// Reads `agent`'s models on `machine` again in the background, once at
    /// a time; whether it started.
    fn refresh_models_later(&self, machine: Uuid, agent: &str) -> bool {
        let key = (machine, agent.to_owned());
        if !lock(&self.refreshing).insert(key.clone()) {
            return true;
        }
        let (presence, agents, cache, clock) = (
            Arc::clone(&self.presence),
            Arc::clone(&self.agents),
            Arc::clone(&self.model_cache),
            Arc::clone(&self.clock),
        );
        let refreshing = Arc::clone(&self.refreshing);
        tokio::spawn(async move {
            if let Err(error) = read_models(&presence, &agents, &cache, &clock, key.0, &key.1).await
            {
                tracing::info!(machine = %key.0, agent = %key.1, %error, "cannot read the models again");
            }
            lock(&refreshing).remove(&key);
        });
        true
    }

    /// Starts `agent` in a project with a first message, with `model` and
    /// `effort` or else the agent's defaults.
    #[expect(
        clippy::too_many_arguments,
        reason = "a chat's whole start: agent, message, choices and files"
    )]
    pub async fn start(
        &self,
        session: Uuid,
        agent: &str,
        message: &str,
        model: Option<&str>,
        effort: Option<&str>,
        mode: Option<&str>,
        fast: Option<bool>,
        attachments: &[Uuid],
        references: &[ReferenceDto],
        command: Option<&super::api::CommandInvocation>,
    ) -> Result<Chat, ServiceError> {
        let model = model.map(str::trim).filter(|value| !value.is_empty());
        let effort = effort.map(str::trim).filter(|value| !value.is_empty());
        let message = message.trim();
        if message.is_empty() && attachments.is_empty() && references.is_empty() {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "message",
                "Write what the agent should do.",
            )]));
        }
        check_count(attachments.len())?;
        let session_id = session;
        let found = self.sessions.get(session_id).await?;
        if found.archived_at.is_some() {
            return Err(ServiceError::Conflict {
                code: "session_archived",
                message: "The session is archived; unarchive it to start a chat.".to_owned(),
            });
        }
        let project = found.project_id;
        let project = self.projects.get(project).await?;
        self.check_available(&project)?;
        let mode = model::mode(agent, mode, project.mode == ProjectMode::ReadOnly)?;
        // The last report decides, when there is one.
        let agents = self.agents.list(project.machine_id).await?;
        if let Some(status) = agents.iter().find(|status| status.agent == agent) {
            if status.state != AgentState::Ready {
                return Err(ServiceError::Conflict {
                    code: "agent_not_ready",
                    message: format!(
                        "{} is not ready on this machine ({}).",
                        agent_label(agent),
                        status.state.as_str().replace('_', " ")
                    ),
                });
            }
        } else if !agents.is_empty() {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "agent",
                "Ahab knows claude-code, codex and pi.",
            )]));
        }

        let now = self.clock.now();
        let mut session = Chat {
            id: Uuid::now_v7(),
            session_id,
            project_id: project.id,
            machine_id: project.machine_id,
            agent: agent.to_owned(),
            native_id: None,
            model: model.map(str::to_owned),
            effort: effort.map(str::to_owned),
            mode: mode.clone(),
            title: model::title(&title_text(message, references)),
            state: ChatState::Live,
            status: ChatStatus::Starting,
            unrestricted: agent == "pi",
            current_turn: None,
            open_requests: Vec::new(),
            close_reason: None,
            created_at: now,
            updated_at: now,
            closed_at: None,
        };
        let message = &self.compose(project.id, message, references).await?;
        self.repo.insert(&session).await?;
        self.announce(&session);
        self.repo.set_fast(session.id, fast).await?;
        let added = self.agents.launch_for(project.machine_id, agent).await?;
        let answer = self
            .presence
            .request(
                project.machine_id,
                if command.is_some() {
                    "chats.startCommand"
                } else {
                    "chats.start"
                },
                json!({
                    "added": added,
                    "command": command,
                    "chat": session.id,
                    "project": project.id,
                    "agent": agent,
                    "message": message,
                    "model": model,
                    "effort": effort,
                    "mode": mode,
                    "fast": fast,
                    "attachments": attachments,
                }),
                START_TIMEOUT,
            )
            .await;
        // Events may have changed it meanwhile.
        session = self.get(session.id).await?;
        match answer {
            Ok(value) => {
                self.started(&mut session, value).await?;
                Ok(session)
            }
            Err(error) => {
                session.close(
                    format!("The agent could not start: {error}"),
                    self.clock.now(),
                );
                self.repo.update(&session).await?;
                self.announce(&session);
                Err(error)
            }
        }
    }

    fn check_available(&self, project: &Project) -> Result<(), ServiceError> {
        match self.projects.availability(project) {
            Availability::Available => Ok(()),
            Availability::MachineOffline => Err(ServiceError::Unavailable {
                code: "machine_offline",
                message: "The project's machine is offline.".to_owned(),
            }),
            Availability::Missing => Err(ServiceError::NotFound {
                code: "folder_missing",
                message: "The project's folder is gone from its machine.".to_owned(),
            }),
        }
    }

    /// Records what the daemon started: the agent's session, model, effort
    /// and mode.
    async fn started(&self, chat: &mut Chat, answer: Value) -> Result<(), ServiceError> {
        if let Ok(started) = serde_json::from_value::<Started>(answer) {
            chat.native_id = Some(started.native_id);
            chat.unrestricted = started.unrestricted;
            // What the agent runs, which may be its default.
            if started.model.is_some() {
                chat.model = started.model;
            }
            if started.effort.is_some() {
                chat.effort = started.effort;
            }
            // The agent no longer offers the saved mode: it runs in its own.
            if let Some(saved) = started.mode_unavailable {
                tracing::info!(chat = %chat.id, saved, current = ?started.mode, "the chat's mode follows its agent");
                chat.mode = started.mode;
            }
            if chat.status == ChatStatus::Starting {
                chat.status = ChatStatus::Working;
            }
            chat.updated_at = self.clock.now();
            self.repo.record_started(chat).await?;
            self.announce(chat);
        }
        Ok(())
    }

    /// Continues a chat whose agent stopped, with a new message: the agent
    /// loads its own stored session and carries on.
    async fn resume(
        &self,
        mut chat: Chat,
        text: &str,
        attachments: &[Uuid],
        command: Option<&super::api::CommandInvocation>,
    ) -> Result<(), ServiceError> {
        let Some(native_id) = chat.native_id.clone() else {
            return Err(ServiceError::Conflict {
                code: "chat_not_resumable",
                message: "This chat never started, so it cannot be continued.".to_owned(),
            });
        };
        if self
            .sessions
            .get(chat.session_id)
            .await?
            .archived_at
            .is_some()
        {
            return Err(ServiceError::Conflict {
                code: "session_archived",
                message: "The session is archived; unarchive it to continue.".to_owned(),
            });
        }
        let project = self.projects.get(chat.project_id).await?;
        self.check_available(&project)?;
        let after = self.repo.last_seq(chat.id).await?;
        let fast = self.repo.fast(chat.id).await?;
        chat.reopen(self.clock.now());
        self.repo.update(&chat).await?;
        let added = self
            .agents
            .launch_for(project.machine_id, &chat.agent)
            .await?;
        let answer = self
            .presence
            .ask(
                project.machine_id,
                if command.is_some() {
                    "chats.resumeCommand"
                } else {
                    "chats.resume"
                },
                json!({
                    "added": added,
                    "chat": chat.id,
                    "project": project.id,
                    "agent": chat.agent,
                    "nativeId": native_id,
                    "message": text,
                    "command": command,
                    "model": chat.model,
                    "effort": chat.effort,
                    "mode": chat.mode,
                    "fast": fast,
                    "attachments": attachments,
                    "after": after,
                }),
                START_TIMEOUT,
                observation_error,
                "machine_request_failed",
            )
            .await;
        // Events may have changed it meanwhile.
        let mut chat = self.get(chat.id).await?;
        match answer {
            Ok(value) => self.started(&mut chat, value).await,
            Err(error) => {
                chat.close(
                    format!("The agent could not continue: {error}"),
                    self.clock.now(),
                );
                self.repo.update(&chat).await?;
                Err(error)
            }
        }
    }

    async fn live_session(&self, id: Uuid) -> Result<Chat, ServiceError> {
        let session = self.get(id).await?;
        if session.state == ChatState::Closed {
            return Err(closed());
        }
        Ok(session)
    }

    async fn control(
        &self,
        session: &Chat,
        method: &str,
        params: Value,
    ) -> Result<(), ServiceError> {
        self.presence
            .request(session.machine_id, method, params, CONTROL_TIMEOUT)
            .await
            .map(|_| ())
    }

    pub async fn send(self: &Arc<Self>, id: Uuid, body: MessageDto) -> Result<(), ServiceError> {
        if body.submission_id.is_none() {
            return self.submit(id, body).await;
        }
        // A dropped HTTP response must not cancel delivery or receipt updates.
        let service = Arc::clone(self);
        tokio::spawn(async move { service.submit(id, body).await })
            .await
            .map_err(|_| unconfirmed())?
    }

    async fn submit(&self, id: Uuid, body: MessageDto) -> Result<(), ServiceError> {
        let text = body.text.trim();
        if text.is_empty() && body.attachments.is_empty() && body.references.is_empty() {
            return Err(ServiceError::Invalid(vec![FieldError::new(
                "text",
                "Write a message.",
            )]));
        }
        check_count(body.attachments.len())?;
        let mut chat = self.get(id).await?;
        let fingerprint = if body.submission_id.is_some() {
            let data = serde_json::to_vec(&body).map_err(|error| {
                ServiceError::Internal(format!("cannot encode a message: {error}"))
            })?;
            STANDARD_NO_PAD.encode(Sha256::digest(data))
        } else {
            String::new()
        };
        if let Some(known) = body.submission_id
            && let Some(previous) = self.repo.submission(id, known).await?
        {
            return submission_result(previous, &fingerprint);
        }
        let prompt = self
            .compose(chat.project_id, text, &body.references)
            .await?;
        if !body.references.is_empty() {
            chat = self.get(id).await?;
        }
        let queued = body.delivery.is_some()
            && chat.state == ChatState::Live
            && matches!(chat.status, ChatStatus::Working | ChatStatus::Starting);
        if !queued && body.submission_id.is_none() {
            return self
                .deliver(chat, &prompt, &body.attachments, body.command.as_ref())
                .await;
        }
        let message = Submission {
            id: body.submission_id.unwrap_or_else(Uuid::now_v7),
            fingerprint,
            state: if queued {
                SubmissionState::Queued
            } else {
                SubmissionState::Sending
            },
            queued,
            interrupt: queued && body.delivery == Some(DeliveryDto::Interrupt),
            shown: text.to_owned(),
            prompt,
            command: body.command,
            attachments: body.attachments,
        };
        if !self.repo.add_submission(id, &message).await? {
            // A concurrent matching request won the atomic insert. If its
            // definitely-unsent receipt was removed meanwhile, retry later.
            let previous = self
                .repo
                .submission(id, message.id)
                .await?
                .ok_or_else(unconfirmed)?;
            return submission_result(previous, &message.fingerprint);
        }
        if queued {
            self.tell_queue(id).await;
            // The turn may have ended between reading its state and accepting
            // the message. The queue claim fences duplicate drain requests.
            if self
                .get(id)
                .await
                .is_ok_and(|chat| chat.status == ChatStatus::Idle)
            {
                let _ = self.drain.send(id);
            } else if message.interrupt {
                self.interrupt_for_queue(&chat).await;
            }
            return Ok(());
        }
        self.expect_echo(id, message.id);
        let result = self
            .deliver(
                chat,
                &message.prompt,
                &message.attachments,
                message.command.as_ref(),
            )
            .await;
        self.settle_echo(
            id,
            message.id,
            !matches!(&result, Err(error) if definitely_unsent(error)),
        );
        match &result {
            Ok(()) => {
                self.repo
                    .update_submission(
                        id,
                        message.id,
                        SubmissionState::Sending,
                        SubmissionState::Sent,
                    )
                    .await?;
            }
            Err(error) if definitely_unsent(error) => {
                self.repo.forget_submission(id, message.id).await?;
            }
            Err(error) => {
                tracing::info!(chat = %id, submission = %message.id, %error, "cannot confirm message delivery");
                self.repo
                    .update_submission(
                        id,
                        message.id,
                        SubmissionState::Sending,
                        SubmissionState::Unconfirmed,
                    )
                    .await?;
                return Err(unconfirmed());
            }
        }
        result
    }

    /// Gives a message to the chat's agent: resuming it when it stopped, or
    /// with the model chosen for its next message.
    async fn deliver(
        &self,
        session: Chat,
        text: &str,
        attachments: &[Uuid],
        command: Option<&super::api::CommandInvocation>,
    ) -> Result<(), ServiceError> {
        let id = session.id;
        if session.state == ChatState::Closed {
            return self.resume(session, text, attachments, command).await;
        }
        // Another model chosen while it ran: its agent's own session goes on
        // with it (Stubb changes models only at a start or a resume).
        let next = lock(&self.next).remove(&id);
        if let Some(next) = next {
            if session.status == ChatStatus::Working || session.status == ChatStatus::Starting {
                lock(&self.next).insert(id, next);
                return Err(ServiceError::Conflict {
                    code: "chat_working",
                    message: "Switch the model once this turn ends.".to_owned(),
                });
            }
            let mut stopped = self.stop_for_switch(&session, next.clone()).await?;
            self.repo.set_fast(id, next.fast).await?;
            stopped.model = next.model;
            stopped.effort = next.effort;
            let resumed = self.resume(stopped, text, attachments, command).await;
            lock(&self.switching).remove(&id);
            return resumed;
        }
        if let Some(command) = command {
            self.presence
                .ask::<Value>(
                    session.machine_id,
                    "chats.invoke",
                    json!({"chat":id,"text":text,"command":command,"attachments":attachments}),
                    CONTROL_TIMEOUT,
                    observation_error,
                    "command_failed",
                )
                .await?;
            return Ok(());
        }
        self.control(
            &session,
            "chats.send",
            json!({ "chat": id, "text": text, "attachments": attachments }),
        )
        .await
    }

    /// A message with its context after it, as the agent reads it;
    /// lines whose file changed since they were selected say so.
    async fn compose(
        &self,
        project: Uuid,
        text: &str,
        references: &[ReferenceDto],
    ) -> Result<String, ServiceError> {
        if references.is_empty() {
            return Ok(text.to_owned());
        }
        let checks = references.iter().map(|item| async move {
            let stale = match (&item.kind, &item.path, &item.version) {
                (ReferenceKindDto::Lines, Some(path), Some(version)) => self
                    .files
                    .current_version(project, path)
                    .await
                    .is_some_and(|now| &now != version),
                _ => false,
            };
            item.to_model(stale)
        });
        let models = futures_util::future::join_all(checks).await;
        reference::check(&models)?;
        Ok(reference::message(text, &models))
    }

    /// Every agent's defaults: what it starts with unless chosen otherwise.
    pub async fn defaults(&self) -> Result<Vec<model::Defaults>, ServiceError> {
        Ok(self.repo.defaults().await?)
    }

    /// Replaces an agent's defaults (the last choices made for it).
    pub async fn save_defaults(
        &self,
        agent: &str,
        chosen_model: Option<&str>,
        effort: Option<&str>,
        mode: Option<&str>,
    ) -> Result<model::Defaults, ServiceError> {
        if !AGENTS.contains(&agent) {
            return Err(ServiceError::NotFound {
                code: "agent_unknown",
                message: format!("Ahab knows {}.", AGENTS.join(", ")),
            });
        }
        let clean = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let mode = match clean(mode) {
            None => None,
            Some(mode) => model::mode(agent, Some(&mode), false)?,
        };
        let defaults = model::Defaults {
            agent: agent.to_owned(),
            model: clean(chosen_model),
            effort: clean(effort),
            mode,
            updated_at: self.clock.now(),
        };
        self.repo.save_defaults(&defaults).await?;
        Ok(defaults)
    }

    /// Changes what the agent may do without asking. A running agent
    /// switches now (Codex from its next turn); an ended chat resumes in it.
    pub async fn set_mode(&self, id: Uuid, mode: &str) -> Result<Chat, ServiceError> {
        let mut chat = self.get(id).await?;
        let project = self.projects.get(chat.project_id).await?;
        let native = crate::modules::machines::added_agent(&chat.agent);
        let mode = if native && project.mode != ProjectMode::ReadOnly {
            let offered = self.modes(id).await?;
            if !offered.available.iter().any(|choice| choice.id == mode) {
                return Err(ServiceError::Conflict {
                    code: "mode_unavailable",
                    message: "The agent no longer offers that mode.".into(),
                });
            }
            Some(mode.to_owned())
        } else {
            model::mode(
                &chat.agent,
                Some(mode),
                project.mode == ProjectMode::ReadOnly,
            )?
        };
        if !native && chat.mode == mode {
            return Ok(chat);
        }
        if chat.state == ChatState::Live {
            self.control(
                &chat,
                "chats.mode",
                json!({ "chat": id, "project": project.id, "mode": mode }),
            )
            .await?;
        }
        chat = self.get(id).await?;
        chat.mode = mode;
        chat.updated_at = self.clock.now();
        self.repo.update(&chat).await?;
        Ok(chat)
    }

    pub async fn interrupt(&self, id: Uuid) -> Result<(), ServiceError> {
        let session = self.live_session(id).await?;
        self.control(
            &session,
            "chats.interrupt",
            json!({ "chat": id, "turn": session.current_turn }),
        )
        .await
    }

    /// Answers one of the chat's open requests. An answer that does not fit
    /// the request is `invalid_decision`, and the request stays open.
    pub async fn respond(
        &self,
        id: Uuid,
        request: &str,
        answer: &AnswerDto,
    ) -> Result<(), ServiceError> {
        let session = self.live_session(id).await?;
        let Some(open) = session
            .open_requests
            .iter()
            .find(|open| open.get("id").and_then(Value::as_str) == Some(request))
        else {
            return Err(request_not_open());
        };
        let decision = model::decision(open, answer).map_err(invalid_decision)?;
        let approves_plan = open["kind"]["type"] == "planApproval"
            && matches!(answer.decision, DecisionDto::Allow | DecisionDto::Select);
        if approves_plan
            && self.projects.get(session.project_id).await?.mode == ProjectMode::ReadOnly
        {
            return Err(invalid_decision(
                "The project is read-only: the agent can only keep planning.".to_owned(),
            ));
        }
        // The plan's option switches the agent's mode: Ahab's follows.
        let mode = match (&decision, session.agent.as_str()) {
            (stubb::request::Decision::Select { option_id }, "claude-code") if approves_plan => {
                model::plan_mode(option_id)
            }
            _ => None,
        };
        let answered = self.answer(&session, request, answer, decision).await;
        if answered.is_ok()
            && let Some(mode) = mode
        {
            let mut chat = self.get(id).await?;
            if chat.mode.as_deref() != Some(mode) {
                chat.mode = Some(mode.to_owned());
                chat.updated_at = self.clock.now();
                self.repo.update(&chat).await?;
                self.announce(&chat);
            }
        }
        answered
    }

    async fn answer(
        &self,
        session: &Chat,
        request: &str,
        answer: &AnswerDto,
        decision: stubb::request::Decision,
    ) -> Result<(), ServiceError> {
        let id = session.id;
        if !self.presence.supports(session.machine_id, "chats.decide") {
            // Its daemon answers only what tool permissions take.
            let plain = match answer.decision {
                DecisionDto::Allow | DecisionDto::AllowForSession | DecisionDto::Deny => {
                    answer.decision
                }
                _ => {
                    return Err(ServiceError::Conflict {
                        code: "machine_outdated",
                        message: "Update Ahab on this chat's machine to answer this request."
                            .to_owned(),
                    });
                }
            };
            return self
                .control(
                    session,
                    "chats.respond",
                    json!({ "chat": id, "request": request, "decision": plain }),
                )
                .await;
        }
        let params = serde_json::to_value(Decide {
            chat: id,
            request: request.to_owned(),
            decision,
        })
        .map_err(|error| ServiceError::Internal(format!("cannot encode a decision: {error}")))?;
        self.presence
            .ask::<Value>(
                session.machine_id,
                "chats.decide",
                params,
                CONTROL_TIMEOUT,
                |code, message| match code {
                    "invalid_decision" => Some(invalid_decision(message.to_owned())),
                    "request_not_found" => Some(request_not_open()),
                    _ => None,
                },
                "machine_request_failed",
            )
            .await
            .map(|_| ())
    }

    fn forget_live_chat(&self, id: Uuid) {
        lock(&self.machines).remove(&id);
        lock(&self.observations).remove(&id);
        lock(&self.next).remove(&id);
    }

    /// Ends a live session. When its machine is offline, the record is
    /// closed anyway: the agent stopped with the machine or will be told.
    pub async fn close(&self, id: Uuid) -> Result<Chat, ServiceError> {
        let mut session = self.get(id).await?;
        if session.state == ChatState::Closed {
            return Ok(session);
        }
        if let Err(error) = self
            .control(&session, "chats.close", json!({ "chat": id }))
            .await
        {
            tracing::info!(session = %id, %error, "could not tell the machine to close the session");
        }
        session = self.get(id).await?;
        session.close("Closed.".to_owned(), self.clock.now());
        self.repo.update(&session).await?;
        self.forget_live_chat(id);
        self.announce(&session);
        if self.repo.withdraw_queue(id).await? {
            self.tell_queue(id).await;
        }
        Ok(session)
    }

    /// Renames a chat, or moves it to another session of its project. Moving
    /// changes only where it is listed: the agent goes on
    /// as it was, knowing nothing of it.
    pub async fn change(
        &self,
        id: Uuid,
        title: Option<&str>,
        session: Option<Uuid>,
    ) -> Result<Chat, ServiceError> {
        let mut chat = self.get(id).await?;
        if let Some(title) = title {
            let title = title.trim();
            if title.is_empty() || title.chars().count() > 120 {
                return Err(ServiceError::Invalid(vec![FieldError::new(
                    "title",
                    "Between 1 and 120 characters.",
                )]));
            }
            chat.title = title.to_owned();
            chat.updated_at = self.clock.now();
            self.repo.update(&chat).await?;
            self.repo.mark_named(id).await?;
        }
        if let Some(target) = session
            && target != chat.session_id
        {
            let target = self.sessions.get(target).await?;
            if target.project_id != chat.project_id {
                return Err(ServiceError::Conflict {
                    code: "chat_in_other_project",
                    message: "A chat moves only to a session of its own project.".to_owned(),
                });
            }
            self.repo.move_to(id, target.id).await?;
            self.announce(&chat);
            chat.session_id = target.id;
        }
        self.announce(&chat);
        Ok(chat)
    }

    /// Removes a closed chat from its session, with its history.
    pub async fn remove(&self, id: Uuid) -> Result<(), ServiceError> {
        let chat = self.get(id).await?;
        if chat.state == ChatState::Live {
            return Err(ServiceError::Conflict {
                code: "chat_live",
                message: "Stop the chat before removing it.".to_owned(),
            });
        }
        self.repo.delete(id).await?;
        self.forget_live_chat(id);
        self.announce(&chat);
        Ok(())
    }

    /// Ends every live chat of a session.
    pub async fn end_session(&self, session: Uuid) -> Result<(), ServiceError> {
        for chat in self.repo.list_for_session(session).await? {
            if chat.state == ChatState::Live {
                self.close(chat.id).await?;
            }
        }
        Ok(())
    }

    /// Stores and applies one event from a daemon.
    /// A daemon's event for chats (routed here by `TerminalService`).
    pub async fn receive(&self, event: MachineEvent) -> Result<(), ServiceError> {
        if event.kind != "chat.event" {
            return Ok(());
        }
        let data: ChatEvent = serde_json::from_value(event.data).map_err(|error| {
            ServiceError::Internal(format!("unreadable session event: {error}"))
        })?;
        // Streamed text is not kept: only whose it is needs checking.
        let known = lock(&self.machines).get(&data.chat).copied();
        if data.seq.is_none() && known.is_some() {
            if known == Some(event.machine) {
                self.note_observation(data.chat, &data.event);
                let _ = self.live.send(Arc::new(LiveEvent {
                    chat: data.chat,
                    seq: None,
                    event: data.event,
                }));
            }
            return Ok(());
        }
        let Some(mut session) = self.repo.get(data.chat).await? else {
            return Ok(());
        };
        if session.machine_id != event.machine {
            return Ok(());
        }
        if session.state == ChatState::Live {
            lock(&self.machines).insert(session.id, session.machine_id);
            self.note_observation(data.chat, &data.event);
        }
        // A switch's stop is kept as what it is: the model changing.
        let switched = (data.event.get("type").and_then(Value::as_str) == Some("closed"))
            .then(|| lock(&self.switching).get(&data.chat).cloned())
            .flatten();
        let mut shown = match &switched {
            Some(next) => {
                json!({ "type": "modelChanged", "model": next.model, "effort": next.effort })
            }
            None => data.event.clone(),
        };
        if data.seq.is_some() {
            let mut echoes = lock(&self.echoes);
            if let Some(waiting) = echoes.get_mut(&data.chat) {
                waiting.tag(&mut shown);
                if waiting.is_empty() {
                    echoes.remove(&data.chat);
                }
            }
        }
        if let Some(seq) = data.seq {
            let now = self.clock.now();
            if !self.repo.add_event(session.id, seq, &shown, now).await? {
                // Sent again after a reconnect; already applied.
                return Ok(());
            }
            let before = session.clone();
            if session.apply(&data.event, now) {
                self.repo.update_activity(&session).await?;
                if switched.is_none() {
                    if session.state != ChatState::Live {
                        self.forget_live_chat(session.id);
                    }
                    self.tell(&before, &session);
                }
                self.after_change(&before, &session);
            }
            if let Some(title) = (data.event.get("type").and_then(Value::as_str) == Some("title"))
                .then(|| data.event.get("title").and_then(Value::as_str))
                .flatten()
            {
                self.agent_titled(&mut session, title).await?;
            }
        }
        let _ = self.live.send(Arc::new(LiveEvent {
            chat: data.chat,
            seq: data.seq,
            event: shown,
        }));
        Ok(())
    }

    /// The agent named the chat (Stubb's `title` event): the title replaces
    /// the one taken from the first message, unless someone named the chat.
    /// Its session follows when its title came from this chat.
    async fn agent_titled(&self, chat: &mut Chat, title: &str) -> Result<(), ServiceError> {
        let title = model::title(title);
        if title.is_empty() || title == chat.title || self.repo.is_named(chat.id).await? {
            return Ok(());
        }
        let before = std::mem::replace(&mut chat.title, title);
        self.repo.update(chat).await?;
        self.announce(chat);
        self.sessions
            .agent_titled(chat.session_id, &before, &chat.title)
            .await
    }

    /// A turn ended: see whether the durable queue has another message.
    fn after_change(&self, before: &Chat, now: &Chat) {
        if now.state == ChatState::Live
            && before.status != ChatStatus::Idle
            && now.status == ChatStatus::Idle
        {
            let _ = self.drain.send(now.id);
        }
    }

    /// After a daemon connects: sessions the server thinks are live there but
    /// the daemon does not run stopped with it.
    async fn reconcile(&self, machine: Uuid) -> Result<(), ServiceError> {
        let answer = self
            .presence
            .request(machine, "chats.live", json!({}), CONTROL_TIMEOUT)
            .await?;
        // An answer it cannot read closes nothing: it would close every chat.
        let running: HashSet<Uuid> = serde_json::from_value::<LiveChats>(answer)
            .map_err(|error| {
                ServiceError::Internal(format!("unreadable chats.live answer: {error}"))
            })?
            .chats
            .into_iter()
            .collect();
        for mut session in self.repo.live_on_machine(machine).await? {
            if session.status != ChatStatus::Starting && !running.contains(&session.id) {
                let before = session.clone();
                session.close(model::MACHINE_RESTARTED.to_owned(), self.clock.now());
                self.repo.update(&session).await?;
                self.forget_live_chat(session.id);
                self.tell(&before, &session);
                self.after_change(&before, &session);
            } else if session.status == ChatStatus::Idle {
                let _ = self.drain.send(session.id);
            }
        }
        Ok(())
    }

    /// Checks each machine's live chats as it connects, for as long as the
    /// server runs. (Daemons' events reach `receive` through the terminal
    /// service, which routes them.)
    pub fn spawn_background(self: &Arc<Self>) {
        // Messages kept before searching existed are taken in once.
        let repo = Arc::clone(&self.repo);
        tokio::spawn(async move {
            match repo.index_messages().await {
                Ok(0) => {}
                Ok(taken) => tracing::info!(taken, "kept messages are searchable now"),
                Err(error) => tracing::warn!(%error, "cannot make kept messages searchable"),
            }
        });
        if let Some(mut drained) = lock(&self.drained).take() {
            let service = Arc::clone(self);
            tokio::spawn(async move {
                while let Some(chat) = drained.recv().await {
                    if let Err(error) = service.send_next(chat).await {
                        tracing::info!(%chat, %error, "cannot send a waiting message");
                    }
                }
            });
        }
        let service = Arc::clone(self);
        let mut connected = self.presence.subscribe();
        tokio::spawn(async move {
            loop {
                match connected.recv().await {
                    Ok(machine) => {
                        let service = Arc::clone(&service);
                        tokio::spawn(async move {
                            if let Err(error) = service.reconcile(machine).await {
                                tracing::info!(%machine, %error, "cannot check the machine's sessions");
                            }
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }
}

fn queue_value(items: &[Submission]) -> Value {
    json!({
        "type": "queue",
        "items": items
            .iter()
            .map(|item| json!({
                "id": item.id,
                "text": item.shown,
                "attachments": item.attachments.len(),
                "interrupt": item.interrupt,
                "unconfirmed": item.state == SubmissionState::Unconfirmed,
            }))
            .collect::<Vec<_>>(),
    })
}

/// What a chat's title is made from: the message, or else what its first
/// reference is.
pub fn title_text(message: &str, references: &[ReferenceDto]) -> String {
    if !message.trim().is_empty() {
        return message.to_owned();
    }
    references
        .first()
        .map(|first| reference::summary(&first.to_model(false)))
        .unwrap_or_default()
}

fn request_not_open() -> ServiceError {
    ServiceError::Conflict {
        code: "request_not_open",
        message: "That request has already been answered or withdrawn.".to_owned(),
    }
}

/// An answer that does not fit its request: `reason` says why.
fn invalid_decision(reason: String) -> ServiceError {
    ServiceError::Conflict {
        code: "invalid_decision",
        message: reason,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::modules::sessions::chat::model::parse_catalog;

    #[test]
    fn reads_models_and_their_efforts_from_a_catalog() {
        let models = parse_catalog(&json!({
            "provider": { "id": "claude" },
            "models": [
                { "id": "opus", "label": "Opus", "description": "Most capable", "isDefault": true,
                  "efforts": [{ "id": "low", "description": "Fast" }, { "id": "high", "description": "Deep" }],
                  "defaultEffort": "high", "fastMode": { "status": "unavailable" } },
                { "id": "", "label": "broken" },
            ],
        }));
        assert_eq!(models.len(), 1);
        assert!(models[0].is_default);
        assert_eq!(models[0].efforts[1].id, "high");
        assert_eq!(models[0].default_effort.as_deref(), Some("high"));
        assert!(parse_catalog(&json!({})).is_empty());
    }
}
