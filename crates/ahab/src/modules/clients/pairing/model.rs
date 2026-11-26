//! Pairing links, codes and requests, and the limits on guessing them. No I/O here.

use std::collections::{HashMap, VecDeque};

use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::modules::clients::client::model::ClientKind;

/// How long a link or code from Add client works.
pub fn link_valid_for() -> Duration {
    Duration::minutes(10)
}

/// How long the link the server prints at its first start works.
pub fn first_run_valid_for() -> Duration {
    Duration::hours(24)
}

/// How long a new client's request waits for an answer.
pub fn request_valid_for() -> Duration {
    Duration::minutes(10)
}

/// At most this many requests wait at once, so a stranger who can reach
/// the server cannot flood the approvals.
pub const MAX_OPEN_REQUESTS: usize = 20;

/// The characters codes are made of: no `O`, `I`, `L` or `U`, which people
/// confuse.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Characters in a link's code (`K7QM-4RXT`): typed on a new client, so it
/// is a secret.
pub const LINK_CODE_LENGTH: usize = 8;

/// Characters in a request's code (`7F3K`): shown by the new client and
/// compared by whoever approves it; not a secret.
pub const REQUEST_CODE_LENGTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingKind {
    /// A paired client or the server offers: whoever opens it is paired.
    Link,
    /// A new client asks, and waits for approval.
    Request,
}

impl PairingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Request => "request",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "link" => Some(Self::Link),
            "request" => Some(Self::Request),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    Open,
    /// A request was approved; its client takes its credential next.
    Approved,
    Denied,
    /// It paired a client.
    Used,
    /// It ran out of time, or was withdrawn.
    Expired,
}

impl PairingState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Used => "used",
            Self::Expired => "expired",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "open" => Some(Self::Open),
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            "used" => Some(Self::Used),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

/// A pairing link (with its code) or a new client's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub id: Uuid,
    pub kind: PairingKind,
    /// The link's secret, or the waiting secret of a request.
    pub secret_hash: [u8; 32],
    /// The normalized code's hash.
    pub code_hash: Option<[u8; 32]>,
    /// A request's code, which approvers see. A link's code is never kept.
    pub code: Option<String>,
    /// The client that offered a link; `None` when the server printed it.
    pub created_by: Option<Uuid>,
    /// The client that approved a request.
    pub approved_by: Option<Uuid>,
    /// For requests: what the approver sees.
    pub client_name: Option<String>,
    pub client_kind: Option<ClientKind>,
    pub user_agent: Option<String>,
    pub address: Option<String>,
    pub state: PairingState,
    /// The client it paired.
    pub client_id: Option<Uuid>,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

impl Pairing {
    /// Still waiting to be used or answered.
    pub fn is_open(&self, now: OffsetDateTime) -> bool {
        self.state == PairingState::Open && now < self.expires_at
    }

    /// The link the server printed while no client was paired.
    pub fn is_first_run(&self) -> bool {
        self.kind == PairingKind::Link && self.created_by.is_none()
    }

    /// The state as it is at `now`: open past its time is expired.
    pub fn state_at(&self, now: OffsetDateTime) -> PairingState {
        match self.state {
            PairingState::Open | PairingState::Approved if now >= self.expires_at => {
                PairingState::Expired
            }
            state => state,
        }
    }
}

/// A code as people type it, made comparable: hyphens and spaces dropped,
/// upper case, and the letters people confuse read as digits (`O` as `0`,
/// `I` and `L` as `1`). `None` when it cannot be a code of either length.
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
    let length_fits = code.len() == LINK_CODE_LENGTH || code.len() == REQUEST_CODE_LENGTH;
    (length_fits && code.bytes().all(|byte| ALPHABET.contains(&byte))).then_some(code)
}

/// A normalized code from random bytes: one character per byte.
pub fn code_from_random(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(byte % 32)] as char)
        .collect()
}

/// A normalized code as shown: `K7QM-4RXT`; short codes as they are.
pub fn format_code(code: &str) -> String {
    if code.len() == LINK_CODE_LENGTH {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_owned()
    }
}

/// Tries allowed per address in a minute.
pub const TRIES_PER_MINUTE: usize = 10;

/// Wrong codes in an hour, from anywhere, after which every open code is
/// withdrawn.
pub const WRONG_CODES_PER_HOUR: usize = 50;

/// The longest an address is kept waiting.
fn longest_wait() -> Duration {
    Duration::hours(1)
}

#[derive(Debug, Default)]
struct Tries {
    recent: VecDeque<OffsetDateTime>,
    blocked_until: Option<OffsetDateTime>,
    /// How often it was blocked lately: each time, it waits twice as long.
    strikes: u32,
    last_strike: Option<OffsetDateTime>,
}

/// The limits on trying secrets (pairing links and codes): ten tries a
/// minute per address, then a wait that doubles each time (up to an hour);
/// and a count of wrong codes from everywhere. Kept in memory: a restart
/// forgets it, as it forgets the codes' attempts.
#[derive(Debug, Default)]
pub struct Limiter {
    by_address: HashMap<String, Tries>,
    wrong_codes: VecDeque<OffsetDateTime>,
}

impl Limiter {
    /// Counts a try from `address`: `Err` with how long to wait when it is
    /// one too many.
    pub fn attempt(&mut self, address: &str, now: OffsetDateTime) -> Result<(), Duration> {
        if self.by_address.len() > 10_000 {
            self.forget_quiet(now);
        }
        let tries = self.by_address.entry(address.to_owned()).or_default();
        if let Some(until) = tries.blocked_until {
            if now < until {
                return Err(until - now);
            }
            tries.blocked_until = None;
        }
        while tries
            .recent
            .front()
            .is_some_and(|&at| at + Duration::minutes(1) <= now)
        {
            tries.recent.pop_front();
        }
        if tries.recent.len() < TRIES_PER_MINUTE {
            tries.recent.push_back(now);
            return Ok(());
        }
        if tries
            .last_strike
            .is_some_and(|at| at + longest_wait() <= now)
        {
            tries.strikes = 0;
        }
        tries.strikes = tries.strikes.saturating_add(1);
        tries.last_strike = Some(now);
        let wait = (Duration::minutes(1) * 2_i32.saturating_pow(tries.strikes.min(10) - 1))
            .min(longest_wait());
        tries.blocked_until = Some(now + wait);
        tries.recent.clear();
        Err(wait)
    }

    /// Counts a wrong code: `true` when that makes too many in the last
    /// hour, and every open code should be withdrawn.
    pub fn wrong_code(&mut self, now: OffsetDateTime) -> bool {
        while self
            .wrong_codes
            .front()
            .is_some_and(|&at| at + Duration::hours(1) <= now)
        {
            self.wrong_codes.pop_front();
        }
        self.wrong_codes.push_back(now);
        if self.wrong_codes.len() >= WRONG_CODES_PER_HOUR {
            self.wrong_codes.clear();
            return true;
        }
        false
    }

    /// Forgets addresses that are neither waiting nor tried lately.
    fn forget_quiet(&mut self, now: OffsetDateTime) {
        self.by_address.retain(|_, tries| {
            tries.blocked_until.is_some_and(|until| now < until)
                || tries
                    .recent
                    .back()
                    .is_some_and(|&at| at + Duration::minutes(1) > now)
                || tries
                    .last_strike
                    .is_some_and(|at| at + longest_wait() > now)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codes_the_way_people_type_them() {
        assert_eq!(normalize_code("k7qm-4rxt").as_deref(), Some("K7QM4RXT"));
        assert_eq!(normalize_code("7f3k").as_deref(), Some("7F3K"));
        assert_eq!(normalize_code("0OIL").as_deref(), Some("0011"));
        assert_eq!(normalize_code("ABC"), None);
        assert_eq!(normalize_code("ABCD-EFGU"), None);
        assert_eq!(format_code("K7QM4RXT"), "K7QM-4RXT");
        assert_eq!(format_code("7F3K"), "7F3K");
        let code = code_from_random(&[0, 31, 32, 255]);
        assert_eq!(normalize_code(&code).as_deref(), Some(code.as_str()));
    }

    #[test]
    fn allows_ten_tries_a_minute_then_waits_longer_each_time() {
        let mut limiter = Limiter::default();
        let start = OffsetDateTime::now_utc();
        for _ in 0..TRIES_PER_MINUTE {
            assert_eq!(limiter.attempt("10.0.0.9", start), Ok(()));
        }
        assert_eq!(
            limiter.attempt("10.0.0.9", start),
            Err(Duration::minutes(1))
        );
        // Another address is not held up.
        assert_eq!(limiter.attempt("10.0.0.10", start), Ok(()));
        // Still waiting.
        assert!(
            limiter
                .attempt("10.0.0.9", start + Duration::seconds(30))
                .is_err()
        );
        // After the wait, ten more; the next block lasts twice as long.
        let later = start + Duration::minutes(1);
        for _ in 0..TRIES_PER_MINUTE {
            assert_eq!(limiter.attempt("10.0.0.9", later), Ok(()));
        }
        assert_eq!(
            limiter.attempt("10.0.0.9", later),
            Err(Duration::minutes(2))
        );
    }

    #[test]
    fn too_many_wrong_codes_withdraw_them_all() {
        let mut limiter = Limiter::default();
        let now = OffsetDateTime::now_utc();
        for _ in 0..WRONG_CODES_PER_HOUR - 1 {
            assert!(!limiter.wrong_code(now));
        }
        assert!(limiter.wrong_code(now));
        // An hour later the count starts again.
        let mut spread = Limiter::default();
        for minute in 0..WRONG_CODES_PER_HOUR * 2 {
            let at = now + Duration::minutes(i64::try_from(minute).unwrap() * 2);
            assert!(!spread.wrong_code(at));
        }
    }

    #[test]
    fn open_until_it_expires() {
        let now = OffsetDateTime::now_utc();
        let pairing = Pairing {
            id: Uuid::nil(),
            kind: PairingKind::Link,
            secret_hash: [0; 32],
            code_hash: None,
            code: None,
            created_by: None,
            approved_by: None,
            client_name: None,
            client_kind: None,
            user_agent: None,
            address: None,
            state: PairingState::Open,
            client_id: None,
            expires_at: now + link_valid_for(),
            created_at: now,
        };
        assert!(pairing.is_open(now));
        assert!(pairing.is_first_run());
        assert_eq!(pairing.state_at(now), PairingState::Open);
        let later = now + Duration::minutes(11);
        assert!(!pairing.is_open(later));
        assert_eq!(pairing.state_at(later), PairingState::Expired);
    }
}
