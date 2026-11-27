//! Terminals through their machine's daemon: starting one (with
//! its session), killing, restarting and removing it, attaching to it, and
//! keeping its state as the machine reports it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::machines::{Channel, MachineEvent, Presence};
use crate::modules::projects::ProjectService;
use crate::modules::sessions::chat::service::ChatService;
use crate::modules::sessions::session::model::{Session, SessionKind};
use crate::modules::sessions::session::service::SessionService;
use crate::modules::sessions::terminal::model::{self, NewTerminal, Terminal, TerminalState};
use crate::modules::sessions::terminal::store::TerminalRepository;
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

const TIMEOUT: Duration = Duration::from_secs(20);

/// How often connected machines' terminals are checked against the server's.
const SWEEP: Duration = Duration::from_mins(5);

pub struct TerminalService {
    repo: Arc<dyn TerminalRepository>,
    sessions: Arc<SessionService>,
    projects: Arc<ProjectService>,
    presence: Arc<Presence>,
    clock: Arc<dyn Clock>,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "terminal_not_found",
        message: "There is no such terminal.".to_owned(),
    }
}

/// The daemon's own terminal errors, as the API reports them.
fn terminal_error(code: &str, message: &str) -> Option<ServiceError> {
    let message = message.to_owned();
    match code {
        "terminal_failed" => Some(ServiceError::Conflict {
            code: "terminal_failed",
            message,
        }),
        "terminal_denied" => Some(ServiceError::Conflict {
            code: "terminal_denied",
            message,
        }),
        "terminal_host_unavailable" => Some(ServiceError::Unavailable {
            code: "terminal_host_unavailable",
            message,
        }),
        _ => None,
    }
}

impl TerminalService {
    pub fn new(
        repo: Arc<dyn TerminalRepository>,
        sessions: Arc<SessionService>,
        projects: Arc<ProjectService>,
        presence: Arc<Presence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            sessions,
            projects,
            presence,
            clock,
        }
    }

    /// The terminals running on `machine`.
    pub async fn running_on(&self, machine: Uuid) -> Result<Vec<Terminal>, ServiceError> {
        Ok(self.repo.running_on(machine).await?)
    }

    pub async fn get(&self, id: Uuid) -> Result<Terminal, ServiceError> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    fn announce(&self, terminal: &Terminal) {
        self.sessions
            .announce(terminal.project_id, terminal.session_id);
    }

    /// A session's terminals, oldest first.
    pub async fn of_session(&self, session: Uuid) -> Result<Vec<Terminal>, ServiceError> {
        Ok(self.repo.for_session(session).await?)
    }

    /// A project's terminals.
    pub async fn of_project(&self, project: Uuid) -> Result<Vec<Terminal>, ServiceError> {
        Ok(self.repo.for_project(project).await?)
    }

    async fn ask(&self, machine: Uuid, method: &str, params: Value) -> Result<Value, ServiceError> {
        self.presence
            .ask(
                machine,
                method,
                params,
                TIMEOUT,
                terminal_error,
                "machine_request_failed",
            )
            .await
    }

    /// Starts a terminal (a terminal session) in a project: its login
    /// shell, or `command`; filed in `folder` if given.
    pub async fn start(
        &self,
        project: Uuid,
        new: NewTerminal,
    ) -> Result<(Session, Terminal), ServiceError> {
        let project = self.projects.get(project).await?;
        let command = new
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty());
        let title = new.title.as_deref().or(command);
        let session = self
            .sessions
            .create(project.id, SessionKind::Terminal, title, new.folder)
            .await?;
        self.launch(&session, new, None, true).await
    }

    /// Starts a terminal in an existing session, beside its chats and other
    /// terminals; `title`, if given, is its name there.
    pub async fn start_in(
        &self,
        session: Uuid,
        new: NewTerminal,
    ) -> Result<(Session, Terminal), ServiceError> {
        let session = self.sessions.get(session).await?;
        if session.archived_at.is_some() {
            return Err(ServiceError::Conflict {
                code: "session_archived",
                message: "The session is archived; unarchive it to start a terminal.".to_owned(),
            });
        }
        let label = new
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        self.launch(&session, new, label, false).await
    }

    /// Starts `new` in `session` on its project's machine. When it cannot
    /// start, nothing is left behind: not the terminal, nor a session made
    /// for it (`own_session`).
    async fn launch(
        &self,
        session: &Session,
        new: NewTerminal,
        label: Option<String>,
        own_session: bool,
    ) -> Result<(Session, Terminal), ServiceError> {
        let NewTerminal {
            command,
            cols,
            rows,
            plugin,
            context,
            ..
        } = new;
        let command = command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty());
        let project = self.projects.get(session.project_id).await?;
        let session = session.clone();
        let (cols, rows) = model::size(cols, rows);
        let now = self.clock.now();
        let terminal = Terminal {
            id: Uuid::now_v7(),
            session_id: session.id,
            project_id: project.id,
            machine_id: project.machine_id,
            command: command.map(str::to_owned),
            state: TerminalState::Running,
            exit_code: None,
            cols,
            rows,
            title: None,
            label,
            plugin,
            context,
            hooked_by: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.repo.insert(&terminal).await?;
        if let Some(label) = &terminal.label {
            self.repo.set_label(terminal.id, Some(label)).await?;
        }
        let started = self
            .ask(
                terminal.machine_id,
                "terminals.create",
                json!({
                    "terminal": terminal.id,
                    "project": project.id,
                    "session": session.id,
                    "command": terminal.command,
                    "cols": cols,
                    "rows": rows,
                    "context": terminal.context,
                }),
            )
            .await;
        let answer = match started {
            Ok(answer) => answer,
            Err(error) => {
                // Nothing runs: no empty terminal is left behind.
                self.repo.delete(terminal.id).await?;
                if own_session {
                    self.sessions.discard(session.id).await?;
                }
                return Err(error);
            }
        };
        let mut terminal = terminal;
        terminal.hooked_by = answer["hookedBy"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect();
        if !terminal.hooked_by.is_empty() {
            self.repo.update(&terminal).await?;
        }
        self.announce(&terminal);
        Ok((session, terminal))
    }

    /// Ends its process; it stays, exited, until archived.
    pub async fn kill(&self, id: Uuid) -> Result<Terminal, ServiceError> {
        let terminal = self.get(id).await?;
        if terminal.state == TerminalState::Running {
            self.ask(
                terminal.machine_id,
                "terminals.kill",
                json!({ "terminal": id }),
            )
            .await?;
        }
        Ok(terminal)
    }

    /// Runs its command again, in the same terminal: attachments see the new
    /// process's output after the old one's.
    pub async fn restart(&self, id: Uuid) -> Result<Terminal, ServiceError> {
        let mut terminal = self.get(id).await?;
        let session = self.sessions.get(terminal.session_id).await?;
        if session.archived_at.is_some() {
            return Err(ServiceError::Conflict {
                code: "session_archived",
                message: "Restore the session before restarting its terminal.".to_owned(),
            });
        }
        let known = self
            .ask(
                terminal.machine_id,
                "terminals.restart",
                json!({ "terminal": id }),
            )
            .await;
        if known.is_err() {
            // The machine no longer has it (it restarted): a new one, with
            // the same ID.
            self.ask(
                terminal.machine_id,
                "terminals.create",
                json!({
                    "terminal": id,
                    "project": terminal.project_id,
                    "session": terminal.session_id,
                    "command": terminal.command,
                    "cols": terminal.cols,
                    "rows": terminal.rows,
                }),
            )
            .await?;
        }
        terminal.restarted(self.clock.now());
        self.repo.update(&terminal).await?;
        self.announce(&terminal);
        Ok(terminal)
    }

    /// Ends a session's terminals, when it is archived.
    pub async fn end_session(&self, session: Uuid) -> Result<(), ServiceError> {
        for terminal in self.repo.for_session(session).await? {
            if terminal.state == TerminalState::Running {
                // An offline machine ends it when it comes back (reconcile).
                let _ = self
                    .ask(
                        terminal.machine_id,
                        "terminals.kill",
                        json!({ "terminal": terminal.id }),
                    )
                    .await;
            }
        }
        Ok(())
    }

    /// Forgets a session's terminals, on their machine too, when it is deleted.
    pub async fn delete_session(&self, session: Uuid) -> Result<(), ServiceError> {
        for terminal in self.repo.for_session(session).await? {
            self.forget(&terminal).await?;
        }
        Ok(())
    }

    async fn forget(&self, terminal: &Terminal) -> Result<(), ServiceError> {
        let _ = self
            .ask(
                terminal.machine_id,
                "terminals.remove",
                json!({ "terminal": terminal.id }),
            )
            .await;
        self.repo.delete(terminal.id).await?;
        self.announce(terminal);
        Ok(())
    }

    /// Renames a terminal (`Some(None)` takes its name away, so its shell's
    /// title shows again) or moves it to another session of its project. Moving
    /// changes only where it is listed: it keeps running.
    pub async fn change(
        &self,
        id: Uuid,
        label: Option<Option<&str>>,
        session: Option<Uuid>,
    ) -> Result<Terminal, ServiceError> {
        let mut terminal = self.get(id).await?;
        if let Some(target) = session
            && target != terminal.session_id
        {
            let target = self.sessions.get(target).await?;
            if target.project_id != terminal.project_id {
                return Err(ServiceError::Conflict {
                    code: "chat_in_other_project",
                    message: "A terminal moves only to a session of its own project.".to_owned(),
                });
            }
            let previous = terminal.session_id;
            terminal.session_id = target.id;
            terminal.updated_at = self.clock.now();
            self.repo.update(&terminal).await?;
            self.sessions.announce(terminal.project_id, previous);
        }
        if let Some(label) = label {
            let label = label.map(str::trim).filter(|label| !label.is_empty());
            if label.is_some_and(|label| label.chars().count() > 120) {
                return Err(ServiceError::Invalid(vec![
                    crate::shared::error::FieldError::new("label", "At most 120 characters."),
                ]));
            }
            self.repo.set_label(id, label).await?;
            terminal.label = label.map(str::to_owned);
        }
        self.announce(&terminal);
        Ok(terminal)
    }

    /// Removes a terminal from its session, with what it showed: an exited
    /// one, or a running one when `stop` (its machine ends its process).
    pub async fn remove(&self, id: Uuid, stop: bool) -> Result<(), ServiceError> {
        let terminal = self.get(id).await?;
        if terminal.state == TerminalState::Running && !stop {
            return Err(ServiceError::Conflict {
                code: "terminal_running",
                message: "Stop the terminal before removing it.".to_owned(),
            });
        }
        if terminal.state == TerminalState::Running {
            self.ask(
                terminal.machine_id,
                "terminals.remove",
                json!({"terminal": id}),
            )
            .await?;
            self.repo.delete(id).await?;
            return Ok(());
        }
        self.forget(&terminal).await
    }

    /// A channel to the terminal on its machine: the host's frames (a
    /// kind, then bytes) and input and resizes back.
    pub async fn attach(&self, id: Uuid) -> Result<Channel, ServiceError> {
        let terminal = self.get(id).await?;
        match self
            .presence
            .open_channel(
                terminal.machine_id,
                "terminals.attach",
                json!({ "terminal": id }),
                TIMEOUT,
            )
            .await?
        {
            Ok(channel) => Ok(channel),
            Err(error) => Err(terminal_error(&error.code, &error.message).unwrap_or(
                ServiceError::Conflict {
                    code: "terminal_gone",
                    message: "The machine no longer has this terminal; restart it.".to_owned(),
                },
            )),
        }
    }

    /// The terminals a plugin started.
    pub async fn by_plugin(&self, plugin: &str) -> Result<Vec<Terminal>, ServiceError> {
        Ok(self.repo.by_plugin(plugin).await?)
    }

    /// A terminal's event from its machine (it ended).
    async fn receive(&self, event: &MachineEvent) -> Result<(), ServiceError> {
        let data = &event.data;
        if data["type"] != "exited" && data["type"] != "title" {
            return Ok(());
        }
        let Some(id) = data["terminal"].as_str().and_then(|id| id.parse().ok()) else {
            return Ok(());
        };
        let Some(mut terminal) = self.repo.get(id).await? else {
            return Ok(());
        };
        if terminal.machine_id != event.machine {
            return Ok(());
        }
        if data["type"] == "title" {
            // Not activity: the list keeps its order as commands run.
            terminal.title = data["title"]
                .as_str()
                .map(|title| title.chars().take(200).collect());
            self.repo.update(&terminal).await?;
            self.announce(&terminal);
            return Ok(());
        }
        let code = data["code"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok());
        terminal.exited(code, self.clock.now());
        self.repo.update(&terminal).await?;
        self.announce(&terminal);
        self.presence.bus().publish(
            "terminal.exited",
            json!({
                "terminal": terminal.id,
                "session": terminal.session_id,
                "project": terminal.project_id,
                "machine": terminal.machine_id,
                "code": code,
                "plugin": terminal.plugin,
            }),
        );
        Ok(())
    }

    /// Brings a machine and the server into line: terminals the server
    /// thinks run there but its host does not have ended with the machine;
    /// terminals its host has that the server no longer knows (their
    /// session or project was deleted) are removed there.
    async fn reconcile(&self, machine: Uuid) -> Result<(), ServiceError> {
        let running = self.repo.running_on(machine).await?;
        let answer = self.ask(machine, "terminals.list", json!({})).await?;
        let listed: Vec<Uuid> = answer["terminals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|listed| listed["id"].as_str()?.parse().ok())
            .collect();
        for id in listed {
            if self.repo.get(id).await?.is_none() {
                self.ask(machine, "terminals.remove", json!({ "terminal": id }))
                    .await?;
            }
        }
        let alive: HashSet<Uuid> = answer["terminals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|listed| listed["exited"].is_null())
            .filter_map(|listed| listed["id"].as_str()?.parse().ok())
            .collect();
        for mut terminal in running {
            if !alive.contains(&terminal.id) {
                terminal.exited(None, self.clock.now());
                self.repo.update(&terminal).await?;
                self.announce(&terminal);
            }
        }
        Ok(())
    }

    /// Routes daemons' events for as long as the server runs: chats' to the
    /// chats, terminals' here; each is acknowledged once handled.
    pub fn spawn_background(self: &Arc<Self>, chats: Arc<ChatService>) {
        if let Some(mut events) = self.presence.take_events() {
            let service = Arc::clone(self);
            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    let (machine, generation, seq) = (event.machine, event.generation, event.seq);
                    let handled = match event.kind.as_str() {
                        "terminal.event" => service.receive(&event).await,
                        _ => chats.receive(event).await,
                    };
                    match handled {
                        Ok(()) => {
                            if seq > 0 {
                                service.presence.ack(machine, generation, seq);
                            }
                        }
                        // Not acknowledged: the daemon sends it again.
                        Err(error) => {
                            tracing::warn!(%machine, %error, "cannot store a machine's event");
                            if seq > 0 {
                                service.presence.retry_events(machine, generation);
                            }
                        }
                    }
                }
            });
        }
        // Now and then, every connected machine: terminals whose session went
        // with a deleted project end there too.
        let sweeping = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(SWEEP);
            every.tick().await;
            loop {
                every.tick().await;
                for machine in sweeping.presence.online() {
                    if let Err(error) = sweeping.reconcile(machine).await {
                        tracing::debug!(%machine, %error, "cannot check the machine's terminals");
                    }
                }
            }
        });
        let service = Arc::clone(self);
        let mut connected = self.presence.subscribe();
        tokio::spawn(async move {
            loop {
                match connected.recv().await {
                    Ok(machine) => {
                        let service = Arc::clone(&service);
                        tokio::spawn(async move {
                            if let Err(error) = service.reconcile(machine).await {
                                tracing::info!(%machine, %error, "cannot check the machine's terminals");
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
