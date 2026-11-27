//! The machines API's request and response types. The server returns them
//! and the command line reads them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::machines::machine::model::{Machine, MachineState};

/// A computer that runs a daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Machine)]
pub struct MachineDto {
    pub id: Uuid,
    #[schema(example = "alexs-laptop")]
    pub name: String,
    pub state: MachineStateDto,
    /// The server's own host.
    pub is_local: bool,
    /// Whether its daemon is connected right now.
    pub online: bool,
    /// A short code to compare with the joining machine's screen.
    #[schema(example = "K7-4QXM")]
    pub check_code: String,
    /// The key's fingerprint, to compare when approving a machine.
    #[schema(example = "SHA256:3q2+7wJ0nJ0b1dFqZb5mQx2s8z7mYgY3N2xg0c6Pq7A")]
    pub fingerprint: String,
    #[schema(example = "linux")]
    pub os: Option<String>,
    #[schema(example = "x86_64")]
    pub arch: Option<String>,
    /// The daemon's Ahab version.
    #[schema(example = "0.1.0")]
    pub version: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub approved_at: Option<OffsetDateTime>,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub last_seen_at: Option<OffsetDateTime>,
    /// Its connected daemon runs an older Ahab, without some of the methods
    /// this server calls: what needs them fails with `machine_outdated`
    /// until it is updated. False while it is not connected.
    #[serde(default)]
    pub outdated: bool,
    /// The methods its connected daemon lacks; empty while it is
    /// not connected.
    #[serde(default)]
    #[schema(example = json!(["machine.metrics"]))]
    pub missing_methods: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = MachineState)]
pub enum MachineStateDto {
    /// Asked to join; waits for approval.
    Pending,
    /// Approved: may connect.
    Active,
}

/// A list of machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = MachineList)]
pub struct MachineListDto {
    pub items: Vec<MachineDto>,
}

/// A new name for a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = RenameMachine)]
pub struct RenameMachineDto {
    #[schema(example = "build-box")]
    pub name: String,
}

impl MachineDto {
    pub fn new(machine: &Machine, online: bool) -> Self {
        Self {
            online,
            check_code: machine.public_key.check_code(),
            id: machine.id,
            name: machine.name.to_string(),
            state: match machine.state {
                MachineState::Pending => MachineStateDto::Pending,
                MachineState::Active => MachineStateDto::Active,
            },
            is_local: machine.is_local,
            fingerprint: machine.public_key.fingerprint().to_string(),
            os: machine.os.clone(),
            arch: machine.arch.clone(),
            version: machine.version.clone(),
            created_at: machine.created_at,
            approved_at: machine.approved_at,
            last_seen_at: machine.last_seen_at,
            outdated: false,
            missing_methods: Vec::new(),
        }
    }

    /// With what its connected daemon lacks (`Presence::missing_methods`).
    #[must_use]
    pub fn lacking(mut self, missing: Vec<String>) -> Self {
        self.outdated = !missing.is_empty();
        self.missing_methods = missing;
        self
    }
}
