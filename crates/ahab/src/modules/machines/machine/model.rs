//! A machine and the rules for it. No I/O here.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shared::error::{DomainError, FieldError};

/// The server's record of a computer that runs a daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub id: Uuid,
    pub name: MachineName,
    pub state: MachineState,
    /// The server's own host.
    pub is_local: bool,
    pub public_key: PublicKey,
    pub os: Option<String>,
    pub arch: Option<String>,
    /// The daemon's Ahab version.
    pub version: Option<String>,
    pub created_at: OffsetDateTime,
    pub approved_at: Option<OffsetDateTime>,
    pub last_seen_at: Option<OffsetDateTime>,
}

impl Machine {
    /// The server's own host: active from the start, with no joining.
    pub fn local(name: MachineName, public_key: PublicKey, now: OffsetDateTime) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            state: MachineState::Active,
            is_local: true,
            public_key,
            os: None,
            arch: None,
            version: None,
            created_at: now,
            approved_at: Some(now),
            last_seen_at: None,
        }
    }

    /// Approves a pending machine; `false` when it was not pending.
    pub fn approve(&mut self, now: OffsetDateTime) -> bool {
        if self.state != MachineState::Pending {
            return false;
        }
        self.state = MachineState::Active;
        self.approved_at = Some(now);
        true
    }

    /// A machine that joined with a one-time code: approved at once.
    pub fn joined(name: MachineName, public_key: PublicKey, now: OffsetDateTime) -> Self {
        Self {
            is_local: false,
            ..Self::local(name, public_key, now)
        }
    }

    /// A machine that asked to join and waits for approval.
    pub fn pending(name: MachineName, public_key: PublicKey, now: OffsetDateTime) -> Self {
        Self {
            state: MachineState::Pending,
            is_local: false,
            approved_at: None,
            ..Self::local(name, public_key, now)
        }
    }
}

/// Whether a machine may connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    /// Asked to join; waits for approval.
    Pending,
    /// Approved: may connect.
    Active,
}

impl MachineState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            _ => None,
        }
    }
}

/// A machine's name: 1 to 63 lowercase letters, digits and hyphens, not
/// starting or ending with a hyphen, like a hostname label. Commands accept
/// it instead of an ID, so names are unique.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MachineName(String);

const NAME_MAX: usize = 63;

impl MachineName {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let invalid = |message: &str| DomainError::Invalid(vec![FieldError::new("name", message)]);
        if value.is_empty() {
            return Err(invalid("Enter a name."));
        }
        if value.len() > NAME_MAX {
            return Err(invalid("Use at most 63 characters."));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(invalid("Use lowercase letters, digits and hyphens."));
        }
        if value.starts_with('-') || value.ends_with('-') {
            return Err(invalid("Do not start or end the name with a hyphen."));
        }
        Ok(Self(value.to_owned()))
    }

    /// A name from a hostname: its first label, lowercased, with separators
    /// turned into hyphens and other characters dropped.
    /// `Alexs-MacBook.local` → `alexs-macbook`; an unusable hostname gives
    /// `machine`.
    pub fn from_hostname(hostname: &str) -> Self {
        let label = hostname.split('.').next().unwrap_or_default();
        let mut name = String::with_capacity(label.len());
        for c in label.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                name.push(c);
            } else if c.is_alphanumeric() || c == '\'' {
                // Letters Ahab names cannot hold, and apostrophes, are dropped.
            } else if !name.ends_with('-') {
                name.push('-');
            }
        }
        let name = name.trim_matches('-');
        let name: String = name.chars().take(NAME_MAX).collect();
        Self::parse(name.trim_end_matches('-')).unwrap_or_else(|_| Self("machine".to_owned()))
    }

    /// This name with `-<n>` appended, shortened to fit: for when the name is
    /// taken.
    pub fn numbered(&self, n: u32) -> Self {
        let suffix = format!("-{n}");
        let keep = NAME_MAX.saturating_sub(suffix.len());
        let base: String = self.0.chars().take(keep).collect();
        Self(format!("{}{suffix}", base.trim_end_matches('-')))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MachineName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A machine's Ed25519 public key: 32 bytes. The private key never leaves the
/// machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DomainError> {
        <[u8; 32]>::try_from(bytes).map(Self).map_err(|_| {
            DomainError::Invalid(vec![FieldError::new(
                "publicKey",
                "An Ed25519 public key has 32 bytes.",
            )])
        })
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// A short code to compare on both screens when approving a machine, such
    /// as `K7-4QX`: the start of the key's SHA-256 in base32.
    pub fn check_code(&self) -> String {
        let digest = Sha256::digest(self.0);
        let code: String = digest
            .iter()
            .take(6)
            .map(|byte| BASE32[usize::from(byte % 32)] as char)
            .collect();
        format!("{}-{}", &code[..2], &code[2..])
    }

    /// `SHA256:` and the key's SHA-256 in base64, as SSH shows fingerprints.
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint(format!(
            "SHA256:{}",
            STANDARD_NO_PAD.encode(Sha256::digest(self.0))
        ))
    }
}

/// Crockford's base32: no I, L, O or U, so codes read aloud stay clear.
pub const BASE32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// What people compare to recognise a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hostname_like_names() {
        for name in ["laptop", "build-box-2", "a", &"x".repeat(63)] {
            assert_eq!(MachineName::parse(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn rejects_other_names() {
        for name in [
            "",
            "Laptop",
            "build box",
            "-box",
            "box-",
            "café",
            &"x".repeat(64),
        ] {
            assert!(MachineName::parse(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn derives_a_name_from_a_hostname() {
        let cases = [
            ("atlas", "atlas"),
            ("Alexs-MacBook-Pro.local", "alexs-macbook-pro"),
            ("Alex's Laptop", "alexs-laptop"),
            ("build_box_01.lan", "build-box-01"),
            ("--odd--", "odd"),
            ("", "machine"),
            ("ünïcödé", "ncd"),
            ("ü", "machine"),
        ];
        for (hostname, name) in cases {
            assert_eq!(
                MachineName::from_hostname(hostname).as_str(),
                name,
                "{hostname}"
            );
        }
        let long = MachineName::from_hostname(&"a".repeat(80));
        assert_eq!(long.as_str().len(), 63);
    }

    #[test]
    fn numbers_a_taken_name_within_the_limit() {
        let name = MachineName::parse("laptop").unwrap();
        assert_eq!(name.numbered(2).as_str(), "laptop-2");
        let long = MachineName::parse(&"a".repeat(63)).unwrap();
        let numbered = long.numbered(12);
        assert_eq!(numbered.as_str().len(), 63);
        assert!(numbered.as_str().ends_with("-12"));
        assert!(MachineName::parse(numbered.as_str()).is_ok());
    }

    #[test]
    fn fingerprints_a_key_like_ssh() {
        let key = PublicKey::from_bytes(&[7; 32]).unwrap();
        let fingerprint = key.fingerprint();
        assert!(fingerprint.as_str().starts_with("SHA256:"));
        assert_eq!(fingerprint.as_str().len(), "SHA256:".len() + 43);
        assert_eq!(
            fingerprint,
            PublicKey::from_bytes(&[7; 32]).unwrap().fingerprint()
        );
        assert_ne!(
            fingerprint,
            PublicKey::from_bytes(&[8; 32]).unwrap().fingerprint()
        );
        assert!(PublicKey::from_bytes(&[1; 31]).is_err());
    }

    #[test]
    fn gives_each_key_a_short_check_code() {
        let code = PublicKey::from_bytes(&[7; 32]).unwrap().check_code();
        assert_eq!(code.len(), 7);
        assert_eq!(&code[2..3], "-");
        assert!(
            code.chars()
                .all(|c| c == '-' || BASE32.contains(&(c as u8)))
        );
        assert_eq!(code, PublicKey::from_bytes(&[7; 32]).unwrap().check_code());
        assert_ne!(code, PublicKey::from_bytes(&[8; 32]).unwrap().check_code());
    }

    #[test]
    fn approves_only_a_pending_machine() {
        let key = PublicKey::from_bytes(&[1; 32]).unwrap();
        let now = OffsetDateTime::now_utc();
        let mut pending = Machine::pending(MachineName::parse("box").unwrap(), key, now);
        assert!(pending.approve(now));
        assert_eq!(pending.state, MachineState::Active);
        assert_eq!(pending.approved_at, Some(now));
        assert!(!pending.approve(now));
    }

    #[test]
    fn local_machines_are_active_and_pending_ones_wait() {
        let key = PublicKey::from_bytes(&[1; 32]).unwrap();
        let now = OffsetDateTime::now_utc();
        let local = Machine::local(MachineName::parse("atlas").unwrap(), key, now);
        assert_eq!(local.state, MachineState::Active);
        assert!(local.is_local);
        assert_eq!(local.approved_at, Some(now));

        let pending = Machine::pending(MachineName::parse("box").unwrap(), key, now);
        assert_eq!(pending.state, MachineState::Pending);
        assert!(!pending.is_local);
        assert_eq!(pending.approved_at, None);
    }
}
