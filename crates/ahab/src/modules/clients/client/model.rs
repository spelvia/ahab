//! Clients and their credentials. No I/O here.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::shared::error::{DomainError, FieldError};

/// Every credential starts with this, so people and secret scanners can
/// tell what it is.
pub const CREDENTIAL_PREFIX: &str = "ahab_";

/// How long a client may go unused before the list says so. It is never
/// revoked for it.
pub fn unused_after() -> Duration {
    Duration::days(90)
}

/// `last_used_at` is written at most this often.
pub fn touch_every() -> Duration {
    Duration::minutes(1)
}

/// How long revoked clients are kept, then deleted.
pub fn kept_after_revoking() -> Duration {
    Duration::days(30)
}

/// What a client is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// The server's own computer, through its credential file.
    Local,
    Browser,
    Cli,
    /// The desktop or mobile app, and clients Ahab does not know yet.
    App,
    /// A token for a script.
    Token,
}

impl ClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Browser => "browser",
            Self::Cli => "cli",
            Self::App => "app",
            Self::Token => "token",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "browser" => Some(Self::Browser),
            "cli" => Some(Self::Cli),
            "app" => Some(Self::App),
            "token" => Some(Self::Token),
            _ => None,
        }
    }

    /// The kind a new client says it is. It cannot claim to be the local
    /// client or a token; a kind this Ahab does not know is an app.
    pub fn claimed(text: Option<&str>) -> Self {
        match text.map(Self::parse) {
            None | Some(Some(Self::Browser)) => Self::Browser,
            Some(Some(Self::Cli)) => Self::Cli,
            Some(Some(Self::App | Self::Local | Self::Token) | None) => Self::App,
        }
    }
}

/// How a client was paired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairedVia {
    /// The server's own computer, through its credential file.
    Local,
    /// A pairing link (the QR code, `ahab open`, the first-run link).
    Link,
    /// A pairing code typed on the new client.
    Code,
    /// The new client asked and was approved.
    Request,
    /// Created as a token by a paired client.
    Token,
}

impl PairedVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Link => "link",
            Self::Code => "code",
            Self::Request => "request",
            Self::Token => "token",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "link" => Some(Self::Link),
            "code" => Some(Self::Code),
            "request" => Some(Self::Request),
            "token" => Some(Self::Token),
            _ => None,
        }
    }
}

/// A client's name, such as "Firefox on Linux".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientName(String);

impl ClientName {
    pub const MAX: usize = 80;

    pub fn parse(text: &str) -> Result<Self, DomainError> {
        let name = text.trim();
        let invalid = |message: &str| DomainError::Invalid(vec![FieldError::new("name", message)]);
        if name.is_empty() {
            return Err(invalid("Enter a name."));
        }
        if name.chars().count() > Self::MAX {
            return Err(invalid("Use at most 80 characters."));
        }
        if name.chars().any(char::is_control) {
            return Err(invalid("Use letters, digits, spaces and punctuation."));
        }
        Ok(Self(name.to_owned()))
    }

    /// `text` made into a name, or `fallback` when it cannot be one.
    pub fn or(text: Option<&str>, fallback: &str) -> Self {
        text.and_then(|text| {
            let cut: String = text.trim().chars().take(Self::MAX).collect();
            Self::parse(&cut).ok()
        })
        .unwrap_or_else(|| Self(fallback.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ClientName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a request came from, as a client's record keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// The client's address, after trusted proxies.
    pub address: Option<String>,
    /// Whether the connection was HTTPS, loopback or a tailnet.
    pub encrypted: bool,
}

/// A client paired with the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: Uuid,
    pub name: ClientName,
    pub kind: ClientKind,
    pub credential_hash: [u8; 32],
    /// The client that let it in; `None` for the local client and clients
    /// paired with a link the server printed.
    pub paired_by: Option<Uuid>,
    pub paired_via: PairedVia,
    pub user_agent: Option<String>,
    pub created_at: OffsetDateTime,
    pub last_used_at: Option<OffsetDateTime>,
    pub last_address: Option<String>,
    pub encrypted: bool,
    pub revoked_at: Option<OffsetDateTime>,
}

impl Client {
    /// A new client with the credential whose hash is `credential_hash`.
    pub fn new(
        name: ClientName,
        kind: ClientKind,
        credential_hash: [u8; 32],
        paired_via: PairedVia,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            kind,
            credential_hash,
            paired_by: None,
            paired_via,
            user_agent: None,
            created_at: now,
            last_used_at: None,
            last_address: None,
            encrypted: false,
            revoked_at: None,
        }
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// Not used for [`unused_after`]: the list says so.
    pub fn is_unused(&self, now: OffsetDateTime) -> bool {
        self.kind != ClientKind::Local
            && self.last_used_at.unwrap_or(self.created_at) + unused_after() < now
    }

    /// Whether a request now should be written down: the first, or the
    /// first in [`touch_every`].
    pub fn needs_touch(&self, now: OffsetDateTime) -> bool {
        self.last_used_at
            .is_none_or(|last| last + touch_every() <= now)
    }
}

/// A credential from 32 random bytes.
pub fn credential_from_random(bytes: &[u8; 32]) -> String {
    format!("{CREDENTIAL_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// The SHA-256 of a secret: credentials, pairing secrets and codes are
/// stored only as this. They are random enough that a slow hash adds
/// nothing.
pub fn hash_secret(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.trim().as_bytes()).into()
}

/// A readable name for a browser from its `User-Agent`, such as "Firefox
/// on Linux".
pub fn browser_name(user_agent: Option<&str>) -> String {
    let Some(agent) = user_agent else {
        return "A browser".to_owned();
    };
    let browser = if agent.contains("Edg/") {
        "Edge"
    } else if agent.contains("OPR/") {
        "Opera"
    } else if agent.contains("Firefox/") || agent.contains("FxiOS/") {
        "Firefox"
    } else if agent.contains("Chrome/") || agent.contains("CriOS/") {
        "Chrome"
    } else if agent.contains("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let system = if agent.contains("iPhone") {
        Some("iPhone")
    } else if agent.contains("iPad") {
        Some("iPad")
    } else if agent.contains("Android") {
        Some("Android")
    } else if agent.contains("CrOS") {
        Some("ChromeOS")
    } else if agent.contains("Windows") {
        Some("Windows")
    } else if agent.contains("Macintosh") || agent.contains("Mac OS X") {
        Some("macOS")
    } else if agent.contains("Linux") {
        Some("Linux")
    } else {
        None
    };
    match system {
        Some(system) => format!("{browser} on {system}"),
        None => browser.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_browsers_from_their_user_agent() {
        let cases = [
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
                "Firefox on Linux",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                "Safari on iPhone",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like \
                 Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0",
                "Edge on Windows",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, \
                 like Gecko) Chrome/140.0.0.0 Safari/537.36",
                "Chrome on macOS",
            ),
            (
                "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/140.0.0.0 Mobile Safari/537.36",
                "Chrome on Android",
            ),
            ("curl/8.0", "A browser"),
        ];
        for (agent, name) in cases {
            assert_eq!(browser_name(Some(agent)), name, "{agent}");
        }
        assert_eq!(browser_name(None), "A browser");
    }

    #[test]
    fn credentials_are_prefixed_and_hashed() {
        let credential = credential_from_random(&[7; 32]);
        assert!(credential.starts_with("ahab_"));
        assert_eq!(credential.len(), 5 + 43);
        assert_eq!(
            hash_secret(&credential),
            hash_secret(&format!(" {credential}\n"))
        );
        assert_ne!(hash_secret(&credential), hash_secret("ahab_other"));
    }

    #[test]
    fn checks_names() {
        assert!(ClientName::parse("Firefox on Linux").is_ok());
        assert!(ClientName::parse("  ").is_err());
        assert!(ClientName::parse(&"x".repeat(81)).is_err());
        assert!(ClientName::parse("bad\nname").is_err());
        assert_eq!(ClientName::or(Some(""), "fallback").as_str(), "fallback");
        assert_eq!(
            ClientName::or(Some(&"y".repeat(100)), "f").as_str().len(),
            80
        );
    }

    #[test]
    fn kinds_a_new_client_may_claim() {
        assert_eq!(ClientKind::claimed(None), ClientKind::Browser);
        assert_eq!(ClientKind::claimed(Some("cli")), ClientKind::Cli);
        assert_eq!(ClientKind::claimed(Some("local")), ClientKind::App);
        assert_eq!(ClientKind::claimed(Some("token")), ClientKind::App);
        assert_eq!(ClientKind::claimed(Some("watch")), ClientKind::App);
    }

    #[test]
    fn marks_clients_unused_after_90_days_and_touches_once_a_minute() {
        let now = OffsetDateTime::now_utc();
        let mut client = Client::new(
            ClientName::parse("Phone").unwrap(),
            ClientKind::Browser,
            [0; 32],
            PairedVia::Link,
            now - Duration::days(91),
        );
        assert!(client.is_unused(now));
        assert!(client.needs_touch(now));
        client.last_used_at = Some(now - Duration::seconds(30));
        assert!(!client.is_unused(now));
        assert!(!client.needs_touch(now));
        assert!(client.needs_touch(now + Duration::seconds(30)));
        client.kind = ClientKind::Local;
        client.last_used_at = None;
        assert!(!client.is_unused(now));
    }
}
