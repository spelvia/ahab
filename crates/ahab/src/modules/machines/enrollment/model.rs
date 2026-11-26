//! One-time join codes and the rules of joining. No I/O here.

use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::modules::machines::machine::model::{BASE32, MachineName};

/// How long a machine may wait for approval before it must ask again.
pub fn pending_for() -> Duration {
    Duration::minutes(15)
}

/// How long a one-time code works.
pub fn code_valid_for() -> Duration {
    Duration::minutes(15)
}

/// At most this many machines wait for approval at once, so a stranger who
/// can reach the server cannot flood the list.
pub const MAX_PENDING: usize = 20;

/// Characters in a code, without the hyphen.
const CODE_LENGTH: usize = 8;

/// A one-time code: joining with it approves the machine at once. Only its
/// hash is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinCode {
    pub id: Uuid,
    pub code_hash: [u8; 32],
    /// The name the machine gets, if chosen in advance.
    pub name: Option<MachineName>,
    pub expires_at: OffsetDateTime,
    /// The machine that used it.
    pub used_by: Option<Uuid>,
    pub created_at: OffsetDateTime,
}

impl JoinCode {
    pub fn is_usable(&self, now: OffsetDateTime) -> bool {
        self.used_by.is_none() && now < self.expires_at
    }
}

/// A code as people type it, made comparable: hyphens and spaces dropped,
/// upper case, and the letters people confuse read as digits (`O` as `0`,
/// `I` and `L` as `1`). `None` when it cannot be a code.
pub fn normalize_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    (code.len() == CODE_LENGTH && code.bytes().all(|byte| BASE32.contains(&byte))).then_some(code)
}

/// A normalized code from random bytes: one base32 character per byte.
pub fn code_from_random(bytes: &[u8; CODE_LENGTH]) -> String {
    bytes
        .iter()
        .map(|byte| BASE32[usize::from(byte % 32)] as char)
        .collect()
}

/// A normalized code as shown: `ABCD-EFGH`.
pub fn format_code(code: &str) -> String {
    format!("{}-{}", &code[..4], &code[4..])
}

pub fn hash_code(normalized: &str) -> [u8; 32] {
    Sha256::digest(normalized.as_bytes()).into()
}

/// What a joining machine signs: its key, bound to the host it asked, so the
/// request proves it holds the private key.
pub fn enroll_message(host: &str, public_key: &str) -> Vec<u8> {
    format!("ahab/enroll/v1\n{host}\n{public_key}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codes_the_way_people_type_them() {
        assert_eq!(normalize_code("ABCD-EFGH").as_deref(), Some("ABCDEFGH"));
        assert_eq!(normalize_code("abcd efgh").as_deref(), Some("ABCDEFGH"));
        assert_eq!(normalize_code("0OIL-1234").as_deref(), Some("00111234"));
        assert_eq!(normalize_code("ABCD-EFG"), None);
        assert_eq!(normalize_code("ABCD-EFGU"), None);
    }

    #[test]
    fn makes_codes_from_the_base32_alphabet() {
        let code = code_from_random(&[0, 31, 32, 255, 7, 8, 9, 10]);
        assert_eq!(code.len(), 8);
        assert_eq!(
            normalize_code(&format_code(&code)).as_deref(),
            Some(code.as_str())
        );
        assert_eq!(&format_code(&code)[4..5], "-");
    }

    #[test]
    fn a_code_works_once_and_not_after_it_expires() {
        let now = OffsetDateTime::now_utc();
        let mut code = JoinCode {
            id: Uuid::nil(),
            code_hash: hash_code("ABCDEFGH"),
            name: None,
            expires_at: now + code_valid_for(),
            used_by: None,
            created_at: now,
        };
        assert!(code.is_usable(now));
        assert!(!code.is_usable(now + Duration::minutes(16)));
        code.used_by = Some(Uuid::nil());
        assert!(!code.is_usable(now));
    }
}
