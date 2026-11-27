//! Chats on the wire: the daemon's `chats.*` messages and `chat.event`, and
//! what the API returns.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::sessions::chat::model::{Chat, ChatState, ChatStatus};
use crate::modules::sessions::reference::api::ReferenceDto;

/// `chats.start`: the server's ID for it, where, which agent, and the
/// first message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartChat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandInvocation>,
    /// How to start an added (ACP) agent; absent for Stubb's own.
    #[serde(default)]
    pub added: Option<AddedLaunch>,
    pub chat: Uuid,
    pub project: Uuid,
    pub agent: String,
    pub message: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// Ahab's permission mode (`chat::model::mode`).
    #[serde(default)]
    pub mode: Option<String>,
    /// The model's fast tier on or off; absent for the provider's default.
    #[serde(default)]
    pub fast: Option<bool>,
    /// Attachments kept on this machine, sent with the message.
    #[serde(default)]
    pub attachments: Vec<Uuid>,
}

/// Fork at this stored turnEnd event, into the same or a fresh session.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ForkChat)]
pub struct ForkChatDto {
    pub seq: i64,
    pub new_session: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkChatRequest {
    pub project: Uuid,
    pub agent: String,
    pub native_id: String,
    pub turn: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SessionModeOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SessionModes {
    pub current: String,
    pub available: Vec<SessionModeOption>,
}

/// One task or delegated agent in a live chat.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChatStopTarget {
    Agent { id: String },
    Task { id: String },
}

#[derive(Debug, Deserialize)]
pub struct StopWorkRequest {
    pub chat: Uuid,
    pub target: ChatStopTarget,
}

/// What the provider acknowledged when asked to stop the selected work.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChatStopReceipt {
    pub disposition: String,
    pub message: Option<String>,
}

/// `chats.taskOutput`: a page of a background task's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOutputRequest {
    pub chat: Uuid,
    pub agent: String,
    /// The agent's own session, read when the chat is not live.
    #[serde(default)]
    pub native_id: Option<String>,
    pub task: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `chats.agentHistory`: a subagent's conversation so far.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHistoryRequest {
    pub chat: Uuid,
    pub agent: String,
    #[serde(default)]
    pub native_id: Option<String>,
    pub subagent: String,
}

/// Which page of a task's output to read.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct TaskOutputQuery {
    /// Where the last page ended (its `nextCursor`); absent for the start.
    pub cursor: Option<String>,
    /// At most this many bytes, 4 to 65536.
    pub limit: Option<u32>,
}

/// A page of a background task's output, as the agent keeps it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = TaskOutput)]
pub struct TaskOutputDto {
    pub task_id: String,
    pub text: String,
    /// Read on from here, also at the current end of a log still growing.
    pub next_cursor: Option<String>,
    /// `complete`, `partial` (its start was not kept), `unavailable` or `unknown`.
    pub availability: String,
    /// At the end of what is written so far.
    pub eof: bool,
}

/// A subagent's conversation so far, as the events that show it.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AgentHistory)]
pub struct AgentHistoryDto {
    #[schema(value_type = Vec<Object>)]
    pub events: Vec<serde_json::Value>,
}

/// `chats.resume`: continue a chat whose agent stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeChat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandInvocation>,
    /// How to start an added (ACP) agent; absent for Stubb's own.
    #[serde(default)]
    pub added: Option<AddedLaunch>,
    pub chat: Uuid,
    pub project: Uuid,
    pub agent: String,
    /// The agent's own session to load.
    pub native_id: String,
    pub message: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub fast: Option<bool>,
    #[serde(default)]
    pub attachments: Vec<Uuid>,
    /// The last event the server kept; new ones are numbered after it.
    pub after: i64,
}

/// `chats.mode`: change a live chat's permission mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeMode {
    pub chat: Uuid,
    pub project: Uuid,
    pub mode: String,
}

/// `agents.catalog`: which agent's models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRequest {
    /// How to start an added (ACP) agent; absent for Stubb's own.
    #[serde(default)]
    pub added: Option<AddedLaunch>,
    pub agent: String,
}

/// The daemon's answer to `chats.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    pub native_id: String,
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// The agent runs without asking (Pi).
    #[serde(default)]
    pub unrestricted: bool,
    /// The chat's saved mode the agent rejected on resume; it runs in
    /// `mode` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_unavailable: Option<String>,
    /// The mode the agent runs in, when `mode_unavailable` is set (`None`
    /// when it does not say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// `chats.send`, `chats.interrupt`, `chats.respond`, `chats.close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandInvocation>,
    pub chat: Uuid,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
    #[serde(default)]
    pub decision: Option<DecisionDto>,
    /// Attachments sent with `text`.
    #[serde(default)]
    pub attachments: Vec<Uuid>,
}

/// The daemon's live chats (`chats.live`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveChats {
    pub chats: Vec<Uuid>,
}

/// `chat.event`: one of the agent's events (Stubb's `session::Event`),
/// numbered per chat when it is kept, without a number when it is only
/// streamed text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatEvent {
    pub chat: Uuid,
    pub seq: Option<i64>,
    pub event: Value,
}

/// What an answer to a request does. A permission (`tool`, `permission`)
/// takes `allow`, `allowForSession`, `deny`, `cancel` or `select`; a
/// question `answer`; a plan `allow`, `select` or `deny` (keep planning, with
/// feedback); a form `submit` or `deny` (decline). Every kind takes `deny`
/// and `cancel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Decision)]
pub enum DecisionDto {
    Allow,
    /// Allow calls like this one for the rest of the session.
    AllowForSession,
    /// Refuse; `message`, when given, tells the agent why.
    Deny,
    /// Refuse and stop the turn.
    Cancel,
    /// Answer a question request: `answers`.
    Answer,
    /// Pick one of the request's `options`: `optionId`.
    Select,
    /// Fill in a form request: `values`.
    Submit,
}

/// `chats.decide`: answer a request with any decision (Stubb's
/// `request::Decision`). `chats.respond` remains for daemons without it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decide {
    pub chat: Uuid,
    pub request: String,
    pub decision: stubb::request::Decision,
}

/// A request an agent waits on, as Stubb sends it (`request::Request`).
/// Clients show a `kind` they do not know as a plain permission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentRequest)]
pub struct RequestDto {
    pub id: String,
    pub turn_id: Option<String>,
    pub kind: RequestKindDto,
    /// The agent's request as received.
    #[schema(value_type = Object)]
    pub native: Value,
}

/// What an agent asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schema(as = AgentRequestKind)]
pub enum RequestKindDto {
    /// Permission to call a tool with `input`.
    #[serde(rename_all = "camelCase")]
    Tool {
        name: String,
        display_name: Option<String>,
        description: Option<String>,
        #[schema(value_type = Object)]
        input: Value,
        tool_call_id: Option<String>,
        /// `allowForSession` is available.
        can_allow_for_session: bool,
    },
    /// Permission to call a tool, with the agent's own choices (ACP).
    #[serde(rename_all = "camelCase")]
    Permission {
        name: String,
        display_name: Option<String>,
        description: Option<String>,
        #[schema(value_type = Object)]
        input: Value,
        tool_call_id: Option<String>,
        options: Vec<ChoiceDto>,
    },
    /// Questions for you, each with options and often your own words.
    #[serde(rename_all = "camelCase")]
    Question {
        questions: Vec<QuestionDto>,
        tool_call_id: Option<String>,
    },
    /// The agent's plan, waiting for approval.
    #[serde(rename_all = "camelCase")]
    PlanApproval {
        /// Markdown.
        plan: String,
        /// Ways to approve it, such as Claude Code's permission modes.
        options: Vec<ChoiceDto>,
        path: Option<String>,
        tool_call_id: Option<String>,
    },
    /// A form to fill in (MCP or ACP elicitation), or a page to open (`url`).
    #[serde(rename_all = "camelCase")]
    Form {
        message: String,
        source: Option<String>,
        fields: Vec<FieldDto>,
        url: Option<String>,
        #[schema(value_type = Object)]
        schema: Value,
        tool_call_id: Option<String>,
    },
}

/// A choice an agent offers, picked with `select`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentChoice)]
pub struct ChoiceDto {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub kind: ChoiceKindDto,
}

/// What picking a choice means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentChoiceKind)]
pub enum ChoiceKindDto {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    #[serde(other)]
    Unknown,
}

/// One question of a question request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentQuestion)]
pub struct QuestionDto {
    /// Names it in an answer's `questionId`.
    pub id: String,
    pub header: Option<String>,
    pub question: String,
    pub options: Vec<QuestionOptionDto>,
    pub multi_select: bool,
    /// Your own words are welcome (an answer's `other`).
    pub allow_other: bool,
    /// The answer is secret, such as a password.
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentQuestionOption)]
pub struct QuestionOptionDto {
    /// An answer's `selected` names options by it.
    pub label: String,
    pub description: Option<String>,
    /// Markdown to show beside it, such as a mockup.
    pub preview: Option<String>,
}

/// One field of a form request: a flat JSON-schema property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentFormField)]
pub struct FieldDto {
    /// Its key in `values`.
    pub name: String,
    pub kind: FieldKindDto,
    pub title: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    pub options: Vec<FieldOptionDto>,
    /// `email`, `uri`, `date` or `date-time`, for text.
    pub format: Option<String>,
    /// Bounds of a number's value, a text's length, a multi-select's count.
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    /// Any JSON value, as the field takes it.
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentFormFieldKind)]
pub enum FieldKindDto {
    Text,
    Number,
    Integer,
    Boolean,
    Select,
    MultiSelect,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentFormFieldOption)]
pub struct FieldOptionDto {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatState)]
pub enum ChatStateDto {
    Live,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatStatus)]
pub enum ChatStatusDto {
    Starting,
    Working,
    Idle,
}

/// A conversation with a coding agent in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Chat)]
pub struct ChatDto {
    pub id: Uuid,
    /// The session it is in.
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub machine_id: Uuid,
    #[schema(example = "claude-code")]
    pub agent: String,
    #[schema(example = "Fix the flaky join test")]
    pub title: String,
    /// The model it runs.
    pub model: Option<String>,
    /// Its reasoning effort.
    pub effort: Option<String>,
    /// What the agent may do without asking: Claude Code `plan`, `ask`,
    /// `acceptEdits` or `bypass`; Codex `readOnly`, `ask`, `workspace` or
    /// `fullAccess`; none for Pi.
    #[schema(example = "ask")]
    pub mode: Option<String>,
    pub state: ChatStateDto,
    pub status: ChatStatusDto,
    /// The agent runs without asking for permission (Pi).
    pub unrestricted: bool,
    /// Once ended, a message continues it (the agent's session is known).
    pub resumable: bool,
    /// This server supports durable submission IDs on messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_submissions: Option<bool>,
    /// Requests waiting for an answer, as the agent sent them.
    #[schema(value_type = Vec<RequestDto>)]
    pub open_requests: Vec<Value>,
    pub close_reason: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub closed_at: Option<OffsetDateTime>,
}

impl From<&Chat> for ChatDto {
    fn from(session: &Chat) -> Self {
        Self {
            id: session.id,
            session_id: session.session_id,
            project_id: session.project_id,
            machine_id: session.machine_id,
            agent: session.agent.clone(),
            title: session.title.clone(),
            model: session.model.clone(),
            effort: session.effort.clone(),
            mode: session.mode.clone(),
            state: match session.state {
                ChatState::Live => ChatStateDto::Live,
                ChatState::Closed => ChatStateDto::Closed,
            },
            status: match session.status {
                ChatStatus::Starting => ChatStatusDto::Starting,
                ChatStatus::Working => ChatStatusDto::Working,
                ChatStatus::Idle => ChatStatusDto::Idle,
            },
            unrestricted: session.unrestricted,
            resumable: session.native_id.is_some(),
            message_submissions: Some(true),
            open_requests: session.open_requests.clone(),
            close_reason: session.close_reason.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            closed_at: session.closed_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ChatList)]
pub struct ChatListDto {
    pub items: Vec<ChatDto>,
}

/// A new session: which agent, the first message, and optionally the model
/// and reasoning effort (else the agent's defaults).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = StartChat)]
pub struct StartChatDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandInvocation>,
    #[schema(example = "claude-code")]
    pub agent: String,
    pub message: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The permission mode (see `Chat.mode`); else the agent's default.
    #[serde(default)]
    pub mode: Option<String>,
    /// The model's fast tier (Claude Code, Codex), where it offers one;
    /// absent for the provider's default.
    #[serde(default)]
    pub fast: Option<bool>,
    /// Attachments uploaded to the project, sent with the message.
    #[serde(default)]
    pub attachments: Vec<Uuid>,
    /// Context for the agent, sent after the message.
    #[serde(default)]
    pub references: Vec<ReferenceDto>,
}

/// A reasoning effort a model accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ModelEffort)]
pub struct EffortDto {
    #[schema(example = "high")]
    pub id: String,
    pub description: String,
}

/// A model an agent offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentModel)]
pub struct ModelDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub is_default: bool,
    /// Reasoning efforts, in the agent's order; empty when it has none.
    pub efforts: Vec<EffortDto>,
    pub default_effort: Option<String>,
    /// Its fast tier, where it offers one.
    pub fast: Option<FastModeDto>,
}

/// A model's faster tier (Claude Code's and Codex's fast mode).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = FastMode)]
#[serde(rename_all = "camelCase")]
pub struct FastModeDto {
    pub label: String,
    pub description: String,
    /// Whether the provider turns it on by default for the model.
    pub default_on: bool,
}

/// The models an agent offers on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentModels)]
#[serde(rename_all = "camelCase")]
pub struct ModelListDto {
    pub items: Vec<ModelDto>,
    /// When the agent was asked for them.
    #[serde(with = "crate::shared::time::rfc3339")]
    pub fetched_at: OffsetDateTime,
    /// They are being read again in the background.
    pub refreshing: bool,
}

/// What an agent starts with unless chosen otherwise: the last model,
/// reasoning effort and permission mode picked for it, on any device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentDefaults)]
pub struct DefaultsDto {
    #[schema(example = "claude-code")]
    pub agent: String,
    #[schema(example = "opus")]
    pub model: Option<String>,
    #[schema(example = "medium")]
    pub effort: Option<String>,
    #[schema(example = "bypass")]
    pub mode: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<&crate::modules::sessions::chat::model::Defaults> for DefaultsDto {
    fn from(defaults: &crate::modules::sessions::chat::model::Defaults) -> Self {
        Self {
            agent: defaults.agent.clone(),
            model: defaults.model.clone(),
            effort: defaults.effort.clone(),
            mode: defaults.mode.clone(),
            updated_at: defaults.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentDefaultsList)]
pub struct DefaultsListDto {
    pub items: Vec<DefaultsDto>,
}

/// An agent's new defaults; absent or empty fields mean the agent's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = SaveAgentDefaults)]
pub struct SaveDefaultsDto {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
}

/// A new permission mode for a chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = SetChatMode)]
pub struct SetModeDto {
    #[schema(example = "acceptEdits")]
    pub mode: String,
}

/// A session entry that the agent can invoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AgentCommand {
    pub server: Option<String>,
    pub arguments: Option<Vec<PromptArgument>>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: CommandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CommandKind {
    Skill,
    McpPrompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

/// The selected provider identity accompanies the user's visible invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommandInvocation {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub arguments: std::collections::BTreeMap<String, String>,
    pub id: String,
    pub kind: CommandKind,
}

/// A message to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = SendMessage)]
pub struct MessageDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandInvocation>,
    /// Reuse this ID with the same message when retrying an unconfirmed request.
    #[serde(
        default,
        rename = "submissionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub submission_id: Option<Uuid>,
    /// May be empty when files are attached or context is added.
    #[serde(default)]
    pub text: String,
    /// Attachments uploaded to the project, sent with the message.
    #[serde(default)]
    pub attachments: Vec<Uuid>,
    /// Context for the agent, sent after the message as text it can read: each
    /// reference's path and lines, and the text as seen.
    #[serde(default)]
    pub references: Vec<ReferenceDto>,
    /// What happens when the agent is working: `queue` keeps the message
    /// until its turn ends, `interrupt` stops the turn and sends it next.
    /// Absent, it goes to the agent at once, as before.
    #[serde(default)]
    pub delivery: Option<DeliveryDto>,
}

/// How a message sent while the agent works reaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Delivery)]
pub enum DeliveryDto {
    Queue,
    Interrupt,
}

/// An answer to a request: the decision, and what it needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AnswerRequest)]
pub struct AnswerDto {
    pub decision: DecisionDto,
    /// With `deny`: why, for the agent; for a plan, what to change.
    #[serde(default)]
    pub message: Option<String>,
    /// With `answer`: one per question.
    #[serde(default)]
    pub answers: Vec<QuestionAnswerDto>,
    /// With `select`: the picked option's `id`.
    #[serde(default)]
    pub option_id: Option<String>,
    /// With `submit`: the values by field name; empty for a page opened.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub values: Option<serde_json::Map<String, Value>>,
}

/// Your answer to one question.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = QuestionAnswer)]
pub struct QuestionAnswerDto {
    pub question_id: String,
    /// The picked options' labels.
    #[serde(default)]
    pub selected: Vec<String>,
    /// Your own words.
    #[serde(default)]
    pub other: Option<String>,
}

/// One kept event of a session: Stubb's event, numbered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatEventRecord)]
pub struct EventDto {
    pub seq: i64,
    #[schema(value_type = Object)]
    pub event: Value,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// What a chat's agent has spent so far, as it reported it after each turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatUsage)]
pub struct ChatUsageDto {
    /// Tokens its turns used, added up; none before the agent reports any.
    pub tokens: Option<i64>,
    /// What it cost in US dollars, as the agent reports it; none for agents
    /// that report no cost.
    pub cost_usd: Option<f64>,
}

/// The waiting messages in the order to send them: their IDs, first first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = QueueOrder)]
pub struct QueueOrderDto {
    pub order: Vec<Uuid>,
}

/// A waiting message's new text. Its files and context stay with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = QueuedEdit)]
pub struct QueuedEditDto {
    pub text: String,
}

/// A chat's turns: each message the person sent, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatTurns)]
pub struct ChatTurnsDto {
    pub items: Vec<ChatTurnDto>,
}

/// One turn: where its message is in the chat's history, and the start of
/// its text (up to 200 characters).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatTurn)]
pub struct ChatTurnDto {
    pub seq: i64,
    pub text: String,
}

/// The files a session's agents edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionEditedFiles)]
pub struct EditedFilesDto {
    /// Paths as the agents gave them, usually absolute; sorted.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatEventList)]
pub struct EventListDto {
    pub items: Vec<EventDto>,
    /// More events follow in the requested direction.
    pub more: bool,
    /// When reading backward, pass this as `before` for the next page, or 0 at the beginning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_before: Option<i64>,
}

/// How many sessions need you, per project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingCounts)]
pub struct WaitingDto {
    /// Project ID → sessions with an open request.
    pub projects: std::collections::BTreeMap<Uuid, u32>,
}

/// An added agent's launch, as the server sends it with a start: its
/// registry entry (as the registry has it) or its custom command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedLaunch {
    pub agent: String,
    pub name: String,
    #[serde(default)]
    pub entry: Option<serde_json::Value>,
    #[serde(default)]
    pub launch: Option<CustomCommand>,
}

/// A custom agent's program, arguments and variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// What a search of a project's sessions found, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionSearch)]
pub struct SessionSearchDto {
    pub items: Vec<SessionSearchHitDto>,
}

/// Where a search hit: a session's title, or one of its messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionSearchMatch)]
pub enum SessionSearchMatchDto {
    Title,
    UserMessage,
    AgentMessage,
}

/// One thing a search found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SessionSearchHit)]
pub struct SessionSearchHitDto {
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub session_title: String,
    #[serde(rename = "match")]
    pub matched: SessionSearchMatchDto,
    /// The message's chat and its event number, to show it; none for a title.
    pub chat_id: Option<Uuid>,
    pub seq: Option<i64>,
    /// The part of the message around what was asked for (or the title).
    pub snippet: String,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub at: OffsetDateTime,
}

/// What to change about a chat; absent fields stay as they are.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UpdateChat)]
pub struct UpdateChatDto {
    /// Its name in its session.
    pub title: Option<String>,
    /// Another session of its project to move it to.
    pub session_id: Option<Uuid>,
}

/// The model (and effort) a chat goes on with; absent keeps what it has.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[schema(as = SetChatModel)]
pub struct SetModelDto {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Fast mode on or off; absent keeps what it has.
    pub fast: Option<bool>,
}

/// A chat's model and effort, and the ones it goes on with at its next
/// message when another was chosen while it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ChatModel)]
pub struct ChatModelDto {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Fast mode, if chosen; else the provider's default.
    pub fast: Option<bool>,
    pub next: Option<NextModelDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = NextChatModel)]
pub struct NextModelDto {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub fast: Option<bool>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Every request kind reads as Stubb writes it, and writes back the same.
    #[test]
    fn request_dtos_carry_every_kind_as_stubb_sends_it() {
        let kinds = [
            json!({ "type": "tool", "name": "Bash", "displayName": null, "description": null,
                "input": { "command": "ls" }, "toolCallId": "c", "canAllowForSession": true }),
            json!({ "type": "permission", "name": "edit", "displayName": "Edit a.rs",
                "description": null, "input": {}, "toolCallId": "c",
                "options": [{ "id": "o1", "label": "Allow", "description": null, "kind": "allowOnce" }] }),
            json!({ "type": "question", "toolCallId": "t", "questions": [{
                "id": "Which library?", "header": "Library", "question": "Which library?",
                "options": [{ "label": "chrono", "description": "Full", "preview": null }],
                "multiSelect": false, "allowOther": true, "secret": false }] }),
            json!({ "type": "planApproval", "plan": "# Plan", "path": "/p.md", "toolCallId": null,
                "options": [{ "id": "acceptEdits", "label": "Yes", "description": null, "kind": "allowOnce" }] }),
            json!({ "type": "form", "message": "Your name?", "source": "github", "url": null,
                "schema": {}, "toolCallId": null, "fields": [{
                    "name": "name", "kind": "text", "title": null, "description": null,
                    "required": true, "options": [], "format": null, "minimum": null,
                    "maximum": null, "default": null }] }),
        ];
        for kind in kinds {
            let request = json!({ "id": "r", "turnId": "t", "native": {}, "kind": kind });
            let stubb: stubb::request::Request = serde_json::from_value(request.clone()).unwrap();
            let sent = serde_json::to_value(&stubb).unwrap();
            assert_eq!(sent, request, "Stubb writes {kind}");
            let dto: RequestDto = serde_json::from_value(sent.clone()).unwrap();
            assert_eq!(serde_json::to_value(&dto).unwrap(), sent);
        }
    }

    #[test]
    fn an_answer_names_what_it_needs_and_a_plain_one_still_reads() {
        let plain: AnswerDto = serde_json::from_value(json!({ "decision": "allow" })).unwrap();
        assert_eq!(plain.decision, DecisionDto::Allow);
        assert!(plain.answers.is_empty() && plain.option_id.is_none());
        let answer: AnswerDto = serde_json::from_value(json!({
            "decision": "answer",
            "answers": [{ "questionId": "q", "selected": ["a"], "other": "b" }],
        }))
        .unwrap();
        assert_eq!(answer.answers[0].question_id, "q");
        let select: AnswerDto =
            serde_json::from_value(json!({ "decision": "select", "optionId": "acceptEdits" }))
                .unwrap();
        assert_eq!(select.option_id.as_deref(), Some("acceptEdits"));
        let decide = Decide {
            chat: Uuid::nil(),
            request: "r".to_owned(),
            decision: stubb::request::Decision::Deny {
                message: Some("No".to_owned()),
            },
        };
        assert_eq!(
            serde_json::to_value(&decide).unwrap()["decision"],
            json!({ "type": "deny", "message": "No" })
        );
    }
}
