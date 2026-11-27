//! A machine's resources: asked of its daemon each time, never kept.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::modules::machines::machine::service::MachineService;
use crate::modules::machines::metrics::api::MachineMetricsDto;
use crate::modules::machines::presence::service::Presence;
use crate::shared::error::ServiceError;

/// How long a sample may take: the first one waits a moment for CPU use.
const TIMEOUT: Duration = Duration::from_secs(10);

pub struct MetricsService {
    machines: Arc<MachineService>,
    presence: Arc<Presence>,
}

impl MetricsService {
    pub fn new(machines: Arc<MachineService>, presence: Arc<Presence>) -> Self {
        Self { machines, presence }
    }

    /// `machine`'s CPU, memory, disks and processes now.
    pub async fn metrics(&self, machine: Uuid) -> Result<MachineMetricsDto, ServiceError> {
        self.machines.get(machine).await?;
        let answer = self
            .presence
            .request(machine, "machine.metrics", serde_json::json!({}), TIMEOUT)
            .await?;
        serde_json::from_value(answer).map_err(|error| {
            ServiceError::Internal(format!("unreadable machine.metrics answer: {error}"))
        })
    }
}
