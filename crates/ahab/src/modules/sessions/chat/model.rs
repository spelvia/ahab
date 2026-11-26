//! A session, and how the agent's events change it.

use std::path::Path;

use crate::modules::machines::Harness;
use serde_json::Value;
use stubb::session::Settings;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::sessions::chat::api::{
    AnswerDto, DecisionDto, EffortDto, FastModeDto, ModelDto,
};

use crate::shared::error::{DomainError, FieldError};

#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    pub id: Uuid,
    /// The session it belongs to.
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub machine_id: Uuid,
    /// `claude-code`, `codex` or `pi`.
    pub agent: String,
    /// The agent's own session ID, for resuming.
    pub native_id: Option<String>,
    /// The model it runs, as the agent names it.
    pub model: Option<String>,
    /// The reasoning effort, when the model has levels.
    pub effort: Option<String>,
    /// What the agent may do without asking (see [`mode`]); `None` for Pi,
    /// which has no modes.
    pub mode: Option<String>,
    pub title: String,
    pub state: ChatState,
    pub status: ChatStatus,
    /// The agent runs without asking (Pi).
    pub unrestricted: bool,
    /// The turn running now, to interrupt it.
    pub current_turn: Option<String>,
    /// Permission requests waiting for an answer, as Stubb sent them.
    pub open_requests: Vec<Value>,
    pub close_reason: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub closed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatState {
    /// Its agent runs.
    Live,
    /// Over; its history is kept.
    Closed,
}

impl ChatState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "live" => Some(Self::Live),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatus {
    Starting,
    /// A turn runs.
    Working,
    /// Waiting for the next message.
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionState {
    Queued,
    Sending,
    Sent,
    Unconfirmed,
    Withdrawn,
}

impl SubmissionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Unconfirmed => "unconfirmed",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "sending" => Some(Self::Sending),
            "sent" => Some(Self::Sent),
            "unconfirmed" => Some(Self::Unconfirmed),
            "withdrawn" => Some(Self::Withdrawn),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Submission {
    pub command: Option<super::api::CommandInvocation>,
    pub id: Uuid,
    pub fingerprint: String,
    pub state: SubmissionState,
    pub queued: bool,
    pub interrupt: bool,
    pub shown: String,
    pub prompt: String,
    pub attachments: Vec<Uuid>,
}

impl ChatStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Working => "working",
            Self::Idle => "idle",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "starting" => Some(Self::Starting),
            "working" => Some(Self::Working),
            "idle" => Some(Self::Idle),
            _ => None,
        }
    }
}

/// A session's title: the first line of its first message, at most 80
/// characters.
/// What an agent starts with unless chosen otherwise: the last model,
/// effort and permission mode picked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defaults {
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: Option<String>,
    pub updated_at: OffsetDateTime,
}

/// Claude Code's modes: plan only, ask before edits and commands (the
/// default), edit without asking, or never ask.
pub const CLAUDE_MODES: &[&str] = &["plan", "ask", "acceptEdits", "bypass"];
/// Codex's modes, its sandbox and when it asks: read only, asking to go
/// beyond it; the project's folder without network, asking before edits and
/// commands that are not plain reads (`ask`); the same folder, asking only to
/// go beyond it (the default); or everything, never asking.
pub const CODEX_MODES: &[&str] = &["readOnly", "ask", "workspace", "fullAccess"];

/// The mode an agent runs in: `wanted` if the agent has it, else its
/// default. A read-only project allows only modes that change nothing.
pub fn mode(
    agent: &str,
    wanted: Option<&str>,
    read_only: bool,
) -> Result<Option<String>, DomainError> {
    let invalid = |message: &str| DomainError::Invalid(vec![FieldError::new("mode", message)]);
    let (modes, default, safe): (&[&str], &str, &[&str]) = match agent {
        "claude-code" => (
            CLAUDE_MODES,
            if read_only { "plan" } else { "ask" },
            &["plan"],
        ),
        "codex" => (
            CODEX_MODES,
            if read_only { "readOnly" } else { "workspace" },
            &["readOnly"],
        ),
        _ => {
            return match wanted {
                None => Ok(None),
                Some(_) => Err(invalid("This agent has no permission modes.")),
            };
        }
    };
    let Some(wanted) = wanted else {
        return Ok(Some(default.to_owned()));
    };
    if !modes.contains(&wanted) {
        return Err(invalid(&format!("Choose one of: {}.", modes.join(", "))));
    }
    if read_only && !safe.contains(&wanted) {
        return Err(invalid("The project is read-only."));
    }
    Ok(Some(wanted.to_owned()))
}

/// A queued message's prompt after its text changed from `shown` to `text`:
/// the context appended to it (`reference::message`) is kept.
pub fn edited_prompt(shown: &str, prompt: &str, text: &str) -> String {
    let context = if shown.trim().is_empty() {
        prompt
    } else {
        prompt
            .strip_prefix(shown)
            .map_or("", |rest| rest.trim_start_matches('\n'))
    };
    match (text.trim().is_empty(), context.is_empty()) {
        (_, true) => text.to_owned(),
        (true, false) => context.to_owned(),
        (false, false) => format!("{text}\n\n{context}"),
    }
}

pub fn title(message: &str) -> String {
    let line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Attached files");
    if line.chars().count() <= 80 {
        line.to_owned()
    } else {
        let short: String = line.chars().take(79).collect();
        format!("{}…", short.trim_end())
    }
}

/// A message that a search found.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageHit {
    pub chat_id: Uuid,
    pub seq: i64,
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub session_title: String,
    pub role: MessageRole,
    pub text: String,
    pub created_at: OffsetDateTime,
}

/// Who wrote a message, for searching sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Agent,
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// A finished message in an event (`itemEnd` of a `userMessage` or an
/// `agentMessage`): who wrote it, and its text. Anything else, or a message
/// with no text, is none.
pub fn message_of(event: &Value) -> Option<(MessageRole, String)> {
    if event.get("type").and_then(Value::as_str) != Some("itemEnd") {
        return None;
    }
    let item = event.get("item")?;
    let (role, text) = match item.get("type").and_then(Value::as_str)? {
        "userMessage" => (
            MessageRole::User,
            item.get("content")?
                .as_array()?
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "agentMessage" => (MessageRole::Agent, item.get("text")?.as_str()?.to_owned()),
        _ => return None,
    };
    (!text.trim().is_empty()).then_some((role, text))
}

/// What a chat's agent spent, from its `turnAccounting` events in order:
/// tokens added up, and the cost. The agent reports its cost so far for its
/// process, which starts again from zero when the chat resumes; so a cost
/// lower than the one before adds on to what came before it.
pub fn usage_of(events: &[Value]) -> (Option<i64>, Option<f64>) {
    let mut tokens = None;
    let mut before = 0.0;
    let mut last: Option<f64> = None;
    for event in events {
        if event.get("type").and_then(Value::as_str) != Some("turnAccounting") {
            continue;
        }
        if let Some(count) = event.get("tokens").and_then(Value::as_i64) {
            tokens = Some(tokens.unwrap_or(0) + count);
        }
        if let Some(cost) = event
            .get("sessionCostUsd")
            .and_then(Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost >= 0.0)
        {
            if let Some(previous) = last
                && cost < previous
            {
                before += previous;
            }
            last = Some(cost);
        }
    }
    (tokens, last.map(|cost| before + cost))
}

/// Which submission each user message an agent echoes came from, for one
/// chat: a client shows a message as "sending" until the echo with its
/// submission ID arrives. Submissions are expected in the order they are
/// given to the agent, which echoes them in that order.
#[derive(Debug, Default)]
pub struct Echoes {
    /// Given to the agent, not echoed yet, oldest first; `true` once the
    /// agent took it.
    waiting: std::collections::VecDeque<(Uuid, bool)>,
    /// Echoed user messages still open (started, not ended): item → submission.
    open: std::collections::HashMap<String, Uuid>,
}

impl Echoes {
    /// A submission is about to be given to the agent.
    pub fn expect(&mut self, submission: Uuid) {
        if !self.waiting.iter().any(|(id, _)| *id == submission) {
            self.waiting.push_back((submission, false));
        }
    }

    /// The agent took it: if its turn ends without an echo, none comes.
    pub fn delivered(&mut self, submission: Uuid) {
        for (id, taken) in &mut self.waiting {
            if *id == submission {
                *taken = true;
            }
        }
    }

    /// It never reached the agent.
    pub fn forget(&mut self, submission: Uuid) {
        self.waiting.retain(|(id, _)| *id != submission);
    }

    /// Whether nothing is expected or open.
    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty() && self.open.is_empty()
    }

    /// Adds `submissionId` to a user message's `itemStart` or `itemEnd`
    /// that echoes an expected submission; a turn's end (or the agent
    /// stopping) forgets what the agent took but never echoed.
    pub fn tag(&mut self, event: &mut Value) {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if kind == "turnEnd" || kind == "closed" {
            self.waiting.retain(|(_, taken)| !taken);
            return;
        }
        if kind != "itemStart" && kind != "itemEnd" {
            return;
        }
        let Some(item) = event.get_mut("item").and_then(Value::as_object_mut) else {
            return;
        };
        if item.get("type").and_then(Value::as_str) != Some("userMessage") {
            return;
        }
        let key = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let known = if kind == "itemEnd" {
            self.open.remove(&key)
        } else {
            self.open.get(&key).copied()
        };
        let Some(submission) = known.or_else(|| {
            let (next, _) = self.waiting.pop_front()?;
            if kind == "itemStart" {
                self.open.insert(key, next);
            }
            Some(next)
        }) else {
            return;
        };
        item.insert(
            "submissionId".to_owned(),
            Value::from(submission.to_string()),
        );
    }
}

/// The words a search asks for, lowercased: each must appear. At most 8,
/// each at most 100 characters.
pub fn search_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|word| word.to_lowercase().chars().take(100).collect::<String>())
        .filter(|word| !word.is_empty())
        .take(8)
        .collect()
}

/// A short part of `text` around the first of `terms` it holds: about
/// `width` characters, on word boundaries, with "…" where it was cut.
pub fn snippet(text: &str, terms: &[String], width: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() <= width {
        return flat;
    }
    let lower: Vec<char> = flat.to_lowercase().chars().collect();
    // Lowercasing may change the length (rarely): then from the start.
    let at = if lower.len() == chars.len() {
        terms
            .iter()
            .filter_map(|term| find_chars(&lower, &term.chars().collect::<Vec<_>>()))
            .min()
            .unwrap_or(0)
    } else {
        0
    };
    let mut start = at.saturating_sub(width / 3);
    let mut end = (start + width).min(chars.len());
    start = end.saturating_sub(width).min(start);
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    let mut out: String = chars[start..end].iter().collect();
    out = out.trim().to_owned();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

fn find_chars(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&at| haystack[at..at + needle.len()] == *needle)
}

/// Whether an event is kept: everything but streamed text and unmapped
/// provider traffic, which only go to clients watching live.
pub fn is_lasting(event: &Value) -> bool {
    !matches!(
        event.get("type").and_then(Value::as_str),
        Some("itemDelta" | "raw")
    )
}

impl Chat {
    /// Applies one of the agent's events (Stubb's `session::Event`, as JSON);
    /// whether anything the list shows changed.
    pub fn apply(&mut self, event: &Value, now: OffsetDateTime) -> bool {
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        let changed = match kind {
            "turnStart" => {
                self.current_turn = event
                    .get("turnId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.status = ChatStatus::Working;
                true
            }
            "turnEnd" => {
                self.current_turn = None;
                self.status = ChatStatus::Idle;
                // An interrupted turn withdraws its requests.
                self.open_requests.clear();
                true
            }
            "status" => match event.get("status").and_then(Value::as_str) {
                Some("idle") if self.current_turn.is_none() => {
                    self.status = ChatStatus::Idle;
                    true
                }
                Some("active") => {
                    self.status = ChatStatus::Working;
                    true
                }
                _ => false,
            },
            "request" => {
                if let Some(request) = event.get("request") {
                    self.open_requests.push(request.clone());
                }
                true
            }
            "requestResolved" => {
                let id = event.get("id").and_then(Value::as_str);
                self.open_requests
                    .retain(|request| request.get("id").and_then(Value::as_str) != id);
                true
            }
            "closed" => {
                self.close(close_reason(event.get("reason")), now);
                true
            }
            _ => false,
        };
        if changed {
            self.updated_at = now;
        }
        changed
    }

    /// Runs again after its agent stopped (resumed).
    pub fn reopen(&mut self, now: OffsetDateTime) {
        self.state = ChatState::Live;
        self.status = ChatStatus::Starting;
        self.current_turn = None;
        self.open_requests.clear();
        self.close_reason = None;
        self.closed_at = None;
        self.updated_at = now;
    }

    pub fn close(&mut self, reason: String, now: OffsetDateTime) {
        if self.state == ChatState::Closed {
            return;
        }
        self.state = ChatState::Closed;
        self.status = ChatStatus::Idle;
        self.current_turn = None;
        self.open_requests.clear();
        self.close_reason = Some(reason);
        self.closed_at = Some(now);
        self.updated_at = now;
    }
}

/// How a chat that stopped unexpectedly is closed ([`transition`] tells it
/// from a chat someone closed).
const STOPPED_UNEXPECTEDLY: &str = "The agent stopped unexpectedly";
/// How a chat whose machine restarted under it is closed.
pub const MACHINE_RESTARTED: &str = "Ahab on its machine restarted, which stopped the agent.";

/// What a change to a chat means for the person following it (its
/// session notifications).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// A turn ended and nothing waits for an answer. The agent may still
    /// start another turn at once, so whoever notifies checks again shortly.
    Finished,
    /// It asks something (a permission request), and waits.
    NeedsYou { request: Value },
    /// It stopped unexpectedly, or with its machine.
    Failed { reason: String },
}

/// What changed from `before` to `after`, if it is something to tell.
pub fn transition(before: &Chat, after: &Chat) -> Option<Transition> {
    if before.state == ChatState::Live && after.state == ChatState::Closed {
        let reason = after.close_reason.clone().unwrap_or_default();
        let failed = reason.starts_with(STOPPED_UNEXPECTEDLY) || reason == MACHINE_RESTARTED;
        return failed.then_some(Transition::Failed { reason });
    }
    if after.state != ChatState::Live {
        return None;
    }
    if after.open_requests.len() > before.open_requests.len() {
        let request = after.open_requests.last().cloned().unwrap_or(Value::Null);
        return Some(Transition::NeedsYou { request });
    }
    let ended = before.status == ChatStatus::Working && after.status == ChatStatus::Idle;
    (ended && after.open_requests.is_empty()).then_some(Transition::Finished)
}

fn close_reason(reason: Option<&Value>) -> String {
    match reason.and_then(|r| r.get("type")).and_then(Value::as_str) {
        Some("agent") => "The agent ended the session.".to_owned(),
        Some("exited") => format!(
            "{STOPPED_UNEXPECTEDLY}: {}",
            reason
                .and_then(|r| r.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("no reason given")
        ),
        // Closed by the client, or for no reason given.
        _ => "Closed.".to_owned(),
    }
}

/// The files a session's agents edited, from its finished items: every
/// completed file change's path (and where a file moved to), each once,
/// sorted. Paths are as the agent gave them, usually absolute.
pub fn edited_files(items: &[Value]) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::new();
    for event in items {
        let item = &event["item"];
        if item["type"] != "toolCall" || item["status"] != "completed" {
            continue;
        }
        if item["detail"]["kind"] == "notebookEdit" {
            if let Some(path) = item["detail"]["path"].as_str() {
                paths.insert(path.to_owned());
            }
            continue;
        }
        if item["detail"]["kind"] != "fileChange" {
            continue;
        }
        let Some(changes) = item["detail"]["changes"].as_array() else {
            continue;
        };
        for change in changes {
            if let Some(path) = change["path"].as_str() {
                paths.insert(path.to_owned());
            }
            if let Some(moved) = change["kind"]["movePath"].as_str() {
                paths.insert(moved.to_owned());
            }
        }
    }
    paths.into_iter().collect()
}

/// What an agent may do without asking, in Stubb's terms, from Ahab's mode
/// (`chat::model::mode`). Claude Code's and Codex's requests are forwarded
/// to Ahab; Codex's mode is its sandbox and its approval policy.
pub fn permissions(
    harness: Harness,
    cwd: &Path,
    read_only: bool,
    mode: Option<&str>,
) -> Option<stubb::permission::Configuration> {
    match harness {
        Harness::Claude => {
            use stubb::providers::claude::PermissionMode;
            let mode = match mode {
                Some("plan") => PermissionMode::Plan,
                Some("acceptEdits") if !read_only => PermissionMode::AcceptEdits,
                Some("bypass") if !read_only => PermissionMode::BypassPermissions,
                // A read-only project plans.
                _ if read_only => PermissionMode::Plan,
                _ => PermissionMode::Default,
            };
            Some(stubb::permission::Configuration::new(
                &stubb::providers::claude::Permissions { mode: Some(mode) },
            ))
        }
        Harness::Codex => {
            use stubb::providers::codex::{ApprovalPolicy, Sandbox};
            let workspace = || Sandbox::WorkspaceWrite {
                writable_roots: Some(vec![cwd.to_path_buf()]),
                network_access: Some(false),
                exclude_tmpdir_env_var: None,
                exclude_slash_tmp: None,
            };
            let (sandbox, approval_policy) = match mode {
                // A read-only project never lets it ask to write.
                _ if read_only => (Sandbox::ReadOnly, ApprovalPolicy::Never),
                Some("readOnly") => (Sandbox::ReadOnly, ApprovalPolicy::OnRequest),
                Some("ask") => (workspace(), ApprovalPolicy::Untrusted),
                Some("fullAccess") => (Sandbox::DangerFullAccess, ApprovalPolicy::Never),
                _ => (workspace(), ApprovalPolicy::OnRequest),
            };
            Some(stubb::permission::Configuration::new(
                &stubb::providers::codex::Permissions {
                    approval_policy: Some(approval_policy),
                    sandbox: Some(sandbox),
                    profile: None,
                },
            ))
        }
        // Pi has no permission modes.
        Harness::Pi => None,
    }
}

pub fn settings(harness: Harness, cwd: &Path, read_only: bool, mode: Option<&str>) -> Settings {
    Settings {
        // Pi asks nothing.
        forward_requests: harness != Harness::Pi,
        permissions: permissions(harness, cwd, read_only, mode),
        ..Settings::default()
    }
}

/// An agent's models from its catalog (Stubb's `Catalog`, as JSON).
pub fn parse_catalog(catalog: &Value) -> Vec<ModelDto> {
    let text = |value: &Value, field: &str| {
        value
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    catalog
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .map(|model| ModelDto {
                    id: text(model, "id"),
                    label: text(model, "label"),
                    description: text(model, "description"),
                    is_default: model
                        .get("isDefault")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    efforts: model
                        .get("efforts")
                        .and_then(Value::as_array)
                        .map(|efforts| {
                            efforts
                                .iter()
                                .map(|effort| EffortDto {
                                    id: text(effort, "id"),
                                    description: text(effort, "description"),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    default_effort: model
                        .get("defaultEffort")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    fast: model
                        .get("fastMode")
                        .filter(|fast| {
                            fast.get("status").and_then(Value::as_str) == Some("available")
                        })
                        .map(|fast| FastModeDto {
                            label: text(fast, "label"),
                            description: text(fast, "description"),
                            default_on: fast
                                .get("defaultOn")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        }),
                })
                .filter(|model| !model.id.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Ahab's mode for the Claude Code permission mode a plan is approved
/// with (`ExitPlanMode`'s options), when it is one of Ahab's.
pub fn plan_mode(option: &str) -> Option<&'static str> {
    match option {
        "acceptEdits" => Some("acceptEdits"),
        "default" => Some("ask"),
        "bypassPermissions" => Some("bypass"),
        "plan" => Some("plan"),
        _ => None,
    }
}

/// The decision `answer` makes for `request` (a pending request, as the
/// agent sent it), checked against what the request asks; else why it does
/// not answer it. A kind this version does not know takes the decisions of
/// a tool permission, and the agent judges them.
pub fn decision(request: &Value, answer: &AnswerDto) -> Result<stubb::request::Decision, String> {
    use stubb::request::Decision;
    let kind = &request["kind"];
    let kind_name = kind["type"].as_str().unwrap_or_default();
    let options = |field: &str| -> Vec<&str> {
        kind[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| option["id"].as_str())
            .collect()
    };
    let only = |kinds: &[&str], what: &str| {
        if kinds.contains(&kind_name) {
            Ok(())
        } else {
            Err(format!("Only {what} take this answer."))
        }
    };
    match answer.decision {
        DecisionDto::Allow => {
            match kind_name {
                "question" => return Err("Answer the questions.".to_owned()),
                "form" => return Err("Fill in the form or decline it.".to_owned()),
                _ => {}
            }
            Ok(Decision::Allow)
        }
        DecisionDto::AllowForSession => {
            let offered = match kind_name {
                "tool" => kind["canAllowForSession"] == true,
                "permission" => kind["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|option| option["kind"] == "allowAlways"),
                "question" | "planApproval" | "form" => false,
                _ => true,
            };
            if offered {
                Ok(Decision::AllowForSession)
            } else {
                Err("This request cannot be allowed for the session.".to_owned())
            }
        }
        DecisionDto::Deny => Ok(Decision::Deny {
            message: answer
                .message
                .as_deref()
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_owned),
        }),
        DecisionDto::Cancel => Ok(Decision::Cancel),
        DecisionDto::Select => {
            only(
                &["permission", "planApproval"],
                "permissions with options and plans",
            )?;
            let Some(id) = answer.option_id.as_deref() else {
                return Err("Pick an option.".to_owned());
            };
            if !options("options").contains(&id) {
                return Err(format!("The request offers no option {id:?}."));
            }
            Ok(Decision::Select {
                option_id: id.to_owned(),
            })
        }
        DecisionDto::Answer => {
            only(&["question"], "questions")?;
            answers(&kind["questions"], answer).map(|answers| Decision::Answer { answers })
        }
        DecisionDto::Submit => {
            only(&["form"], "forms")?;
            let values = answer.values.clone().unwrap_or_default();
            if kind["url"].is_null() {
                for field in kind["fields"].as_array().into_iter().flatten() {
                    let name = field["name"].as_str().unwrap_or_default();
                    let missing = match values.get(name) {
                        None | Some(Value::Null) => true,
                        Some(Value::String(text)) => text.trim().is_empty(),
                        Some(_) => false,
                    };
                    if field["required"] == true && missing {
                        let title = field["title"].as_str().unwrap_or(name);
                        return Err(format!("Fill in {title}."));
                    }
                }
            }
            Ok(Decision::Submit { values })
        }
    }
}

/// An answer for each of `questions`, each checked against its question.
fn answers(questions: &Value, answer: &AnswerDto) -> Result<Vec<stubb::request::Answer>, String> {
    let questions = questions.as_array().map(Vec::as_slice).unwrap_or_default();
    for given in &answer.answers {
        if !questions
            .iter()
            .any(|question| question["id"].as_str() == Some(given.question_id.as_str()))
        {
            return Err(format!("There is no question {:?}.", given.question_id));
        }
    }
    questions
        .iter()
        .map(|question| {
            let id = question["id"].as_str().unwrap_or_default();
            let text = question["question"].as_str().unwrap_or(id);
            let given = answer
                .answers
                .iter()
                .find(|given| given.question_id == id)
                .ok_or_else(|| format!("Answer {text:?}."))?;
            let labels: Vec<&str> = question["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|option| option["label"].as_str())
                .collect();
            if let Some(unknown) = given
                .selected
                .iter()
                .find(|label| !labels.contains(&label.as_str()))
            {
                return Err(format!("{text:?} has no option {unknown:?}."));
            }
            if given.selected.len() > 1 && question["multiSelect"] != true {
                return Err(format!("Pick one answer to {text:?}."));
            }
            let other = given
                .other
                .as_deref()
                .filter(|other| !other.trim().is_empty());
            if other.is_some() && question["allowOther"] != true && !labels.is_empty() {
                return Err(format!("{text:?} takes only its options."));
            }
            if given.selected.is_empty() && other.is_none() {
                return Err(format!("Answer {text:?}."));
            }
            let picked = stubb::request::Answer::selected(id, given.selected.iter().cloned());
            Ok(match other {
                Some(other) => picked.with_other(other),
                None => picked,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_edited_queued_message_keeps_its_context() {
        let context = "<file path=\"a.rs\">x</file>";
        let prompt = format!("Fix it\n\n{context}");
        assert_eq!(
            edited_prompt("Fix it", &prompt, "Fix it properly"),
            format!("Fix it properly\n\n{context}")
        );
        assert_eq!(
            edited_prompt("", context, "Look"),
            format!("Look\n\n{context}")
        );
        assert_eq!(edited_prompt("Fix it", "Fix it", "Other"), "Other");
        assert_eq!(edited_prompt("Fix it", &prompt, " "), context);
    }

    fn answer(
        decision: crate::modules::sessions::chat::api::DecisionDto,
    ) -> crate::modules::sessions::chat::api::AnswerDto {
        crate::modules::sessions::chat::api::AnswerDto {
            decision,
            message: None,
            answers: Vec::new(),
            option_id: None,
            values: None,
        }
    }

    #[test]
    fn answers_are_checked_against_what_the_request_asks() {
        use crate::modules::sessions::chat::api::{DecisionDto as D, QuestionAnswerDto};
        use stubb::request::Decision;
        let tool = json!({ "id": "r", "kind": { "type": "tool", "name": "Bash", "canAllowForSession": false } });
        assert_eq!(
            super::decision(&tool, &answer(D::Allow)),
            Ok(Decision::Allow)
        );
        assert!(super::decision(&tool, &answer(D::AllowForSession)).is_err());
        assert!(super::decision(&tool, &answer(D::Answer)).is_err());
        let deny = crate::modules::sessions::chat::api::AnswerDto {
            message: Some("  not now ".to_owned()),
            ..answer(D::Deny)
        };
        assert_eq!(
            super::decision(&tool, &deny),
            Ok(Decision::Deny {
                message: Some("not now".to_owned())
            })
        );
        assert_eq!(
            super::decision(&tool, &answer(D::Cancel)),
            Ok(Decision::Cancel)
        );

        let question = json!({ "id": "r", "kind": { "type": "question", "questions": [
            { "id": "lib", "question": "Which library?", "multiSelect": false, "allowOther": true,
              "options": [{ "label": "chrono" }, { "label": "time" }] },
            { "id": "token", "question": "Your token?", "options": [], "secret": true },
        ] } });
        let answered = |answers: Vec<QuestionAnswerDto>| {
            super::decision(
                &question,
                &crate::modules::sessions::chat::api::AnswerDto {
                    answers,
                    ..answer(D::Answer)
                },
            )
        };
        let given = |id: &str, selected: &[&str], other: Option<&str>| QuestionAnswerDto {
            question_id: id.to_owned(),
            selected: selected.iter().map(|label| (*label).to_owned()).collect(),
            other: other.map(str::to_owned),
        };
        let Ok(Decision::Answer { answers }) = answered(vec![
            given("lib", &["time"], Some("but pinned")),
            given("token", &[], Some("s3cret")),
        ]) else {
            panic!("a full answer is an answer");
        };
        assert_eq!(
            serde_json::to_value(&answers).unwrap(),
            json!([
                { "questionId": "lib", "selected": ["time"], "other": "but pinned" },
                { "questionId": "token", "selected": [], "other": "s3cret" },
            ])
        );
        assert!(
            answered(vec![given("lib", &["time"], None)]).is_err(),
            "every question"
        );
        assert!(
            answered(vec![
                given("lib", &["moment"], None),
                given("token", &[], Some("x"))
            ])
            .is_err()
        );
        assert!(
            answered(vec![
                given("lib", &["time", "chrono"], None),
                given("token", &[], Some("x"))
            ])
            .is_err(),
            "one only"
        );
        assert!(
            answered(vec![
                given("lib", &[], Some(" ")),
                given("token", &[], Some("x"))
            ])
            .is_err(),
            "blank"
        );
        assert!(
            answered(vec![
                given("lib", &["time"], None),
                given("token", &[], Some("x")),
                given("nope", &[], Some("x"))
            ])
            .is_err()
        );
        assert!(super::decision(&question, &answer(D::Allow)).is_err());

        let plan = json!({ "id": "r", "kind": { "type": "planApproval", "plan": "# Plan",
            "options": [{ "id": "acceptEdits", "label": "Yes", "kind": "allowOnce" }] } });
        let select = |id: &str| crate::modules::sessions::chat::api::AnswerDto {
            option_id: Some(id.to_owned()),
            ..answer(D::Select)
        };
        assert_eq!(
            super::decision(&plan, &select("acceptEdits")),
            Ok(Decision::Select {
                option_id: "acceptEdits".to_owned()
            })
        );
        assert!(super::decision(&plan, &select("bypass")).is_err());
        assert!(
            super::decision(&plan, &answer(D::Select)).is_err(),
            "no option"
        );
        assert_eq!(
            super::decision(&plan, &answer(D::Allow)),
            Ok(Decision::Allow)
        );
        assert!(super::decision(&tool, &select("acceptEdits")).is_err());

        let permission = json!({ "id": "r", "kind": { "type": "permission", "name": "edit",
            "options": [{ "id": "always", "label": "Always", "kind": "allowAlways" }] } });
        assert_eq!(
            super::decision(&permission, &answer(D::AllowForSession)),
            Ok(Decision::AllowForSession)
        );

        let form = json!({ "id": "r", "kind": { "type": "form", "message": "Details", "url": null,
            "fields": [{ "name": "email", "title": "Email", "kind": "text", "required": true },
                       { "name": "age", "kind": "integer", "required": false }] } });
        let submit = |values: Value| crate::modules::sessions::chat::api::AnswerDto {
            values: values.as_object().cloned(),
            ..answer(D::Submit)
        };
        assert_eq!(
            super::decision(&form, &submit(json!({ "email": "a@b.c" }))),
            Ok(Decision::Submit {
                values: json!({ "email": "a@b.c" }).as_object().cloned().unwrap()
            })
        );
        assert_eq!(
            super::decision(&form, &submit(json!({ "email": "" }))),
            Err("Fill in Email.".to_owned())
        );
        assert!(super::decision(&form, &answer(D::Allow)).is_err());
        let page = json!({ "id": "r", "kind": { "type": "form", "message": "Sign in", "url": "https://x", "fields": [] } });
        assert!(super::decision(&page, &answer(D::Submit)).is_ok());

        // A kind this version does not know takes a permission's answers.
        let newer = json!({ "id": "r", "kind": { "type": "somethingNew" } });
        assert_eq!(
            super::decision(&newer, &answer(D::Allow)),
            Ok(Decision::Allow)
        );
    }

    #[test]
    fn edited_files_come_from_completed_file_changes() {
        let change = |status: &str, path: &str| {
            serde_json::json!({
                "type": "itemEnd",
                "item": {"type": "toolCall", "status": status, "detail": {
                    "kind": "fileChange",
                    "changes": [{"path": path, "kind": {"type": "update"}, "diff": ""}],
                }},
            })
        };
        let moved = serde_json::json!({
            "type": "itemEnd",
            "item": {"type": "toolCall", "status": "completed", "detail": {
                "kind": "fileChange",
                "changes": [{"path": "/w/old.rs", "kind": {"type": "update", "movePath": "/w/new.rs"}, "diff": ""}],
            }},
        });
        let command = serde_json::json!({
            "type": "itemEnd",
            "item": {"type": "toolCall", "status": "completed", "detail": {"kind": "command", "command": "ls"}},
        });
        let items = [
            change("completed", "/w/b.rs"),
            change("declined", "/w/declined.rs"),
            change("completed", "/w/a.rs"),
            change("completed", "/w/b.rs"),
            moved,
            command,
        ];
        assert_eq!(
            edited_files(&items),
            ["/w/a.rs", "/w/b.rs", "/w/new.rs", "/w/old.rs"]
        );
    }

    use serde_json::json;

    use super::*;

    fn session() -> Chat {
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
            state: ChatState::Live,
            status: ChatStatus::Starting,
            unrestricted: false,
            current_turn: None,
            open_requests: Vec::new(),
            close_reason: None,
            created_at: now,
            updated_at: now,
            closed_at: None,
        }
    }

    /// `chat` after `event`, and what that tells.
    fn after(chat: &Chat, event: serde_json::Value) -> (Chat, Option<Transition>) {
        let mut next = chat.clone();
        next.apply(&event, OffsetDateTime::now_utc());
        let told = transition(chat, &next);
        (next, told)
    }

    #[test]
    fn a_turn_that_ends_with_nothing_waiting_is_finished() {
        let (working, told) = after(
            &session(),
            serde_json::json!({"type": "turnStart", "turnId": "t1"}),
        );
        assert_eq!(told, None);
        let (_, told) = after(&working, serde_json::json!({"type": "turnEnd"}));
        assert_eq!(told, Some(Transition::Finished));
    }

    #[test]
    fn a_question_needs_its_person_once() {
        let (working, _) = after(
            &session(),
            serde_json::json!({"type": "turnStart", "turnId": "t1"}),
        );
        let request = serde_json::json!({"id": "r1", "title": "Run cargo test?"});
        let (asking, told) = after(
            &working,
            serde_json::json!({"type": "request", "request": request.clone()}),
        );
        assert_eq!(told, Some(Transition::NeedsYou { request }));
        // Answered: nothing new to tell.
        let (_, told) = after(
            &asking,
            serde_json::json!({"type": "requestResolved", "id": "r1"}),
        );
        assert_eq!(told, None);
    }

    #[test]
    fn stopping_unexpectedly_fails_but_closing_does_not() {
        let crashed = serde_json::json!({"type": "closed", "reason": {"type": "exited", "message": "killed"}});
        let (_, told) = after(&session(), crashed);
        assert!(
            matches!(told, Some(Transition::Failed { ref reason }) if reason.ends_with("killed"))
        );
        let (_, told) = after(&session(), serde_json::json!({"type": "closed"}));
        assert_eq!(told, None);
        let mut restarted = session();
        restarted.close(MACHINE_RESTARTED.to_owned(), OffsetDateTime::now_utc());
        assert!(matches!(
            transition(&session(), &restarted),
            Some(Transition::Failed { .. })
        ));
    }

    #[test]
    fn messages_are_found_in_finished_items_only() {
        let user = json!({ "type": "itemEnd", "item": { "type": "userMessage", "content": [
            { "type": "text", "text": "Fix the" }, { "type": "image" }, { "type": "text", "text": "build" }
        ] } });
        assert_eq!(
            message_of(&user),
            Some((MessageRole::User, "Fix the\nbuild".to_owned()))
        );
        let agent =
            json!({ "type": "itemEnd", "item": { "type": "agentMessage", "text": "Done." } });
        assert_eq!(
            message_of(&agent),
            Some((MessageRole::Agent, "Done.".to_owned()))
        );
        for other in [
            json!({ "type": "itemDelta", "item": { "type": "agentMessage", "text": "Do" } }),
            json!({ "type": "itemEnd", "item": { "type": "toolCall", "title": "ls" } }),
            json!({ "type": "itemEnd", "item": { "type": "agentMessage", "text": "  " } }),
        ] {
            assert_eq!(message_of(&other), None);
        }
    }

    #[test]
    fn search_terms_are_lowercased_words() {
        assert_eq!(
            search_terms("  Flaky  CI\ttests "),
            ["flaky", "ci", "tests"]
        );
        assert!(search_terms("   ").is_empty());
        assert_eq!(search_terms("a b c d e f g h i j").len(), 8);
    }

    #[test]
    fn snippets_cut_around_the_first_match_on_word_boundaries() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        assert_eq!(
            snippet("short text", &["text".to_owned()], 40),
            "short text"
        );
        let cut = snippet(text, &["nine".to_owned()], 20);
        assert!(cut.starts_with('…') && cut.ends_with('…'), "{cut}");
        assert!(cut.contains("nine"), "{cut}");
        assert!(!cut.contains("one two"), "{cut}");
        assert_eq!(
            snippet(text, &["nothing".to_owned()], 20).chars().next(),
            Some('o')
        );
    }

    #[test]
    fn modes_default_per_agent_and_respect_read_only_projects() {
        assert_eq!(
            mode("claude-code", None, false).unwrap().as_deref(),
            Some("ask")
        );
        assert_eq!(
            mode("codex", None, false).unwrap().as_deref(),
            Some("workspace")
        );
        assert_eq!(
            mode("codex", None, true).unwrap().as_deref(),
            Some("readOnly")
        );
        assert_eq!(
            mode("claude-code", Some("bypass"), false)
                .unwrap()
                .as_deref(),
            Some("bypass")
        );
        assert!(mode("claude-code", Some("fullAccess"), false).is_err());
        assert!(mode("codex", Some("fullAccess"), true).is_err());
        assert_eq!(mode("pi", None, false).unwrap(), None);
        assert!(mode("pi", Some("ask"), false).is_err());
    }

    #[test]
    fn titles_come_from_the_first_line() {
        assert_eq!(title("\n  Fix the flaky test\nmore"), "Fix the flaky test");
        assert_eq!(title(""), "Attached files");
        assert_eq!(title(&"x".repeat(100)).chars().count(), 80);
    }

    #[test]
    fn turns_requests_and_closing_change_the_state() {
        let mut s = session();
        let now = OffsetDateTime::now_utc();
        s.apply(&json!({ "type": "turnStart", "turnId": "t1" }), now);
        assert_eq!(s.status, ChatStatus::Working);
        assert_eq!(s.current_turn.as_deref(), Some("t1"));

        s.apply(
            &json!({ "type": "request", "request": { "id": "r1", "kind": { "type": "tool" } } }),
            now,
        );
        assert_eq!(s.open_requests.len(), 1);
        s.apply(
            &json!({ "type": "requestResolved", "id": "r1", "outcome": { "type": "withdrawn" } }),
            now,
        );
        assert!(s.open_requests.is_empty());

        // A stray idle status during a turn does not end it.
        assert!(!s.apply(&json!({ "type": "status", "status": "idle" }), now));
        s.apply(&json!({ "type": "turnEnd", "turn": {} }), now);
        assert_eq!(s.status, ChatStatus::Idle);
        assert_eq!(s.current_turn, None);

        s.apply(
            &json!({ "type": "closed", "reason": { "type": "exited", "message": "boom" } }),
            now,
        );
        assert_eq!(s.state, ChatState::Closed);
        assert_eq!(
            s.close_reason.as_deref(),
            Some("The agent stopped unexpectedly: boom")
        );
        assert!(!s.apply(&json!({ "type": "itemEnd" }), now));
    }

    #[test]
    fn streamed_text_is_not_kept() {
        assert!(!is_lasting(&json!({ "type": "itemDelta" })));
        assert!(is_lasting(&json!({ "type": "itemEnd" })));
    }

    #[test]
    fn usage_adds_up_turns_and_carries_cost_across_resumes() {
        let turn = |tokens: Option<i64>, cost: Option<f64>| json!({ "type": "turnAccounting", "turnId": "t", "tokens": tokens, "sessionCostUsd": cost });
        assert_eq!(super::usage_of(&[]), (None, None));
        let (tokens, cost) = super::usage_of(&[
            turn(Some(100), Some(0.5)),
            json!({ "type": "turnEnd" }),
            turn(Some(50), Some(1.25)),
            // Resumed: its process counts from zero again.
            turn(None, Some(0.25)),
        ]);
        assert_eq!(tokens, Some(150));
        assert_eq!(cost, Some(1.5));
        assert_eq!(super::usage_of(&[turn(Some(7), None)]), (Some(7), None));
    }

    #[test]
    fn echoed_user_messages_carry_their_submission() {
        use super::Echoes;
        let first = uuid::Uuid::now_v7();
        let second = uuid::Uuid::now_v7();
        let mut echoes = Echoes::default();
        let user = |kind: &str, id: &str| json!({ "type": kind, "turnId": "t", "item": { "type": "userMessage", "id": id, "content": [] } });
        let tagged =
            |event: &serde_json::Value| event["item"]["submissionId"].as_str().map(str::to_owned);

        // Nothing expected: an agent's own user message stays as it is.
        let mut own = user("itemEnd", "u0");
        echoes.tag(&mut own);
        assert_eq!(tagged(&own), None);

        echoes.expect(first);
        echoes.expect(second);
        let mut start = user("itemStart", "u1");
        echoes.tag(&mut start);
        let mut answer = json!({ "type": "itemEnd", "item": { "type": "agentMessage", "id": "a", "text": "hi" } });
        echoes.tag(&mut answer);
        assert!(answer["item"].get("submissionId").is_none());
        let mut end = user("itemEnd", "u1");
        echoes.tag(&mut end);
        assert_eq!(tagged(&start), Some(first.to_string()));
        assert_eq!(tagged(&end), Some(first.to_string()));

        // One that never reached the agent is not matched to the next echo.
        echoes.forget(second);
        let third = uuid::Uuid::now_v7();
        echoes.expect(third);
        let mut next = user("itemEnd", "u2");
        echoes.tag(&mut next);
        assert_eq!(tagged(&next), Some(third.to_string()));

        // One the agent took but whose turn ended without an echo is dropped.
        let lost = uuid::Uuid::now_v7();
        echoes.expect(lost);
        echoes.delivered(lost);
        echoes.tag(&mut json!({ "type": "turnEnd", "turn": {} }));
        let mut later = user("itemEnd", "u3");
        echoes.tag(&mut later);
        assert_eq!(tagged(&later), None);
        assert!(echoes.is_empty());
    }
}
