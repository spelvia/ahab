//! A session, and its state as its chats make it.

use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::sessions::chat::model::{Chat, ChatState, ChatStatus};
use crate::shared::error::{DomainError, FieldError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: SessionKind,
    pub title: String,
    /// The project folder it is filed in.
    pub folder_id: Option<Uuid>,
    /// Pinned sessions are listed first.
    pub pinned_at: Option<OffsetDateTime>,
    /// Archived sessions run nothing and are listed apart.
    pub archived_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// What a session holds: chats with agents, or a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Chat,
    Terminal,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Terminal => "terminal",
        }
    }

    /// Unknown kinds read as chats, the kind every session had before.
    pub fn parse(text: &str) -> Self {
        if text == "terminal" {
            Self::Terminal
        } else {
            Self::Chat
        }
    }
}

/// A folder of sessions, per project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// The folder it is in; none at the project's top level.
    pub parent_id: Option<Uuid>,
    /// When it was pinned; pinned folders come first.
    pub pinned_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Whether folder `id` is `ancestor` or inside it, following parents.
pub fn is_within(folders: &[Folder], id: Uuid, ancestor: Uuid) -> bool {
    let mut at = Some(id);
    // A cycle in stored data ends the walk instead of looping.
    for _ in 0..=folders.len() {
        match at {
            Some(current) if current == ancestor => return true,
            Some(current) => {
                at = folders
                    .iter()
                    .find(|folder| folder.id == current)
                    .and_then(|folder| folder.parent_id);
            }
            None => return false,
        }
    }
    false
}

/// A folder's name: 1 to 60 characters.
pub fn folder_name(value: &str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "name",
            "Name the folder.",
        )]));
    }
    if value.chars().count() > 60 {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "name",
            "Use at most 60 characters.",
        )]));
    }
    Ok(value.to_owned())
}

/// A session's state, from its chats, most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A chat waits for an answer.
    NeedsYou,
    /// A chat is working.
    Working,
    /// A chat runs and waits for your next message.
    Idle,
    /// Nothing runs.
    Ended,
}

pub fn state(chats: &[Chat]) -> SessionState {
    let live = chats.iter().filter(|chat| chat.state == ChatState::Live);
    let mut state = SessionState::Ended;
    for chat in live {
        if !chat.open_requests.is_empty() {
            return SessionState::NeedsYou;
        }
        state = match (state, chat.status) {
            (_, ChatStatus::Working | ChatStatus::Starting) | (SessionState::Working, _) => {
                SessionState::Working
            }
            _ => SessionState::Idle,
        };
    }
    state
}

/// When anything in the session last changed.
pub fn last_activity(session: &Session, chats: &[Chat]) -> OffsetDateTime {
    chats
        .iter()
        .map(|chat| chat.updated_at)
        .chain([session.updated_at])
        .max()
        .unwrap_or(session.updated_at)
}

/// A session's title: 1 to 120 characters; blank gets the default.
pub fn title(value: Option<&str>, fallback: &str) -> Result<String, DomainError> {
    let value = value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(fallback);
    if value.chars().count() > 120 {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "title",
            "Use at most 120 characters.",
        )]));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn chat(state: ChatState, status: ChatStatus, waiting: bool) -> Chat {
        let now = OffsetDateTime::now_utc();
        Chat {
            id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            project_id: Uuid::now_v7(),
            machine_id: Uuid::now_v7(),
            agent: "claude-code".into(),
            native_id: None,
            model: None,
            effort: None,
            mode: None,
            title: "t".into(),
            state,
            status,
            unrestricted: false,
            current_turn: None,
            open_requests: if waiting {
                vec![json!({ "id": "r" })]
            } else {
                Vec::new()
            },
            close_reason: None,
            created_at: now,
            updated_at: now,
            closed_at: None,
        }
    }

    #[test]
    fn the_most_urgent_chat_decides() {
        use ChatState::{Closed, Live};
        use ChatStatus::{Idle, Working};
        assert_eq!(state(&[]), SessionState::Ended);
        assert_eq!(state(&[chat(Closed, Idle, false)]), SessionState::Ended);
        assert_eq!(state(&[chat(Live, Idle, false)]), SessionState::Idle);
        assert_eq!(
            state(&[chat(Live, Idle, false), chat(Live, Working, false)]),
            SessionState::Working
        );
        assert_eq!(
            state(&[chat(Live, Working, false), chat(Live, Idle, true)]),
            SessionState::NeedsYou
        );
    }

    #[test]
    fn folder_names_are_short_and_not_blank() {
        assert_eq!(folder_name(" Auth ").unwrap(), "Auth");
        assert!(folder_name("  ").is_err());
        assert!(folder_name(&"f".repeat(61)).is_err());
    }

    #[test]
    fn titles_fall_back_and_stay_short() {
        assert_eq!(title(Some("  "), "New session").unwrap(), "New session");
        assert_eq!(title(Some(" Fix it "), "x").unwrap(), "Fix it");
        assert!(title(Some(&"y".repeat(121)), "x").is_err());
    }
}
