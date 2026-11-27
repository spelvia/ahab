//! An agent's readiness on one machine.

use stubb::discovery::Cli;
use time::OffsetDateTime;
use uuid::Uuid;

/// Whether an agent can be used on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Installed and signed in.
    Ready,
    /// Installed, but signed out.
    NotSignedIn,
    /// Installed, but a version this Ahab cannot drive.
    UnsupportedVersion,
    /// Not found on the machine.
    NotInstalled,
    /// Detection failed; the detail says why.
    Error,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotSignedIn => "not_signed_in",
            Self::UnsupportedVersion => "unsupported_version",
            Self::NotInstalled => "not_installed",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "ready" => Self::Ready,
            "not_signed_in" => Self::NotSignedIn,
            "unsupported_version" => Self::UnsupportedVersion,
            "not_installed" => Self::NotInstalled,
            "error" => Self::Error,
            _ => return None,
        })
    }
}

/// The agents Ahab knows, by their stable identifiers.
pub const AGENTS: [&str; 3] = ["claude-code", "codex", "pi"];

/// The latest report for one agent on one machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub machine_id: Uuid,
    /// `claude-code`, `codex`, `pi`, or an added agent (`acp:<id>`,
    /// `custom:<name>`).
    pub agent: String,
    /// Its name, for agents that bring their own (added ones).
    pub label: Option<String>,
    pub state: AgentState,
    pub version: Option<String>,
    pub path: Option<String>,
    /// The account, when the agent names it.
    pub account: Option<String>,
    pub detail: Option<String>,
    pub checked_at: OffsetDateTime,
}

/// How Ahab runs an agent: one of Stubb's own harnesses, or any agent over
/// the Agent Client Protocol (from the ACP registry, or a custom command).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Native(Harness),
    Acp(String),
}

/// How Ahab runs `agent`, when it can.
pub fn kind(agent: &str) -> Option<AgentKind> {
    if let Some(harness) = harness(agent) {
        return Some(AgentKind::Native(harness));
    }
    added(agent).then(|| AgentKind::Acp(agent.to_owned()))
}

/// Whether `agent` is one added to machines: `acp:<registry id>` or
/// `custom:<name>`, with a name of letters, digits, `-`, `_` and `.`.
pub fn added(agent: &str) -> bool {
    let name = agent
        .strip_prefix("acp:")
        .or_else(|| agent.strip_prefix("custom:"));
    name.is_some_and(|name| {
        !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

/// The registry entry an `acp:` agent stands for.
pub fn registry_id(agent: &str) -> Option<&str> {
    agent.strip_prefix("acp:")
}

/// How to start a custom agent: its program, arguments and variables.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomLaunch {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// An agent added to a machine: from the ACP registry (`launch` absent) or
/// custom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedAgent {
    pub machine_id: Uuid,
    pub agent: String,
    pub name: String,
    pub launch: Option<CustomLaunch>,
    pub added_at: OffsetDateTime,
}

/// One of Stubb's own agents, which Ahab runs natively (not over ACP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Harness {
    Claude,
    Codex,
    Pi,
}

impl Harness {
    /// Stubb's command-line agent behind it.
    pub fn cli(self) -> Cli {
        match self {
            Self::Claude => stubb::providers::claude::CLI,
            Self::Codex => stubb::providers::codex::CLI,
            Self::Pi => stubb::providers::pi::CLI,
        }
    }

    /// The executable's usual name (`codex`).
    pub fn program(self) -> &'static str {
        self.cli().program()
    }

    /// The harness behind one of Stubb's command-line agents, when Ahab runs it.
    pub fn of(cli: Cli) -> Option<Self> {
        [Self::Claude, Self::Codex, Self::Pi]
            .into_iter()
            .find(|harness| harness.cli() == cli)
    }
}

/// The Stubb harness that runs an agent, by its identifier.
pub fn harness(agent: &str) -> Option<Harness> {
    match agent {
        "claude-code" => Some(Harness::Claude),
        "codex" => Some(Harness::Codex),
        "pi" => Some(Harness::Pi),
        _ => None,
    }
}

/// An agent's identifier, from Stubb's command-line agent that runs it.
pub fn agent_id(cli: Cli) -> &'static str {
    match Harness::of(cli) {
        Some(Harness::Claude) => "claude-code",
        Some(Harness::Codex) => "codex",
        Some(Harness::Pi) => "pi",
        // An agent a newer Stubb adds: Stubb's own identifier for it.
        None => cli.id(),
    }
}

/// A readable name for an agent identifier.
pub fn label(agent: &str) -> &str {
    match agent {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "pi" => "Pi",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_through_their_names() {
        for state in [
            AgentState::Ready,
            AgentState::NotSignedIn,
            AgentState::UnsupportedVersion,
            AgentState::NotInstalled,
            AgentState::Error,
        ] {
            assert_eq!(AgentState::parse(state.as_str()), Some(state));
        }
        assert_eq!(AgentState::parse("sleeping"), None);
    }

    #[test]
    fn added_agents_are_told_from_built_in_ones() {
        assert_eq!(kind("codex"), Some(AgentKind::Native(Harness::Codex)));
        assert_eq!(
            kind("acp:gemini"),
            Some(AgentKind::Acp("acp:gemini".into()))
        );
        assert_eq!(
            kind("custom:my-agent"),
            Some(AgentKind::Acp("custom:my-agent".into()))
        );
        assert_eq!(kind("acp:"), None);
        assert_eq!(kind("acp:../x"), None);
        assert_eq!(kind("vim"), None);
        assert_eq!(registry_id("acp:gemini"), Some("gemini"));
    }

    #[test]
    fn agents_have_readable_names() {
        assert_eq!(label("claude-code"), "Claude Code");
        assert_eq!(label("unknown"), "unknown");
    }
}
