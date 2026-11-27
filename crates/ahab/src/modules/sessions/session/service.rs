//! What can be done with sessions: listing them with their chats, creating,
//! renaming, pinning, filing in folders, archiving and deleting, and their
//! saved layouts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use crate::modules::projects::ProjectService;
use crate::modules::sessions::chat::model::Chat;
use crate::modules::sessions::chat::store::ChatRepository;
use crate::modules::sessions::session::model::{self, Folder, Session, SessionKind};
use crate::modules::sessions::session::store::SessionRepository;
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

pub struct SessionService {
    repo: Arc<dyn SessionRepository>,
    chats: Arc<dyn ChatRepository>,
    projects: Arc<ProjectService>,
    clock: Arc<dyn Clock>,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "session_not_found",
        message: "There is no such session.".to_owned(),
    }
}

fn folder_not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "folder_not_found",
        message: "There is no such folder in this project.".to_owned(),
    }
}

/// What to change in a session; `None` leaves a field as it is.
#[derive(Debug, Default)]
pub struct Change<'a> {
    pub title: Option<&'a str>,
    /// `Some(None)` moves it out of its folder.
    pub folder: Option<Option<Uuid>>,
    pub pinned: Option<bool>,
}

impl SessionService {
    pub fn new(
        repo: Arc<dyn SessionRepository>,
        chats: Arc<dyn ChatRepository>,
        projects: Arc<ProjectService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            chats,
            projects,
            clock,
        }
    }

    pub async fn get(&self, id: Uuid) -> Result<Session, ServiceError> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    /// A project's sessions with their chats, most recent activity first.
    pub async fn list(&self, project: Uuid) -> Result<Vec<(Session, Vec<Chat>)>, ServiceError> {
        let sessions = self.repo.list_for_project(project).await?;
        let mut chats: HashMap<Uuid, Vec<Chat>> = HashMap::new();
        for chat in self.chats.list_for_project(project).await? {
            chats.entry(chat.session_id).or_default().push(chat);
        }
        let mut list: Vec<(Session, Vec<Chat>)> = sessions
            .into_iter()
            .map(|session| {
                let mine = chats.remove(&session.id).unwrap_or_default();
                (session, mine)
            })
            .collect();
        list.sort_by_key(|(session, chats)| {
            std::cmp::Reverse(model::last_activity(session, chats))
        });
        Ok(list)
    }

    /// One session with its chats.
    pub async fn with_chats(&self, id: Uuid) -> Result<(Session, Vec<Chat>), ServiceError> {
        let session = self.get(id).await?;
        let chats = self.chats.list_for_session(id).await?;
        Ok((session, chats))
    }

    /// How many sessions need you, per project.
    pub async fn waiting(&self) -> Result<BTreeMap<Uuid, u32>, ServiceError> {
        let mut seen = HashSet::new();
        let mut counts = BTreeMap::new();
        for chat in self.chats.waiting().await? {
            if seen.insert(chat.session_id) {
                *counts.entry(chat.project_id).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    pub(crate) fn announce(&self, project: Uuid, session: Uuid) {
        self.projects.bus().publish(
            "sessions.changed",
            serde_json::json!({ "project": project, "session": session }),
        );
    }

    /// A new, empty session in a project, filed in `folder` if given (one
    /// of the project's session folders).
    pub async fn create(
        &self,
        project: Uuid,
        kind: SessionKind,
        title: Option<&str>,
        folder: Option<Uuid>,
    ) -> Result<Session, ServiceError> {
        let project = self.projects.get(project).await?;
        if let Some(folder) = folder {
            let found = self.repo.folder(folder).await?;
            if found.is_none_or(|found| found.project_id != project.id) {
                return Err(folder_not_found());
            }
        }
        let now = self.clock.now();
        let session = Session {
            id: Uuid::now_v7(),
            project_id: project.id,
            kind,
            title: model::title(
                title,
                match kind {
                    SessionKind::Chat => "New session",
                    SessionKind::Terminal => "Terminal",
                },
            )?,
            folder_id: folder,
            pinned_at: None,
            archived_at: None,
            created_at: now,
            updated_at: now,
        };
        self.repo.insert(&session).await?;
        self.announce(session.project_id, session.id);
        Ok(session)
    }

    /// The agent named the chat that gave the session its title: the session
    /// takes the new one too, unless someone named it or its title is no
    /// longer the chat's old one.
    pub async fn agent_titled(&self, id: Uuid, from: &str, to: &str) -> Result<(), ServiceError> {
        let mut session = self.get(id).await?;
        if session.title != from || session.title == to || self.repo.is_named(id).await? {
            return Ok(());
        }
        session.title = model::title(Some(to), &session.title)?;
        self.repo.update(&session).await?;
        self.announce(session.project_id, session.id);
        Ok(())
    }

    /// Renames, pins or files a session. Only renaming counts as activity.
    pub async fn change(&self, id: Uuid, change: Change<'_>) -> Result<Session, ServiceError> {
        let mut session = self.get(id).await?;
        let now = self.clock.now();
        if let Some(title) = change.title {
            session.title = model::title(Some(title), &session.title)?;
            session.updated_at = now;
            self.repo.mark_named(id).await?;
        }
        if let Some(folder) = change.folder {
            if let Some(folder) = folder {
                let found = self.repo.folder(folder).await?;
                if found.is_none_or(|found| found.project_id != session.project_id) {
                    return Err(folder_not_found());
                }
            }
            session.folder_id = folder;
        }
        if let Some(pinned) = change.pinned {
            session.pinned_at = match (pinned, session.pinned_at) {
                (true, Some(at)) => Some(at),
                (true, None) => Some(now),
                (false, _) => None,
            };
        }
        self.repo.update(&session).await?;
        self.announce(session.project_id, session.id);
        Ok(session)
    }

    /// Marks a session archived; its chats must be ended first.
    pub async fn archive(&self, id: Uuid) -> Result<Session, ServiceError> {
        let mut session = self.get(id).await?;
        if session.archived_at.is_none() {
            session.archived_at = Some(self.clock.now());
            self.repo.update(&session).await?;
            self.announce(session.project_id, session.id);
            self.projects.bus().publish(
                "session.archived",
                serde_json::json!({ "session": session.id, "project": session.project_id }),
            );
        }
        Ok(session)
    }

    pub async fn unarchive(&self, id: Uuid) -> Result<Session, ServiceError> {
        let mut session = self.get(id).await?;
        if session.archived_at.is_some() {
            session.archived_at = None;
            self.repo.update(&session).await?;
            self.announce(session.project_id, session.id);
        }
        Ok(session)
    }

    /// Deletes an archived session with its chats, their history and its
    /// layouts.
    pub async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        let session = self.get(id).await?;
        if session.archived_at.is_none() {
            return Err(ServiceError::Conflict {
                code: "session_not_archived",
                message: "Archive the session before deleting it.".to_owned(),
            });
        }
        self.chats.delete_for_session(id).await?;
        self.repo.delete(id).await?;
        self.announce(session.project_id, session.id);
        Ok(())
    }

    /// Removes a session whose conversation could not start.
    pub async fn discard(&self, id: Uuid) -> Result<(), ServiceError> {
        let session = self.get(id).await?;
        self.chats.delete_for_session(id).await?;
        self.repo.delete(id).await?;
        self.announce(session.project_id, id);
        Ok(())
    }

    /// A project's folders, by name.
    pub async fn folders(&self, project: Uuid) -> Result<Vec<Folder>, ServiceError> {
        Ok(self.repo.folders(project).await?)
    }

    /// Makes a folder at the project's top level or in `parent`.
    pub async fn create_folder(
        &self,
        project: Uuid,
        name: &str,
        parent: Option<Uuid>,
    ) -> Result<Folder, ServiceError> {
        if let Some(parent) = parent {
            let found = self
                .repo
                .folder(parent)
                .await?
                .ok_or_else(folder_not_found)?;
            if found.project_id != project {
                return Err(folder_not_found());
            }
        }
        let now = self.clock.now();
        let folder = Folder {
            id: Uuid::now_v7(),
            project_id: project,
            name: model::folder_name(name)?,
            parent_id: parent,
            pinned_at: None,
            created_at: now,
            updated_at: now,
        };
        self.repo.insert_folder(&folder).await?;
        Ok(folder)
    }

    /// Renames a folder, moves it into another of the project's folders (or
    /// to the top level: `Some(None)`), or pins it. Never into itself or one
    /// of its own folders.
    pub async fn change_folder(
        &self,
        id: Uuid,
        name: Option<&str>,
        parent: Option<Option<Uuid>>,
        pinned: Option<bool>,
    ) -> Result<Folder, ServiceError> {
        let mut folder = self.repo.folder(id).await?.ok_or_else(folder_not_found)?;
        if let Some(name) = name {
            folder.name = model::folder_name(name)?;
        }
        if let Some(parent) = parent {
            if let Some(parent) = parent {
                let all = self.repo.folders(folder.project_id).await?;
                if !all.iter().any(|other| other.id == parent) {
                    return Err(folder_not_found());
                }
                if model::is_within(&all, parent, id) {
                    return Err(ServiceError::Conflict {
                        code: "folder_in_itself",
                        message: "A folder cannot go into itself or one of its own folders."
                            .to_owned(),
                    });
                }
            }
            folder.parent_id = parent;
        }
        if let Some(pinned) = pinned {
            folder.pinned_at = match (pinned, folder.pinned_at) {
                (true, Some(at)) => Some(at),
                (true, None) => Some(self.clock.now()),
                (false, _) => None,
            };
        }
        folder.updated_at = self.clock.now();
        self.repo.update_folder(&folder).await?;
        Ok(folder)
    }

    /// Deletes a folder; what it held (sessions and folders) moves up to
    /// where it was.
    pub async fn delete_folder(&self, id: Uuid) -> Result<(), ServiceError> {
        let folder = self.repo.folder(id).await?.ok_or_else(folder_not_found)?;
        self.repo.move_contents(id, folder.parent_id).await?;
        self.repo.delete_folder(id).await?;
        Ok(())
    }
}
