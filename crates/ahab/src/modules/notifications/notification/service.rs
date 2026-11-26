//! Sending notifications, the inbox, the server's settings, and turning
//! chats' transitions into notifications.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::modules::machines::{MachineService, Presence};
use crate::modules::notifications::notification::model::ChatPlace;
use crate::modules::notifications::notification::model::{
    self, NewNotification, Notification, Settings,
};
use crate::modules::notifications::notification::store::NotificationRepository;
use crate::modules::projects::ProjectService;
use crate::modules::sessions::{
    ChatService, ChatState, ChatStatus, SessionService, TerminalService, Transition,
};
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

/// How long a chat must stay idle after a turn before it counts as finished:
/// agents often start a follow-up turn at once (finished means it
/// stopped completely).
const FINISHED_AFTER: Duration = Duration::from_secs(4);
/// How long a machine with work on it must stay offline to be told.
const OFFLINE_AFTER: Duration = Duration::from_mins(1);
/// How often Ahab looks for a newer release.
const CHECK_EVERY: Duration = Duration::from_hours(24);
/// This server's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where newer releases of Ahab are listed, and whether pre-releases count.
#[derive(Debug, Clone)]
pub struct ReleaseCheck {
    pub url: String,
    pub prereleases: bool,
}

/// What watching machines needs.
#[derive(Clone)]
pub struct Watched {
    pub chats: Arc<ChatService>,
    pub terminals: Arc<TerminalService>,
    pub machines: Arc<MachineService>,
    pub presence: Arc<Presence>,
}

/// What a chat's notifications are told with: the chat, and where it is.
#[derive(Clone)]
pub struct ChatSources {
    pub chats: Arc<ChatService>,
    pub projects: Arc<ProjectService>,
    pub sessions: Arc<SessionService>,
}

impl ChatSources {
    /// The names of where `chat` is; a name that cannot be read is left out.
    async fn place(&self, chat: &crate::modules::sessions::Chat) -> ChatPlace {
        let (project, session) = tokio::join!(
            self.projects.get(chat.project_id),
            self.sessions.get(chat.session_id),
        );
        ChatPlace {
            project: project.ok().map(|project| project.name.to_string()),
            session: session.ok().map(|session| session.title),
        }
    }
}

/// How often read notifications older than the kept days are deleted.
const CLEAN_EVERY: Duration = Duration::from_hours(24);

/// What every open app hears (the stream).
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationEvent {
    /// A new one: shown as a toast, and outside the app when allowed.
    Created(Box<Notification>),
    /// Read, cleared or settings changed: the inbox and count are fetched again.
    Changed { unread: u64 },
}

/// Asked before a notification is kept: plugins' `notifications.send`
/// hook may change it or drop it.
#[async_trait::async_trait]
pub trait Intercept: Send + Sync {
    /// The notification to keep, or `None` to drop it.
    async fn before_send(&self, new: NewNotification) -> Option<NewNotification>;
}

pub struct NotificationService {
    repo: Arc<dyn NotificationRepository>,
    clock: Arc<dyn Clock>,
    live: broadcast::Sender<Arc<NotificationEvent>>,
    intercept: std::sync::OnceLock<std::sync::Weak<dyn Intercept>>,
}

impl NotificationService {
    pub fn new(repo: Arc<dyn NotificationRepository>, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            live: broadcast::channel(256).0,
            intercept: std::sync::OnceLock::new(),
        }
    }

    /// Who is asked before each notification is kept (set once, at start).
    pub fn intercept_with(&self, intercept: &Arc<dyn Intercept>) {
        let _ = self.intercept.set(Arc::downgrade(intercept));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<NotificationEvent>> {
        self.live.subscribe()
    }

    pub async fn settings(&self) -> Result<Settings, ServiceError> {
        Ok(self.repo.settings().await?.unwrap_or_default())
    }

    pub async fn set_settings(&self, settings: Settings) -> Result<Settings, ServiceError> {
        settings.check().map_err(ServiceError::Invalid)?;
        self.repo.set_settings(&settings, self.clock.now()).await?;
        self.changed().await?;
        Ok(settings)
    }

    /// Sends `new` unless its kind (or notifications at all) is turned off;
    /// the notification kept, if any.
    pub async fn send(&self, new: NewNotification) -> Result<Option<Notification>, ServiceError> {
        if !self.settings().await?.sends(&new.kind) {
            return Ok(None);
        }
        let new = match self.intercept.get().and_then(std::sync::Weak::upgrade) {
            Some(intercept) => match intercept.before_send(new).await {
                Some(new) => new,
                None => return Ok(None),
            },
            None => new,
        };
        let notification = Notification {
            id: Uuid::now_v7(),
            kind: new.kind,
            source: new.source,
            level: new.level,
            title: new.title,
            body: new.body,
            project_id: new.project_id,
            target: new.target,
            group_key: new.group_key,
            created_at: self.clock.now(),
            read_at: None,
        };
        self.repo.add(&notification).await?;
        let _ = self.live.send(Arc::new(NotificationEvent::Created(Box::new(
            notification.clone(),
        ))));
        self.changed().await?;
        Ok(Some(notification))
    }

    /// The inbox: newest first, page by page (`before`).
    pub async fn list(
        &self,
        unread_only: bool,
        before: Option<Uuid>,
        limit: u32,
    ) -> Result<(Vec<Notification>, u64), ServiceError> {
        let before = match before {
            Some(id) => Some(self.get(id).await?.created_at),
            None => None,
        };
        // Counted first: one sent in between shows in the list, so the count
        // never promises one the list lacks.
        let unread = self.repo.unread().await?;
        let items = self
            .repo
            .list(unread_only, before, limit.clamp(1, 200))
            .await?;
        Ok((items, unread))
    }

    pub async fn get(&self, id: Uuid) -> Result<Notification, ServiceError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| ServiceError::NotFound {
                code: "notification_not_found",
                message: "There is no such notification.".to_owned(),
            })
    }

    pub async fn read(&self, id: Uuid) -> Result<Notification, ServiceError> {
        self.get(id).await?;
        if self.repo.mark_read(id, self.clock.now()).await? {
            self.changed().await?;
        }
        self.get(id).await
    }

    /// Marks every unread notification read, of `project` when given.
    pub async fn read_all(&self, project: Option<Uuid>) -> Result<u64, ServiceError> {
        let count = self.repo.mark_all_read(project, self.clock.now()).await?;
        if count > 0 {
            self.changed().await?;
        }
        Ok(count)
    }

    /// Deletes every read notification (Clear).
    pub async fn clear_read(&self) -> Result<u64, ServiceError> {
        let count = self.repo.delete_read(None).await?;
        if count > 0 {
            self.changed().await?;
        }
        Ok(count)
    }

    /// Deletes read notifications older than the kept days (none when they
    /// are kept forever); how many.
    pub async fn clean(&self) -> Result<u64, ServiceError> {
        let Some(days) = self.settings().await?.keep_days else {
            return Ok(0);
        };
        let before = self.clock.now() - time::Duration::days(i64::from(days));
        Ok(self.repo.delete_read(Some(before)).await?)
    }

    /// A machine went offline: told when it has running sessions or
    /// terminals and has not come back. (The caller waits first.)
    pub async fn machine_offline(
        &self,
        watched: &Watched,
        machine: Uuid,
    ) -> Result<(), ServiceError> {
        if watched.presence.is_online(machine) {
            return Ok(());
        }
        let work = watched.chats.live_on(machine).await?.len()
            + watched.terminals.running_on(machine).await?.len();
        if work == 0 {
            return Ok(());
        }
        let name = watched.machines.get(machine).await?.name.to_string();
        self.send(model::machine_offline(machine, &name, work))
            .await?;
        Ok(())
    }

    /// A machine connected: what said it was offline is over, and an older
    /// daemon is told once per version.
    pub async fn machine_connected(
        &self,
        watched: &Watched,
        machine: Uuid,
    ) -> Result<(), ServiceError> {
        if self
            .repo
            .read_group(&model::offline_group(machine), self.clock.now())
            .await?
            > 0
        {
            self.changed().await?;
        }
        let missing = watched
            .presence
            .missing_methods(machine)
            .unwrap_or_default();
        if missing.is_empty() {
            return Ok(());
        }
        let found = watched.machines.get(machine).await?;
        let name = found.name.to_string();
        let new = model::machine_behind(machine, &name, found.version.as_deref(), VERSION);
        let told = self
            .repo
            .latest_in_group(&model::behind_group(machine))
            .await?;
        if told.is_some_and(|told| told.body == new.body) {
            return Ok(());
        }
        self.send(new).await?;
        Ok(())
    }

    /// Asks where releases are listed for a newer Ahab, and tells about it
    /// once per version.
    pub async fn check_for_update(&self, check: &ReleaseCheck) -> Result<(), ServiceError> {
        if !self.settings().await?.sends(model::UPDATE_AVAILABLE) {
            return Ok(());
        }
        let releases: serde_json::Value = reqwest::Client::new()
            .get(&check.url)
            .header("User-Agent", format!("Ahab/{VERSION}"))
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|error| ServiceError::Internal(format!("cannot ask for releases: {error}")))?
            .json()
            .await
            .map_err(|error| ServiceError::Internal(format!("unreadable releases: {error}")))?;
        let current = semver::Version::parse(VERSION)
            .map_err(|error| ServiceError::Internal(format!("this version: {error}")))?;
        let Some((version, url)) = model::newer_release(&releases, &current, check.prereleases)
        else {
            return Ok(());
        };
        let new = model::update_available(&version, &url, VERSION);
        let told = self.repo.latest_in_group(model::UPDATE_GROUP).await?;
        if told.is_some_and(|told| told.title == new.title) {
            return Ok(());
        }
        self.send(new).await?;
        Ok(())
    }

    /// Checks for a newer Ahab on start and then daily.
    pub fn spawn_update_check(self: &Arc<Self>, check: ReleaseCheck) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(CHECK_EVERY);
            loop {
                every.tick().await;
                if let Err(error) = service.check_for_update(&check).await {
                    tracing::info!(%error, "cannot check for a newer Ahab");
                }
            }
        });
    }

    /// Tells about machines going offline and older daemons connecting, for
    /// as long as the server runs.
    pub fn spawn_machine_watch(self: &Arc<Self>, watched: Watched) {
        let service = Arc::clone(self);
        let mut connected = watched.presence.subscribe();
        let on_connect = watched.clone();
        tokio::spawn(async move {
            loop {
                match connected.recv().await {
                    Ok(machine) => {
                        if let Err(error) = service.machine_connected(&on_connect, machine).await {
                            tracing::info!(%error, %machine, "cannot tell about a connected machine");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        let service = Arc::clone(self);
        let mut heard = watched.presence.bus().subscribe();
        tokio::spawn(async move {
            loop {
                match heard.recv().await {
                    Ok(event) if event.name == "machine.offline" => {
                        let Some(machine) = event.data["machine"]
                            .as_str()
                            .and_then(|id| Uuid::parse_str(id).ok())
                        else {
                            continue;
                        };
                        let service = Arc::clone(&service);
                        let watched = watched.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(OFFLINE_AFTER).await;
                            if let Err(error) = service.machine_offline(&watched, machine).await {
                                tracing::info!(%error, %machine, "cannot tell about an offline machine");
                            }
                        });
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    async fn changed(&self) -> Result<(), ServiceError> {
        let unread = self.repo.unread().await?;
        let _ = self
            .live
            .send(Arc::new(NotificationEvent::Changed { unread }));
        Ok(())
    }

    /// Turns chats that finish, need their person or fail into notifications,
    /// and deletes old read ones once a day, for as long as the server runs.
    pub fn spawn_background(self: &Arc<Self>, sources: ChatSources) {
        let service = Arc::clone(self);
        let chats = sources;
        let mut transitions = chats.chats.transitions();
        tokio::spawn(async move {
            loop {
                match transitions.recv().await {
                    Ok(told) => {
                        let service = Arc::clone(&service);
                        let chats = chats.clone();
                        tokio::spawn(async move {
                            if let Err(error) =
                                service.tell(&chats, &told.chat, &told.transition).await
                            {
                                tracing::warn!(%error, chat = %told.chat.id, "cannot send a notification");
                            }
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "notifications missed chat changes");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(CLEAN_EVERY);
            loop {
                every.tick().await;
                match service.clean().await {
                    Ok(0) => {}
                    Ok(count) => tracing::info!(count, "deleted old read notifications"),
                    Err(error) => tracing::warn!(%error, "cannot delete old notifications"),
                }
            }
        });
    }

    async fn tell(
        &self,
        sources: &ChatSources,
        chat: &crate::modules::sessions::Chat,
        transition: &Transition,
    ) -> Result<(), ServiceError> {
        let mut chat = chat.clone();
        if *transition == Transition::Finished {
            tokio::time::sleep(FINISHED_AFTER).await;
            chat = sources.chats.get(chat.id).await?;
            let still_done = chat.state == ChatState::Live
                && chat.status == ChatStatus::Idle
                && chat.open_requests.is_empty();
            if !still_done {
                return Ok(());
            }
        }
        let place = sources.place(&chat).await;
        self.send(model::from_transition(&chat, transition, &place))
            .await?;
        Ok(())
    }
}
