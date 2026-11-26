//! Web Push: a browser's subscription, the server's VAPID
//! key (RFC 8292), and the encrypted message a push service carries
//! (RFC 8291, `aes128gcm`, RFC 8188).

use aws_lc_rs::aead::{AES_128_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use aws_lc_rs::agreement::{self, ECDH_P256, PrivateKey, UnparsedPublicKey};
use aws_lc_rs::hkdf::{HKDF_SHA256, KeyType, Prk, Salt};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::notifications::notification::model::Notification;
use crate::shared::error::FieldError;

/// A browser that asked for pushes, through its push service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: Uuid,
    /// The client that registered it: revoking the client ends it.
    pub client_id: Uuid,
    /// Where the push service takes messages for this browser.
    pub endpoint: String,
    /// The browser's P-256 public key (65 bytes, uncompressed).
    pub p256dh: Vec<u8>,
    /// The browser's 16-byte authentication secret.
    pub auth: Vec<u8>,
    /// "Firefox on Linux", from the browser.
    pub label: String,
    pub created_at: OffsetDateTime,
    pub last_used_at: Option<OffsetDateTime>,
}

/// What a browser sends to subscribe, checked.
#[derive(Debug, Clone)]
pub struct NewSubscription {
    pub endpoint: String,
    pub p256dh: Vec<u8>,
    pub auth: Vec<u8>,
    pub label: String,
}

impl NewSubscription {
    /// Checks what `PushSubscription.toJSON()` gave: an `https` endpoint
    /// and the keys, base64url-encoded.
    pub fn parse(
        endpoint: &str,
        p256dh: &str,
        auth: &str,
        label: &str,
    ) -> Result<Self, FieldError> {
        if !endpoint.starts_with("https://")
            || origin(endpoint).is_none()
            || endpoint.chars().any(char::is_whitespace)
        {
            return Err(FieldError::new(
                "endpoint",
                "A push service is reached over https.",
            ));
        }
        let p256dh = decode(p256dh)
            .filter(|key| key.len() == 65 && key[0] == 4)
            .ok_or_else(|| FieldError::new("keys.p256dh", "Not a P-256 public key."))?;
        let auth = decode(auth)
            .filter(|secret| secret.len() == 16)
            .ok_or_else(|| FieldError::new("keys.auth", "Not a 16-byte secret."))?;
        let label = label.trim();
        let label = if label.is_empty() {
            "A browser".to_owned()
        } else {
            label.chars().take(80).collect()
        };
        Ok(Self {
            endpoint: endpoint.to_owned(),
            p256dh,
            auth,
            label,
        })
    }
}

/// `scheme://host[:port]` of an http(s) URL: the push service a VAPID
/// token is for (its `aud`); `None` for anything else, or with a user name.
pub fn origin(endpoint: &str) -> Option<String> {
    let (scheme, rest) = endpoint.split_once("://")?;
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() || host.contains('@') {
        return None;
    }
    Some(format!("{scheme}://{}", host.to_ascii_lowercase()))
}

fn decode(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).ok()
}

/// Base64url without padding, as Web Push writes keys.
pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The server's VAPID key: push services know messages are from this
/// server by it, and browsers subscribe with its public half.
pub struct VapidKey {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
}

impl std::fmt::Debug for VapidKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VapidKey")
            .field("public", &self.public())
            .finish_non_exhaustive()
    }
}

/// A key that could not be made or read.
#[derive(Debug, thiserror::Error)]
#[error("the push key is unreadable")]
pub struct KeyError;

impl VapidKey {
    pub fn generate() -> Result<Self, KeyError> {
        let document =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .map_err(|_| KeyError)?;
        Self::from_pkcs8(document.as_ref())
    }

    /// The key as kept on disk (PKCS #8, DER).
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, KeyError> {
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8)
            .map_err(|_| KeyError)?;
        Ok(Self {
            pair,
            pkcs8: pkcs8.to_vec(),
        })
    }

    pub fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    /// The public key browsers subscribe with (`applicationServerKey`).
    pub fn public(&self) -> String {
        encode(self.pair.public_key().as_ref())
    }

    /// The `Authorization` header for a message to `endpoint`, valid for
    /// 12 hours from `now` (RFC 8292).
    pub fn authorization(&self, endpoint: &str, now: OffsetDateTime) -> Result<String, KeyError> {
        let audience = origin(endpoint).ok_or(KeyError)?;
        let header = encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = encode(
            json!({
                "aud": audience,
                "exp": now.unix_timestamp() + 12 * 60 * 60,
                "sub": SUBJECT,
            })
            .to_string()
            .as_bytes(),
        );
        let signed = format!("{header}.{claims}");
        let signature = self
            .pair
            .sign(&SystemRandom::new(), signed.as_bytes())
            .map_err(|_| KeyError)?;
        Ok(format!(
            "vapid t={signed}.{}, k={}",
            encode(signature.as_ref()),
            self.public()
        ))
    }
}

/// Who push services may contact about this server's messages (RFC 8292
/// asks for one); Ahab's project page, never the person's address.
const SUBJECT: &str = "https://github.com/spelvia/ahab";

/// The size of the one record a message is (RFC 8188's `rs`).
const RECORD_SIZE: u32 = 4096;

/// Encryption failed: the browser's keys are not usable.
#[derive(Debug, thiserror::Error)]
#[error("the subscription's keys cannot be used")]
pub struct EncryptError;

struct Len(usize);

impl KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(prk: &Prk, info: &[&[u8]], len: usize) -> Result<Vec<u8>, EncryptError> {
    let mut out = vec![0; len];
    prk.expand(info, Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| EncryptError)?;
    Ok(out)
}

/// The keys for one message from the shared secret (RFC 8291 section 3.4).
fn content_keys(
    shared: &[u8],
    auth: &[u8],
    browser: &[u8],
    server: &[u8],
    salt: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), EncryptError> {
    let key_info: &[&[u8]] = &[b"WebPush: info\0", browser, server];
    let ikm = expand(&Salt::new(HKDF_SHA256, auth).extract(shared), key_info, 32)?;
    let prk = Salt::new(HKDF_SHA256, salt).extract(&ikm);
    let key = expand(&prk, &[b"Content-Encoding: aes128gcm\0"], 16)?;
    let nonce = expand(&prk, &[b"Content-Encoding: nonce\0"], 12)?;
    Ok((key, nonce))
}

/// `plaintext` encrypted for the browser with `p256dh` and `auth`: the body
/// of a Web Push message, `Content-Encoding: aes128gcm`.
pub fn encrypt(p256dh: &[u8], auth: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, EncryptError> {
    let random = SystemRandom::new();
    let ours = PrivateKey::generate(&ECDH_P256).map_err(|_| EncryptError)?;
    let mut salt = [0_u8; 16];
    random.fill(&mut salt).map_err(|_| EncryptError)?;
    seal(&ours, &salt, p256dh, auth, plaintext)
}

/// `encrypt` with a given key and salt (fresh ones for every message).
fn seal(
    ours: &PrivateKey,
    salt: &[u8; 16],
    p256dh: &[u8],
    auth: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, EncryptError> {
    let public = ours.compute_public_key().map_err(|_| EncryptError)?;
    let public = public.as_ref().to_vec();
    let shared = agreement::agree(
        ours,
        UnparsedPublicKey::new(&ECDH_P256, p256dh),
        EncryptError,
        |shared| Ok(shared.to_vec()),
    )?;
    let (key, nonce) = content_keys(&shared, auth, p256dh, &public, salt)?;
    let key = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &key).map_err(|_| EncryptError)?);
    let nonce = Nonce::try_assume_unique_for_key(&nonce).map_err(|_| EncryptError)?;
    // One record, the last: the content, then the delimiter 2.
    let mut record = plaintext.to_vec();
    record.push(2);
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut record)
        .map_err(|_| EncryptError)?;
    let mut body = Vec::with_capacity(16 + 4 + 1 + public.len() + record.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(u8::try_from(public.len()).map_err(|_| EncryptError)?);
    body.extend_from_slice(&public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// The most a notification's text takes in a message: push services take
/// about 4 KB, encrypted.
const MOST_TEXT: usize = 1000;

/// The page of this server a notification opens, as the web app's routes
/// name it; `None` for one with nothing to open here.
pub fn target_path(target: &Value) -> Option<String> {
    let text = |key: &str| target.get(key).and_then(Value::as_str);
    match text("type")? {
        "session" => Some(format!(
            "/p/{}/s/{}",
            text("projectId")?,
            text("sessionId")?
        )),
        "machine" => Some(format!("/settings/machines/{}", text("machineId")?)),
        _ => None,
    }
}

/// The body, and on a line of its own where a chat's notification happened.
fn with_place(notification: &Notification) -> Option<String> {
    let name = |field: &str| {
        notification
            .target
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let place = crate::modules::notifications::notification::model::ChatPlace {
        project: name("projectName"),
        session: name("sessionTitle"),
    };
    // The session's title often is the body already (an agent finished).
    let place = crate::modules::notifications::notification::model::ChatPlace {
        session: place
            .session
            .filter(|title| notification.body.as_deref() != Some(title.as_str())),
        ..place
    };
    match (notification.body.clone(), place.line()) {
        (Some(body), Some(line)) => Some(format!("{body}\n{line}")),
        (body, None) => body,
        (None, line) => line,
    }
}

/// What the service worker gets for `notification` (`public/push-worker.js`).
pub fn payload(notification: &Notification) -> Vec<u8> {
    let short = |text: &str| text.chars().take(MOST_TEXT).collect::<String>();
    let url = (notification.target.get("type").and_then(Value::as_str) == Some("url"))
        .then(|| notification.target.get("url").and_then(Value::as_str))
        .flatten();
    json!({
        "id": notification.id,
        "kind": notification.kind,
        "title": short(&notification.title),
        // Under the body: where it happened (`ahab · stubb · Fix the test`).
        "body": with_place(notification).as_deref().map(short),
        "tag": notification.group_key.clone().unwrap_or_else(|| notification.id.to_string()),
        "path": target_path(&notification.target),
        "url": url,
    })
    .to_string()
    .into_bytes()
}

/// What a test push says.
pub fn test_payload() -> Vec<u8> {
    json!({
        "id": Uuid::now_v7(),
        "kind": "ahab.test",
        "title": "Ahab can reach this device",
        "body": "Notifications arrive here even when Ahab is closed.",
        "tag": "ahab.test",
        "path": "/settings/notifications",
    })
    .to_string()
    .into_bytes()
}

/// What a push service's answer means for the subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// 404 or 410: the browser unsubscribed; the subscription is removed.
    Gone,
    Failed,
}

pub fn delivery(status: u16) -> Delivery {
    match status {
        200..=299 => Delivery::Sent,
        404 | 410 => Delivery::Gone,
        _ => Delivery::Failed,
    }
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey as Verifying};

    use super::*;

    fn chat_notification(body: Option<&str>, target: Value) -> Notification {
        Notification {
            id: Uuid::now_v7(),
            kind: "sessions.finished".to_owned(),
            source: "ahab".to_owned(),
            level: crate::modules::notifications::notification::model::Level::Success,
            title: "Codex finished".to_owned(),
            body: body.map(str::to_owned),
            project_id: None,
            target,
            group_key: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            read_at: None,
        }
    }

    #[test]
    fn a_chat_notification_says_where_it_happened() {
        let target = json!({ "type": "session", "projectName": "ahab",
                             "sessionTitle": "Fix the test" });
        // The session's title is not said twice.
        let finished = chat_notification(Some("Fix the test"), target.clone());
        assert_eq!(with_place(&finished).as_deref(), Some("Fix the test\nahab"));
        let asked = chat_notification(Some("May I run make?"), target);
        assert_eq!(
            with_place(&asked).as_deref(),
            Some("May I run make?\nahab · Fix the test")
        );
        // Older ones, without names, are as they were.
        let older = chat_notification(Some("Done"), json!({ "type": "session" }));
        assert_eq!(with_place(&older).as_deref(), Some("Done"));
    }

    /// What a browser does with the message: the reverse of `encrypt`.
    fn decrypt(browser: &PrivateKey, auth: &[u8], body: &[u8]) -> Vec<u8> {
        let salt = &body[..16];
        assert_eq!(
            u32::from_be_bytes(body[16..20].try_into().unwrap()),
            RECORD_SIZE
        );
        let length = usize::from(body[20]);
        let server = &body[21..21 + length];
        let ours = browser.compute_public_key().unwrap();
        let shared = agreement::agree(
            browser,
            UnparsedPublicKey::new(&ECDH_P256, server),
            (),
            |shared| Ok(shared.to_vec()),
        )
        .unwrap();
        let (key, nonce) = content_keys(&shared, auth, ours.as_ref(), server, salt).unwrap();
        let key = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &key).unwrap());
        let mut record = body[21 + length..].to_vec();
        let plain = key
            .open_in_place(
                Nonce::try_assume_unique_for_key(&nonce).unwrap(),
                Aad::empty(),
                &mut record,
            )
            .unwrap();
        assert_eq!(plain.last(), Some(&2), "the last record's delimiter");
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn a_browser_reads_what_was_encrypted_for_it() {
        let browser = PrivateKey::generate(&ECDH_P256).unwrap();
        let public = browser.compute_public_key().unwrap();
        let auth = [9_u8; 16];
        let body = encrypt(public.as_ref(), &auth, b"{\"title\":\"Done\"}").unwrap();
        assert_eq!(decrypt(&browser, &auth, &body), b"{\"title\":\"Done\"}");
        // Each message has its own salt and key.
        assert_ne!(
            encrypt(public.as_ref(), &auth, b"x").unwrap()[..16],
            body[..16]
        );
    }

    /// RFC 8291, Appendix A.
    #[test]
    fn the_rfc_example_encrypts_as_written() {
        let ours = PrivateKey::from_private_key(
            &ECDH_P256,
            &decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw").unwrap(),
        )
        .unwrap();
        let salt: [u8; 16] = decode("DGv6ra1nlYgDCS1FRnbzlw")
            .unwrap()
            .try_into()
            .unwrap();
        let browser = decode(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        )
        .unwrap();
        let auth = decode("BTBZMqHH6r4Tts7J_aSIgg").unwrap();
        let body = seal(
            &ours,
            &salt,
            &browser,
            &auth,
            b"When I grow up, I want to be a watermelon",
        )
        .unwrap();
        assert_eq!(
            encode(&body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn the_vapid_header_is_a_signed_token_for_the_push_service() {
        let key = VapidKey::generate().unwrap();
        let again = VapidKey::from_pkcs8(key.pkcs8()).unwrap();
        assert_eq!(again.public(), key.public());
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let header = key
            .authorization("https://push.example.org/send/abc?x=1", now)
            .unwrap();
        let (token, public) = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        assert_eq!(public, key.public());
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let claims: Value =
            serde_json::from_slice(&decode(signed.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.org");
        assert_eq!(claims["exp"], 1_800_000_000 + 43_200);
        Verifying::new(&ECDSA_P256_SHA256_FIXED, decode(public).unwrap())
            .verify(signed.as_bytes(), &decode(signature).unwrap())
            .unwrap();
    }

    #[test]
    fn a_subscription_needs_https_and_real_keys() {
        let key = encode(&[4; 65]);
        let auth = encode(&[1; 16]);
        assert!(
            NewSubscription::parse("https://fcm.googleapis.com/fcm/send/x", &key, &auth, "")
                .is_ok()
        );
        assert!(NewSubscription::parse("http://push.example/x", &key, &auth, "").is_err());
        assert!(
            NewSubscription::parse("https://push.example/x", &encode(&[4; 10]), &auth, "").is_err()
        );
        assert!(
            NewSubscription::parse("https://push.example/x", &key, &encode(&[1; 8]), "").is_err()
        );
        assert!(NewSubscription::parse("https://user@push.example/x", &key, &auth, "").is_err());
        assert_eq!(
            origin("https://Push.Example:8443/a?b").as_deref(),
            Some("https://push.example:8443")
        );
        let fine = NewSubscription::parse("https://push.example/x", &key, &auth, "  ").unwrap();
        assert_eq!(fine.label, "A browser");
    }

    #[test]
    fn what_opening_a_push_shows() {
        let session = json!({ "type": "session", "projectId": "p", "sessionId": "s" });
        assert_eq!(target_path(&session).as_deref(), Some("/p/p/s/s"));
        assert_eq!(
            target_path(&json!({ "type": "machine", "machineId": "m" })).as_deref(),
            Some("/settings/machines/m")
        );
        assert_eq!(
            target_path(&json!({ "type": "url", "url": "https://x" })),
            None
        );
        assert_eq!(delivery(201), Delivery::Sent);
        assert_eq!(delivery(410), Delivery::Gone);
        assert_eq!(delivery(429), Delivery::Failed);
    }
}
