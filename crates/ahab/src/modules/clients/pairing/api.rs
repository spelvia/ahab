//! The pairing API's request and response types; `ahab login`, `ahab open`
//! and `ahab clients` read them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::clients::client::api::{ClientDto, ClientKindDto};
use crate::modules::clients::pairing::model::{Pairing, PairingState};

/// Where a pairing stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PairingState)]
pub enum PairingStateDto {
    /// Waiting to be used or answered.
    Open,
    /// A request was approved; the client takes its credential next.
    Approved,
    Denied,
    /// It paired a client.
    Used,
    /// It ran out of time, or was withdrawn.
    Expired,
}

impl From<PairingState> for PairingStateDto {
    fn from(state: PairingState) -> Self {
        match state {
            PairingState::Open => Self::Open,
            PairingState::Approved => Self::Approved,
            PairingState::Denied => Self::Denied,
            PairingState::Used => Self::Used,
            PairingState::Expired => Self::Expired,
        }
    }
}

/// A new pairing link.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatePairing)]
pub struct CreatePairingDto {
    /// The first-run link instead: for this computer only, while no other
    /// client is paired. It works for 24 hours, has no code, and replaces
    /// earlier first-run links.
    #[serde(default)]
    pub first_run: bool,
}

/// A pairing link just created: its secret and code are shown only now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatedPairing)]
pub struct CreatedPairingDto {
    pub id: Uuid,
    /// The link: whoever opens it is paired. The secret is after `#`.
    #[schema(example = "https://ahab.example/pair#9fK2…")]
    pub url: String,
    /// The secret alone, for clients that make their own link.
    pub secret: String,
    /// The code to type on the new client instead; none on a first-run
    /// link.
    #[serde(default)]
    #[schema(example = "K7QM-4RXT")]
    pub code: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// A pairing link, to see whether it was used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PairingLink)]
pub struct PairingLinkDto {
    pub id: Uuid,
    pub state: PairingStateDto,
    /// The client it paired.
    #[serde(default)]
    pub client_id: Option<Uuid>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub expires_at: OffsetDateTime,
}

impl PairingLinkDto {
    pub fn new(pairing: &Pairing, state: PairingState) -> Self {
        Self {
            id: pairing.id,
            state: state.into(),
            client_id: pairing.client_id,
            expires_at: pairing.expires_at,
        }
    }
}

/// Pairs this client: with a link's secret or a code, one of them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Pair)]
pub struct PairDto {
    /// The secret after `#` in a pairing link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// A pairing code from Add client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "K7QM-4RXT")]
    pub code: Option<String>,
    /// This client's name; browsers are named from their `User-Agent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "ahab CLI on build-box")]
    pub name: Option<String>,
    /// `browser` (the default), `cli` or `app`. Browsers get a cookie; the
    /// others get the credential in the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "cli")]
    pub kind: Option<String>,
}

/// A client that was just paired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Paired)]
pub struct PairedDto {
    pub client: ClientDto,
    /// Its credential, for `Authorization: Bearer`; shown only now. Absent
    /// for browsers, which get it as a cookie they cannot read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// A new client asks to be paired.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[schema(as = CreatePairingRequest)]
pub struct CreatePairingRequestDto {
    /// The name approvers see; browsers are named from their `User-Agent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "ahab CLI on build-box")]
    pub name: Option<String>,
    /// `browser` (the default), `cli` or `app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "cli")]
    pub kind: Option<String>,
}

/// A request just made: show the code, and wait with the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatedPairingRequest)]
pub struct CreatedPairingRequestDto {
    pub id: Uuid,
    /// What this client shows, for the approver to compare.
    #[schema(example = "7F3K")]
    pub code: String,
    /// Wait with `GET /api/v1/pairing-requests/{id}?secret=…`.
    pub secret: String,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// A request waiting for an answer, as approvers see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PairingRequest)]
pub struct PairingRequestDto {
    pub id: Uuid,
    #[schema(example = "7F3K")]
    pub code: String,
    #[schema(example = "Firefox on Linux")]
    pub client_name: String,
    pub client_kind: ClientKindDto,
    /// The address it asked from.
    #[serde(default)]
    #[schema(example = "192.168.178.40")]
    pub address: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    pub state: PairingStateDto,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl PairingRequestDto {
    pub fn new(pairing: &Pairing) -> Self {
        Self {
            id: pairing.id,
            code: pairing.code.clone().unwrap_or_default(),
            client_name: pairing.client_name.clone().unwrap_or_default(),
            client_kind: pairing
                .client_kind
                .map_or(ClientKindDto::App, ClientKindDto::from),
            address: pairing.address.clone(),
            user_agent: pairing.user_agent.clone(),
            state: pairing.state.into(),
            expires_at: pairing.expires_at,
            created_at: pairing.created_at,
        }
    }
}

/// The requests waiting for an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = PairingRequestList)]
pub struct PairingRequestListDto {
    pub items: Vec<PairingRequestDto>,
}

/// The answer to a request, so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PairingAnswer)]
pub struct PairingAnswerDto {
    /// `open` (ask again), `used` (paired: see `client`) or `denied`.
    pub state: PairingStateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientDto>,
    /// The credential, once approved; shown only in this answer. Absent
    /// for browsers, which get it as a cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}
