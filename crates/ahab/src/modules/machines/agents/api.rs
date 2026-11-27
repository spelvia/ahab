//! Agent readiness on the wire: what a daemon answers to `agents.detect`, and
//! what the API returns.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::modules::machines::agents::model::{AgentState, AgentStatus, CustomLaunch};

/// Whether an agent can be used on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentState)]
pub enum AgentStateDto {
    Ready,
    NotSignedIn,
    UnsupportedVersion,
    NotInstalled,
    /// Also a state from a newer daemon this server does not know.
    #[serde(other)]
    Error,
}

impl From<AgentState> for AgentStateDto {
    fn from(state: AgentState) -> Self {
        match state {
            AgentState::Ready => Self::Ready,
            AgentState::NotSignedIn => Self::NotSignedIn,
            AgentState::UnsupportedVersion => Self::UnsupportedVersion,
            AgentState::NotInstalled => Self::NotInstalled,
            AgentState::Error => Self::Error,
        }
    }
}

impl From<AgentStateDto> for AgentState {
    fn from(state: AgentStateDto) -> Self {
        match state {
            AgentStateDto::Ready => Self::Ready,
            AgentStateDto::NotSignedIn => Self::NotSignedIn,
            AgentStateDto::UnsupportedVersion => Self::UnsupportedVersion,
            AgentStateDto::NotInstalled => Self::NotInstalled,
            AgentStateDto::Error => Self::Error,
        }
    }
}

/// One agent as a daemon detected it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAgent {
    pub agent: String,
    /// Its name, for added agents.
    #[serde(default)]
    pub label: Option<String>,
    pub state: AgentStateDto,
    pub version: Option<String>,
    pub path: Option<String>,
    pub account: Option<String>,
    pub detail: Option<String>,
}

/// A daemon's answer to `agents.detect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detected {
    pub agents: Vec<DetectedAgent>,
}

/// An agent's readiness on a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentStatus)]
pub struct AgentStatusDto {
    /// `claude-code`, `codex` or `pi`.
    #[schema(example = "claude-code")]
    pub agent: String,
    /// A readable name, such as "Claude Code".
    #[schema(example = "Claude Code")]
    pub label: String,
    pub state: AgentStateDto,
    #[schema(example = "2.1.322")]
    pub version: Option<String>,
    /// Where the agent's program is on the machine.
    pub path: Option<String>,
    /// The signed-in account, when the agent names it.
    pub account: Option<String>,
    /// Why something could not be determined.
    pub detail: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub checked_at: OffsetDateTime,
}

/// The agents on one machine, as last detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentStatusList)]
pub struct AgentStatusListDto {
    pub items: Vec<AgentStatusDto>,
}

impl From<&AgentStatus> for AgentStatusDto {
    fn from(status: &AgentStatus) -> Self {
        Self {
            label: status.label.clone().unwrap_or_else(|| {
                crate::modules::machines::agents::model::label(&status.agent).to_owned()
            }),
            agent: status.agent.clone(),
            state: status.state.into(),
            version: status.version.clone(),
            path: status.path.clone(),
            account: status.account.clone(),
            detail: status.detail.clone(),
            checked_at: status.checked_at,
        }
    }
}

/// Whether a newer version of an agent is out, and whether Ahab can install
/// it on the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentUpdate)]
pub struct AgentUpdateDto {
    #[schema(example = "codex")]
    pub agent: String,
    #[schema(example = "Codex")]
    pub label: String,
    #[schema(example = "0.167.1")]
    pub installed: Option<String>,
    /// The newest release on the channel it follows; absent when it could not
    /// be looked up.
    #[schema(example = "0.169.0")]
    pub latest: Option<String>,
    /// The channel, such as `latest` or `stable`.
    pub channel: String,
    /// A newer version is out.
    pub available: bool,
    /// How it was installed: `native`, `npm`, `homebrew`, `nix` or `unknown`.
    #[schema(example = "npm")]
    pub method: String,
    /// Ahab can update it here (with the agent's own updater).
    pub can_update: bool,
    /// The command that updates it: the one Ahab runs, or what to do instead.
    #[schema(example = "codex update")]
    pub command: String,
    /// Why Ahab cannot update it, or why the latest version is unknown.
    pub reason: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub checked_at: OffsetDateTime,
}

/// The agents on one machine that could be updated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentUpdateList)]
pub struct AgentUpdateListDto {
    pub items: Vec<AgentUpdateDto>,
}

/// What updating an agent did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentUpdated)]
pub struct AgentUpdatedDto {
    pub agent: String,
    pub before: Option<String>,
    pub after: Option<String>,
    /// The updater finished without an error.
    pub success: bool,
    /// The end of what the updater printed.
    pub output: String,
}

/// Which agent to update, for `agents.update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAgent {
    pub agent: String,
}

/// An added agent for the daemon to detect (and later start): a registry
/// entry (as the registry has it) or a custom command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedToDetect {
    pub agent: String,
    pub name: String,
    #[serde(default)]
    pub entry: Option<serde_json::Value>,
    #[serde(default)]
    pub launch: Option<CustomLaunch>,
}

/// What `agents.detect` is asked: the added agents, besides Stubb's own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetectRequest {
    #[serde(default)]
    pub added: Vec<AddedToDetect>,
}

/// One agent Ahab can run: built in, or from the ACP registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CatalogAgent)]
pub struct CatalogAgentDto {
    /// `claude-code`, or `acp:<registry id>` such as `acp:gemini`.
    #[schema(example = "acp:gemini")]
    pub id: String,
    #[schema(example = "Gemini CLI")]
    pub name: String,
    /// Who makes it, such as "Anthropic".
    pub maker: Option<String>,
    pub description: String,
    /// Its logo is at `/api/v1/agents/{id}/logo`.
    pub has_logo: bool,
    /// The logo is drawn in one colour: show it in the text's colour.
    pub monochrome: bool,
    /// `builtin` or `registry`.
    #[schema(example = "registry")]
    pub source: String,
    /// The release the registry points at.
    pub version: Option<String>,
    pub repository: Option<String>,
    pub website: Option<String>,
    pub license: Option<String>,
    /// How it is launched: `npx`, `uvx`, `binary`.
    pub methods: Vec<String>,
    /// Whether Ahab can add it to a machine yet (npx and uvx agents; agents
    /// shipped only as downloads come later).
    pub addable: bool,
}

/// Every agent Ahab can run, and where the registry part came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentCatalog)]
pub struct AgentCatalogDto {
    pub items: Vec<CatalogAgentDto>,
    /// The ACP registry's version, and whether it is the copy shipped with
    /// Ahab (`bundled`) or fetched (`fetched`).
    pub registry_version: String,
    pub registry_source: String,
}

/// Adds an agent to a machine: one from the registry by `agent`
/// (`acp:<id>`), or a custom one by `name` and `program`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AddAgent)]
pub struct AddAgentDto {
    #[schema(example = "acp:gemini")]
    pub agent: Option<String>,
    /// A custom agent's name.
    pub name: Option<String>,
    /// A custom agent's program (a path, or a name looked up on `PATH`).
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// An agent added to a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AddedAgent)]
pub struct AddedAgentDto {
    #[schema(example = "acp:gemini")]
    pub agent: String,
    pub name: String,
    /// A custom command, not a registry agent.
    pub custom: bool,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub added_at: OffsetDateTime,
}

impl From<&crate::modules::machines::agents::model::AddedAgent> for AddedAgentDto {
    fn from(agent: &crate::modules::machines::agents::model::AddedAgent) -> Self {
        Self {
            agent: agent.agent.clone(),
            name: agent.name.clone(),
            custom: agent.launch.is_some(),
            added_at: agent.added_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_from_a_newer_daemon_reads_as_the_fallback() {
        let value: AgentStateDto = serde_json::from_str(r#""fromTheFuture""#).unwrap();
        assert_eq!(value, AgentStateDto::Error);
    }
}
