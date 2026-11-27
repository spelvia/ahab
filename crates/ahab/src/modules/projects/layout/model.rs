//! A saved layout. The server keeps it as the client gave it; only the
//! client reads the tree inside.

use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

/// Desktop and web share a layout; phones have their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientClass {
    Desktop,
    Mobile,
}

impl ClientClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Mobile => "mobile",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "desktop" => Some(Self::Desktop),
            "mobile" => Some(Self::Mobile),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub project_id: Uuid,
    pub client_class: ClientClass,
    pub layout: Value,
    /// The layout before the last save, so a bad save can be undone.
    pub previous: Option<Value>,
    /// Goes up by one with every save.
    pub revision: i64,
    pub updated_at: OffsetDateTime,
}

/// The largest layout the server keeps, in bytes of JSON.
pub const MAX_BYTES: usize = 256 * 1024;
