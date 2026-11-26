//! The joining API's request and response types; `ahab setup` reads them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::machines::enrollment::model::JoinCode;
use crate::modules::machines::machine::api::MachineStateDto;
use crate::modules::machines::machine::model::Machine;

/// A machine asks to join.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = EnrollRequest)]
pub struct EnrollRequestDto {
    /// The machine's Ed25519 public key, in base64.
    pub public_key: String,
    /// Its signature of `ahab/enroll/v1`, the host it asked and the key, in
    /// base64.
    pub signature: String,
    #[schema(example = "build-box")]
    pub hostname: String,
    /// A one-time code: the machine is approved at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "ABCD-EFGH")]
    pub code: Option<String>,
}

/// A machine's request to join, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Enrollment)]
pub struct EnrollmentDto {
    /// The machine's ID; follow it with `GET /api/v1/enrollments/{id}`.
    pub id: Uuid,
    pub name: String,
    pub state: MachineStateDto,
    /// The code to compare with the web app before approving.
    pub check_code: String,
    pub fingerprint: String,
    /// The server's key fingerprint, which the machine pins.
    pub server_fingerprint: String,
}

impl EnrollmentDto {
    pub fn new(machine: &Machine, server_fingerprint: String) -> Self {
        let dto = crate::modules::machines::machine::api::MachineDto::new(machine, false);
        Self {
            id: dto.id,
            name: dto.name,
            state: dto.state,
            check_code: dto.check_code,
            fingerprint: dto.fingerprint,
            server_fingerprint,
        }
    }
}

/// A new one-time code.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[schema(as = CreateJoinCode)]
pub struct CreateJoinCodeDto {
    /// The name the machine gets; else its hostname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "build-box")]
    pub name: Option<String>,
}

/// A one-time code that still works. The code itself is shown only once,
/// when it is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = JoinCode)]
pub struct JoinCodeDto {
    pub id: Uuid,
    pub name: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<&JoinCode> for JoinCodeDto {
    fn from(code: &JoinCode) -> Self {
        Self {
            id: code.id,
            name: code.name.as_ref().map(ToString::to_string),
            expires_at: code.expires_at,
            created_at: code.created_at,
        }
    }
}

/// A code that was just created, with the code itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatedJoinCode)]
pub struct CreatedJoinCodeDto {
    #[schema(example = "ABCD-EFGH")]
    pub code: String,
    #[serde(flatten)]
    pub join_code: JoinCodeDto,
    /// The address machines join at: `public_url`, else the address this
    /// request used (another machine's view of it when that was this
    /// computer's own). Absent from servers before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "https://ahab.example")]
    pub server_url: Option<String>,
    /// The server's key fingerprint, which the joining machine pins
    /// (`ahab setup --server-fingerprint`). Absent from servers before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_fingerprint: Option<String>,
}

/// The codes that still work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = JoinCodeList)]
pub struct JoinCodeListDto {
    pub items: Vec<JoinCodeDto>,
}
