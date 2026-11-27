//! The sessions API's request and response types.

use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::sessions::chat::api::{ChatDto, StartChatDto};
use crate::modules::sessions::chat::model::Chat;
use crate::modules::sessions::session::model::{self, Folder, Session, SessionKind, SessionState};
use crate::modules::sessions::terminal::api::TerminalDto;
use crate::modules::sessions::terminal::model::{Terminal, TerminalState};

/// A session's state, from its chats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionState)]
pub enum SessionStateDto {
    /// A chat waits for an answer.
    NeedsYou,
    Working,
    Idle,
    /// Nothing runs.
    Ended,
}

/// What a session holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionKind)]
pub enum SessionKindDto {
    /// Chats with coding agents.
    Chat,
    /// A terminal.
    Terminal,
}

/// One piece of work in a project: its chats, or its terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Session)]
pub struct SessionDto {
    pub id: Uuid,
    pub kind: SessionKindDto,
    pub project_id: Uuid,
    #[schema(example = "Fix the flaky join test")]
    pub title: String,
    /// The project folder it is filed in.
    pub folder_id: Option<Uuid>,
    /// When it was pinned; pinned sessions are listed first.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub pinned_at: Option<OffsetDateTime>,
    /// When it was archived; archived sessions run nothing.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
    pub state: SessionStateDto,
    /// Chats waiting for an answer.
    pub waiting: u32,
    /// Most recent first.
    pub chats: Vec<ChatDto>,
    /// Its first terminal: a terminal session's own.
    pub terminal: Option<TerminalDto>,
    /// Every terminal in it, oldest first.
    pub terminals: Vec<TerminalDto>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When anything in it last changed.
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl SessionDto {
    pub fn new(session: &Session, chats: &[Chat], terminals: &[Terminal]) -> Self {
        let state = match model::state(chats) {
            // A running terminal waits for you, like an idle chat.
            SessionState::Ended
                if terminals
                    .iter()
                    .any(|terminal| terminal.state == TerminalState::Running) =>
            {
                SessionState::Idle
            }
            state => state,
        };
        Self {
            id: session.id,
            kind: match session.kind {
                SessionKind::Chat => SessionKindDto::Chat,
                SessionKind::Terminal => SessionKindDto::Terminal,
            },
            project_id: session.project_id,
            title: session.title.clone(),
            folder_id: session.folder_id,
            pinned_at: session.pinned_at,
            archived_at: session.archived_at,
            state: match state {
                SessionState::NeedsYou => SessionStateDto::NeedsYou,
                SessionState::Working => SessionStateDto::Working,
                SessionState::Idle => SessionStateDto::Idle,
                SessionState::Ended => SessionStateDto::Ended,
            },
            waiting: u32::try_from(
                chats
                    .iter()
                    .filter(|chat| !chat.open_requests.is_empty())
                    .count(),
            )
            .unwrap_or(u32::MAX),
            chats: chats.iter().map(ChatDto::from).collect(),
            terminal: terminals.first().map(TerminalDto::from),
            terminals: terminals.iter().map(TerminalDto::from).collect(),
            created_at: session.created_at,
            updated_at: terminals
                .iter()
                .map(|terminal| terminal.updated_at)
                .fold(model::last_activity(session, chats), OffsetDateTime::max),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = SessionList)]
pub struct SessionListDto {
    pub items: Vec<SessionDto>,
}

/// A new session: its conversation with an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = CreateSession)]
pub struct CreateSessionDto {
    /// Else the first message, shortened.
    pub title: Option<String>,
    pub chat: StartChatDto,
    /// The project's folder to file it in; else none.
    #[serde(rename = "folderId")]
    pub folder_id: Option<Uuid>,
}

/// Reads a field that may be absent (`None`), `null` (`Some(None)`) or set.
#[expect(
    clippy::option_option,
    reason = "absent, null and set are three different requests"
)]
pub(crate) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::deserialize(deserializer).map(Some)
}

/// What to change; absent fields stay as they are.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UpdateSession)]
pub struct UpdateSessionDto {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    /// A folder of the session's project, or `null` to take it out of one.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub folder_id: Option<Option<Uuid>>,
}

/// A folder of sessions in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionFolder)]
pub struct FolderDto {
    pub id: Uuid,
    pub project_id: Uuid,
    #[schema(example = "Auth rewrite")]
    pub name: String,
    /// The folder it is in; none at the project's top level.
    pub parent_id: Option<Uuid>,
    /// When it was pinned; pinned folders come first.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub pinned_at: Option<OffsetDateTime>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<&Folder> for FolderDto {
    fn from(folder: &Folder) -> Self {
        Self {
            id: folder.id,
            project_id: folder.project_id,
            name: folder.name.clone(),
            parent_id: folder.parent_id,
            pinned_at: folder.pinned_at,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = SessionFolderList)]
pub struct FolderListDto {
    /// By name.
    pub items: Vec<FolderDto>,
}

/// A folder's name, 1 to 60 characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = NameSessionFolder)]
pub struct NameFolderDto {
    pub name: String,
    /// The folder to make it in; else the project's top level.
    #[serde(default, rename = "parentId")]
    pub parent_id: Option<Uuid>,
}

/// What to change about a folder; absent fields stay as they are.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UpdateSessionFolder)]
pub struct UpdateFolderDto {
    pub name: Option<String>,
    /// Another folder of the project to move it into, or `null` for the top level.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub parent_id: Option<Option<Uuid>>,
    pub pinned: Option<bool>,
}

/// How many sessions need you, per project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingCounts)]
pub struct WaitingDto {
    /// Project ID → sessions with a chat waiting for an answer.
    pub projects: std::collections::BTreeMap<Uuid, u32>,
}
