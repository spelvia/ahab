//! Plan limits per machine: asking daemons, and keeping the answers a while.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::modules::machines::machine::service::MachineService;
use crate::modules::machines::presence::service::Presence;
use crate::modules::machines::usage::api::AgentUsageListDto;
use crate::shared::error::ServiceError;

/// How long reading plan limits may take: two providers, asked side by side.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a machine's plan limits are shown before asking again.
const FRESH: Duration = Duration::from_mins(5);

pub struct UsageService {
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
    /// Each machine's last plan limits, and when they were asked for. Kept
    /// in memory only: they are stale within minutes anyway.
    kept: Mutex<HashMap<Uuid, (Instant, AgentUsageListDto)>>,
}

impl UsageService {
    pub fn new(machines: Arc<MachineService>, presence: Arc<Presence>) -> Self {
        Self {
            machines,
            presence,
            kept: Mutex::new(HashMap::new()),
        }
    }

    /// The plan limits of the agents signed in on `machine` (Claude Code's
    /// and Codex's), asked of its daemon at most every 5 minutes unless
    /// `refresh`; the daemon itself asks each provider at most once a minute.
    pub async fn usage(
        &self,
        machine: Uuid,
        refresh: bool,
    ) -> Result<AgentUsageListDto, ServiceError> {
        self.machines.get(machine).await?;
        let kept = self.kept(machine);
        if !refresh
            && let Some((asked, list)) = &kept
            && asked.elapsed() < FRESH
        {
            return Ok(list.clone());
        }
        let answer = match self
            .presence
            .request(machine, "agents.usage", serde_json::json!({}), TIMEOUT)
            .await
        {
            Ok(answer) => answer,
            // An offline machine still shows what it last said.
            Err(error) => return kept.map(|(_, list)| list).ok_or(error),
        };
        let list: AgentUsageListDto = serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable agents.usage answer: {error}"))
        })?;
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(machine, (Instant::now(), list.clone()));
        Ok(list)
    }

    fn kept(&self, machine: Uuid) -> Option<(Instant, AgentUsageListDto)> {
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&machine)
            .cloned()
    }
}
