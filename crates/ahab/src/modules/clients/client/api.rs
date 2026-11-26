//! The clients API's request and response types; `ahab clients` reads them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::clients::client::model::{Client, ClientKind, PairedVia};

/// What a client is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ClientKind)]
pub enum ClientKindDto {
    /// The server's own computer, through its credential file.
    Local,
    Browser,
    Cli,
    /// The desktop or mobile app.
    App,
    /// A token for a script.
    Token,
}

impl From<ClientKind> for ClientKindDto {
    fn from(kind: ClientKind) -> Self {
        match kind {
            ClientKind::Local => Self::Local,
            ClientKind::Browser => Self::Browser,
            ClientKind::Cli => Self::Cli,
            ClientKind::App => Self::App,
            ClientKind::Token => Self::Token,
        }
    }
}

/// How a client was paired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PairedVia)]
pub enum PairedViaDto {
    Local,
    Link,
    Code,
    Request,
    Token,
}

impl From<PairedVia> for PairedViaDto {
    fn from(via: PairedVia) -> Self {
        match via {
            PairedVia::Local => Self::Local,
            PairedVia::Link => Self::Link,
            PairedVia::Code => Self::Code,
            PairedVia::Request => Self::Request,
            PairedVia::Token => Self::Token,
        }
    }
}

/// A client paired with this server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Client)]
pub struct ClientDto {
    pub id: Uuid,
    #[schema(example = "Firefox on Linux")]
    pub name: String,
    pub kind: ClientKindDto,
    pub paired_via: PairedViaDto,
    /// The client that let it in; absent for this computer and clients
    /// paired with a link the server printed.
    #[serde(default)]
    pub paired_by: Option<Uuid>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Written at most once a minute.
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    /// The address it was last used from.
    #[serde(default)]
    #[schema(example = "192.168.178.40")]
    pub last_address: Option<String>,
    /// Whether it was last used over HTTPS, this computer or a tailnet:
    /// false shows "Not encrypted".
    pub encrypted: bool,
    /// The client making this request.
    pub current: bool,
    /// Not used for 90 days. It is never revoked for it.
    pub unused: bool,
}

impl ClientDto {
    pub fn new(client: &Client, current: Option<Uuid>, unused: bool) -> Self {
        Self {
            id: client.id,
            name: client.name.to_string(),
            kind: client.kind.into(),
            paired_via: client.paired_via.into(),
            paired_by: client.paired_by,
            user_agent: client.user_agent.clone(),
            created_at: client.created_at,
            last_used_at: client.last_used_at,
            last_address: client.last_address.clone(),
            encrypted: client.encrypted,
            current: current == Some(client.id),
            unused,
        }
    }
}

/// The clients paired with this server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ClientList)]
pub struct ClientListDto {
    pub items: Vec<ClientDto>,
}

/// A client's new name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = RenameClient)]
pub struct RenameClientDto {
    #[schema(example = "Work laptop")]
    pub name: String,
}

/// A token for a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = CreateToken)]
pub struct CreateTokenDto {
    #[schema(example = "CI deploys")]
    pub name: String,
}

/// A token that was just created: shown only in this answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatedToken)]
pub struct CreatedTokenDto {
    /// Send it as `Authorization: Bearer <token>`, or set `AHAB_TOKEN`.
    pub token: String,
    pub client: ClientDto,
}
