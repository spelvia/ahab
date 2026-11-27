//! Plan limits on the wire: what a daemon answers to `agents.usage`, and
//! what the API returns.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

/// Whether an agent's plan limits could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UsageStatus)]
pub enum UsageStatusDto {
    /// The limits, as the agent's provider reports them.
    Available,
    /// Nothing to show, and why: signed out, an expired or rejected
    /// sign-in, or an API key (which has no plan limits).
    Unavailable,
    /// Asking failed; `problem` says why. Also a status from a newer
    /// daemon this server does not know.
    #[serde(other)]
    Error,
}

/// One limit of an agent's plan, such as its 5-hour session or its week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = UsageWindow)]
pub struct UsageWindowDto {
    /// Stable within an agent: `session`, `weekly`, `weekly:<model>`…
    #[schema(example = "session")]
    pub id: String,
    /// "Session (5 hours)", "Weekly", "Weekly · Opus".
    #[schema(example = "Session (5 hours)")]
    pub label: String,
    /// A few characters where space is tight ("5h", "wk"); absent for
    /// limits that only show in full.
    #[schema(example = "5h")]
    pub short_label: Option<String>,
    /// How much of the limit is used, 0 to 100.
    #[schema(example = 42.0)]
    pub used_percent: f64,
    /// When the limit starts over.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub resets_at: Option<OffsetDateTime>,
}

/// An agent's plan limits on one machine, for the account signed in there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = AgentUsage)]
pub struct AgentUsageDto {
    /// `claude-code` or `codex`.
    #[schema(example = "claude-code")]
    pub agent: String,
    #[schema(example = "Claude Code")]
    pub label: String,
    pub status: UsageStatusDto,
    /// The plan, such as "Max 5x" or "Pro".
    #[schema(example = "Max 5x")]
    pub plan: Option<String>,
    /// The limits, most important first; empty unless `available`.
    pub windows: Vec<UsageWindowDto>,
    /// Why there is nothing (or not everything) to show.
    pub problem: Option<String>,
    /// When the provider was asked.
    #[serde(with = "crate::shared::time::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

/// A daemon's answer to `agents.usage`, and the API's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = AgentUsageList)]
pub struct AgentUsageListDto {
    pub items: Vec<AgentUsageDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_from_a_newer_daemon_reads_as_the_fallback() {
        let value: UsageStatusDto = serde_json::from_str(r#""fromTheFuture""#).unwrap();
        assert_eq!(value, UsageStatusDto::Error);
    }
}
