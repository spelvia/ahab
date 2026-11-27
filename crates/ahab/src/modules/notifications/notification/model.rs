//! What a notification is, its kinds, and the server's settings.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::sessions::{Chat, Transition};
use crate::shared::error::FieldError;

/// How important a notification is; it picks its icon and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// A level read back; an unknown one is shown as information.
    pub fn parse(value: &str) -> Self {
        match value {
            "success" => Self::Success,
            "warning" => Self::Warning,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

/// A kind of notification, as Settings → Notifications lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

pub const FINISHED: &str = "sessions.finished";
pub const NEEDS_YOU: &str = "sessions.needs-you";
pub const FAILED: &str = "sessions.failed";
pub const MACHINE_OFFLINE: &str = "machines.offline";
pub const MACHINE_BEHIND: &str = "machines.behind";
pub const UPDATE_AVAILABLE: &str = "ahab.update-available";

/// Ahab's own kinds.
pub const KINDS: [Kind; 6] = [
    Kind {
        id: FINISHED,
        title: "An agent finished",
        description: "Its turn ended and it waits for your next message.",
    },
    Kind {
        id: NEEDS_YOU,
        title: "An agent needs you",
        description: "It asks a question or for permission, and waits.",
    },
    Kind {
        id: FAILED,
        title: "An agent stopped",
        description: "It stopped unexpectedly, or with its machine.",
    },
    Kind {
        id: MACHINE_OFFLINE,
        title: "A machine went offline",
        description: "With sessions or terminals running on it, for more than a minute.",
    },
    Kind {
        id: MACHINE_BEHIND,
        title: "A machine runs an older Ahab",
        description: "Parts of Ahab do not work on it until it is updated.",
    },
    Kind {
        id: UPDATE_AVAILABLE,
        title: "A new Ahab is available",
        description: "A newer release of Ahab came out.",
    },
];

/// Something that happened, kept for the inbox.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: Uuid,
    pub kind: String,
    /// `ahab`, or a plugin's ID.
    pub source: String,
    pub level: Level,
    pub title: String,
    pub body: Option<String>,
    pub project_id: Option<Uuid>,
    /// What opening it shows (`{"type": "session", …}`).
    pub target: Value,
    /// Unread notifications of a group: the newest stands for them all.
    pub group_key: Option<String>,
    pub created_at: OffsetDateTime,
    pub read_at: Option<OffsetDateTime>,
}

/// A notification to send.
#[derive(Debug, Clone, PartialEq)]
pub struct NewNotification {
    pub kind: String,
    pub source: String,
    pub level: Level,
    pub title: String,
    pub body: Option<String>,
    pub project_id: Option<Uuid>,
    pub target: Value,
    pub group_key: Option<String>,
}

/// How long read notifications are kept unless changed.
pub const DEFAULT_KEEP_DAYS: u32 = 30;
/// The longest a person can choose to keep them for, short of forever.
pub const MAX_KEEP_DAYS: u32 = 3650;

/// The server's notification settings (one person per server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Notifications at all.
    pub enabled: bool,
    /// Days read notifications are kept; `None` keeps them forever.
    pub keep_days: Option<u32>,
    /// Kinds turned off (`false`) or on; absent kinds are on.
    pub kinds: BTreeMap<String, bool>,
    /// Shown outside the app too: the browser's, the desktop's, the phone's.
    pub outside: bool,
    /// Kinds shown only inside the app, whatever `outside` says.
    pub inside_only: BTreeSet<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_days: Some(DEFAULT_KEEP_DAYS),
            kinds: BTreeMap::new(),
            outside: true,
            inside_only: BTreeSet::new(),
        }
    }
}

impl Settings {
    /// Whether a notification of `kind` is sent at all.
    pub fn sends(&self, kind: &str) -> bool {
        self.enabled && self.kinds.get(kind).copied().unwrap_or(true)
    }

    /// Whether a notification of `kind` is shown outside the app too (the
    /// system's notifications, phones).
    pub fn goes_outside(&self, kind: &str) -> bool {
        self.outside && self.sends(kind) && !self.inside_only.contains(kind)
    }

    pub fn check(&self) -> Result<(), Vec<FieldError>> {
        match self.keep_days {
            Some(days) if days == 0 || days > MAX_KEEP_DAYS => Err(vec![FieldError::new(
                "keepDays",
                format!("Between 1 and {MAX_KEEP_DAYS} days, or none to keep them."),
            )]),
            _ => Ok(()),
        }
    }
}

/// An agent's name as people know it; for an ACP or custom one whose name
/// is unknown here, its ID, capitalised.
pub fn agent_label(agent: &str) -> String {
    let named = crate::modules::machines::agent_label(agent);
    let Some(id) = named
        .strip_prefix("acp:")
        .or_else(|| named.strip_prefix("custom:"))
    else {
        return named.to_owned();
    };
    let mut chars = id.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn machine_target(machine: Uuid) -> Value {
    json!({ "type": "machine", "machineId": machine })
}

/// A machine with work running on it went offline (`work` sessions and
/// terminals).
pub fn machine_offline(machine: Uuid, name: &str, work: usize) -> NewNotification {
    let running = if work == 1 {
        "Its running session or terminal".to_owned()
    } else {
        format!("Its {work} running sessions and terminals")
    };
    NewNotification {
        kind: MACHINE_OFFLINE.to_owned(),
        source: "ahab".to_owned(),
        level: Level::Warning,
        title: format!("{name} is offline"),
        body: Some(format!(
            "{running} cannot be reached until it connects again."
        )),
        project_id: None,
        target: machine_target(machine),
        group_key: Some(offline_group(machine)),
    }
}

pub fn offline_group(machine: Uuid) -> String {
    format!("machine:{machine}:offline")
}

pub fn behind_group(machine: Uuid) -> String {
    format!("machine:{machine}:behind")
}

/// A machine's daemon is older than the server: parts of Ahab do not work
/// there.
pub fn machine_behind(
    machine: Uuid,
    name: &str,
    version: Option<&str>,
    server: &str,
) -> NewNotification {
    let there = version.map_or_else(|| "an older Ahab".to_owned(), |v| format!("Ahab {v}"));
    NewNotification {
        kind: MACHINE_BEHIND.to_owned(),
        source: "ahab".to_owned(),
        level: Level::Info,
        title: format!("{name} runs an older Ahab"),
        body: Some(format!(
            "It runs {there}, this server {server}. Update Ahab there so all of it works on {name}."
        )),
        project_id: None,
        target: machine_target(machine),
        group_key: Some(behind_group(machine)),
    }
}

/// The group a newer release's notification stands for: a later one
/// replaces it.
pub const UPDATE_GROUP: &str = "ahab.update";

/// A newer Ahab release came out.
pub fn update_available(version: &semver::Version, url: &str, current: &str) -> NewNotification {
    NewNotification {
        kind: UPDATE_AVAILABLE.to_owned(),
        source: "ahab".to_owned(),
        level: Level::Info,
        title: format!("Ahab {version} is available"),
        body: Some(format!(
            "This server runs {current}. Update it the way Ahab was installed here; the release notes say what changed."
        )),
        project_id: None,
        target: json!({ "type": "url", "url": url }),
        group_key: Some(UPDATE_GROUP.to_owned()),
    }
}

/// The newest release in GitHub's list of releases that is newer than
/// `current`: its version and page. Drafts never count; prereleases only
/// when `prereleases`.
pub fn newer_release(
    releases: &Value,
    current: &semver::Version,
    prereleases: bool,
) -> Option<(semver::Version, String)> {
    releases
        .as_array()?
        .iter()
        .filter(|release| release["draft"] != true)
        .filter(|release| prereleases || release["prerelease"] != true)
        .filter_map(|release| {
            let tag = release["tag_name"].as_str()?;
            let version = semver::Version::parse(tag.trim_start_matches('v')).ok()?;
            let url = release["html_url"].as_str().unwrap_or_default().to_owned();
            Some((version, url))
        })
        .filter(|(version, _)| version > current)
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// What a chat's transition tells: one notification, grouped by the chat,
/// so its newest state stands for it in the inbox.
/// Where a chat is, by the names people read: its project and session (each absent when it could not be read).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPlace {
    pub project: Option<String>,
    pub session: Option<String>,
}

impl ChatPlace {
    /// `ahab · Fix the flaky test`, for a line under a notification.
    pub fn line(&self) -> Option<String> {
        let parts: Vec<&str> = [&self.project, &self.session]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// What a waiting request says, by its kind: "Claude Code asks: Which
/// library?", "Claude Code has a plan for you to approve", else "… needs
/// you"; and a line for under it, when the request has one.
fn asks(agent: &str, request: &Value) -> (String, Option<String>) {
    let kind = &request["kind"];
    let text = |value: &Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| short(text, 120))
    };
    match kind["type"].as_str() {
        Some("question") => {
            let questions = kind["questions"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            if let Some(first) = questions
                .first()
                .and_then(|question| text(&question["question"]))
            {
                let more = questions.len() - 1;
                let body = (more > 0).then(|| {
                    if more == 1 {
                        "And 1 more question.".to_owned()
                    } else {
                        format!("And {more} more questions.")
                    }
                });
                return (format!("{agent} asks: {first}"), body);
            }
            (format!("{agent} asks you something"), None)
        }
        Some("planApproval") => {
            let first = kind["plan"].as_str().and_then(|plan| {
                plan.lines()
                    .map(|line| line.trim().trim_start_matches('#').trim())
                    .find(|line| !line.is_empty())
                    .map(|line| short(line, 120))
            });
            (format!("{agent} has a plan for you to approve"), first)
        }
        Some("form") => match text(&kind["message"]) {
            Some(message) => (format!("{agent} asks: {message}"), None),
            None => (format!("{agent} asks you to fill in a form"), None),
        },
        _ => (
            format!("{agent} needs you"),
            text(&request["title"])
                .or_else(|| text(&request["message"]))
                .or_else(|| text(&kind["description"])),
        ),
    }
}

/// `text`, cut to `limit` characters with an ellipsis.
fn short(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let cut: String = text.chars().take(limit - 1).collect();
    format!("{}…", cut.trim_end())
}

/// `agent`: the name the chat's agent was added under, when known.
pub fn from_transition(
    chat: &Chat,
    transition: &Transition,
    place: &ChatPlace,
    agent: Option<&str>,
) -> NewNotification {
    let agent = agent.map_or_else(|| agent_label(&chat.agent), str::to_owned);
    let (kind, level, title, body) = match transition {
        Transition::Finished => (
            FINISHED,
            Level::Success,
            format!("{agent} finished"),
            Some(chat.title.clone()),
        ),
        Transition::NeedsYou { request } => {
            let (title, body) = asks(&agent, request);
            (
                NEEDS_YOU,
                Level::Warning,
                title,
                Some(body.unwrap_or_else(|| chat.title.clone())),
            )
        }
        Transition::Failed { reason } => (
            FAILED,
            Level::Error,
            format!("{agent} stopped"),
            Some(reason.clone()),
        ),
    };
    NewNotification {
        kind: kind.to_owned(),
        source: "ahab".to_owned(),
        level,
        title,
        body,
        project_id: Some(chat.project_id),
        // The names as they were when it was sent: shown with it, even after
        // a rename (the IDs still lead to it).
        target: json!({
            "type": "session",
            "projectId": chat.project_id,
            "sessionId": chat.session_id,
            "chatId": chat.id,
            "projectName": place.project,
            "sessionTitle": place.session,
        }),
        group_key: Some(format!("chat:{}", chat.id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_says_what_it_asks() {
        let question = json!({ "id": "r", "kind": { "type": "question", "questions": [
            { "id": "q1", "question": "Which library?", "options": [] },
            { "id": "q2", "question": "Which version?", "options": [] },
        ] } });
        assert_eq!(
            asks("Claude Code", &question),
            (
                "Claude Code asks: Which library?".to_owned(),
                Some("And 1 more question.".to_owned())
            )
        );
        let plan = json!({ "id": "r", "kind": { "type": "planApproval", "plan": "\n# Add dark mode\n\n1. Tokens", "options": [] } });
        assert_eq!(
            asks("Claude Code", &plan),
            (
                "Claude Code has a plan for you to approve".to_owned(),
                Some("Add dark mode".to_owned())
            )
        );
        let form = json!({ "id": "r", "kind": { "type": "form", "message": "Your GitHub login?", "fields": [] } });
        assert_eq!(asks("Codex", &form).0, "Codex asks: Your GitHub login?");
        let tool = json!({ "id": "r", "kind": { "type": "tool", "name": "Bash", "description": "Run the tests" } });
        assert_eq!(
            asks("Codex", &tool),
            (
                "Codex needs you".to_owned(),
                Some("Run the tests".to_owned())
            )
        );
        assert_eq!(short(&"a".repeat(200), 120).chars().count(), 120);
    }

    #[test]
    fn a_kind_kept_inside_never_goes_outside() {
        let mut settings = Settings::default();
        assert!(settings.goes_outside(FINISHED));
        settings.inside_only.insert(FINISHED.to_owned());
        assert!(!settings.goes_outside(FINISHED));
        assert!(settings.sends(FINISHED), "still shown inside");
        assert!(settings.goes_outside(NEEDS_YOU));
        settings.outside = false;
        assert!(!settings.goes_outside(NEEDS_YOU));
    }

    #[test]
    fn kinds_are_on_unless_turned_off_and_all_off_with_notifications() {
        let mut settings = Settings::default();
        assert!(settings.sends(FINISHED));
        settings.kinds.insert(FINISHED.to_owned(), false);
        assert!(!settings.sends(FINISHED));
        assert!(settings.sends(NEEDS_YOU));
        settings.enabled = false;
        assert!(!settings.sends(NEEDS_YOU));
    }

    #[test]
    fn keeping_is_a_number_of_days_or_forever() {
        assert!(Settings::default().check().is_ok());
        let forever = Settings {
            keep_days: None,
            ..Settings::default()
        };
        assert!(forever.check().is_ok());
        for days in [0, MAX_KEEP_DAYS + 1] {
            let wrong = Settings {
                keep_days: Some(days),
                ..Settings::default()
            };
            assert!(wrong.check().is_err(), "{days}");
        }
    }

    #[test]
    fn the_newest_newer_release_counts_and_drafts_never() {
        let releases = json!([
            { "tag_name": "v0.3.0", "draft": true, "html_url": "d" },
            { "tag_name": "v0.2.1-rc.1", "prerelease": true, "html_url": "p" },
            { "tag_name": "v0.2.0", "html_url": "https://github.com/x/releases/v0.2.0" },
            { "tag_name": "v0.1.0", "html_url": "o" },
            { "tag_name": "nightly", "html_url": "n" },
        ]);
        let current = semver::Version::parse("0.1.0").unwrap();
        let (version, url) = newer_release(&releases, &current, false).unwrap();
        assert_eq!(version.to_string(), "0.2.0");
        assert!(url.ends_with("v0.2.0"));
        let (pre, _) = newer_release(&releases, &current, true).unwrap();
        assert_eq!(pre.to_string(), "0.2.1-rc.1");
        let newest = semver::Version::parse("0.2.0").unwrap();
        assert!(newer_release(&releases, &newest, false).is_none());
        assert!(newer_release(&json!({ "message": "Not Found" }), &current, false).is_none());
    }

    #[test]
    fn agents_are_named_as_people_know_them() {
        assert_eq!(agent_label("claude-code"), "Claude Code");
        assert_eq!(agent_label("acp:gemini"), "Gemini");
        assert_eq!(agent_label("custom:my-agent"), "My-agent");
    }
}
