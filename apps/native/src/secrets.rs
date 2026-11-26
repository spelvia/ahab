//! Where the app keeps each server's credential: the system
//! keychain (Secret Service on Linux), or, where none answers (a session
//! without a keyring, such as a headless one), an encrypted file in the
//! app's data folder.
//!
//! The file is encrypted with a key kept beside it, both readable only by
//! their owner (0600). That keeps credentials out of backups and casual
//! reads of one file, not from someone who can read the whole folder as
//! you: the same protection as the command line's `logins.json`. Settings
//! says which store is in use.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use keyring_core::CredentialStore;
use serde::{Deserialize, Serialize};

/// The keychain entries' service name.
pub const SERVICE: &str = "dev.ahab.native";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the keychain refused: {0}")]
    Keychain(String),
    #[error("cannot use the credential file: {0}")]
    File(#[from] std::io::Error),
    #[error("the credential file cannot be read: {0}")]
    Corrupt(String),
}

/// Which store keeps the credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StoreKind {
    Keychain,
    EncryptedFile,
}

pub trait SecretStore: Send + Sync {
    fn kind(&self) -> StoreKind;
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// The system keychain, through `keyring-core` (tests give it the mock
/// store).
pub struct Keychain {
    store: Arc<CredentialStore>,
}

impl Keychain {
    pub fn new(store: Arc<CredentialStore>) -> Self {
        Self { store }
    }

    /// This computer's keychain, when one answers.
    pub fn system() -> Result<Self, SecretError> {
        #[cfg(target_os = "linux")]
        {
            let store = zbus_secret_service_keyring_store::Store::new()
                .map_err(|error| SecretError::Keychain(error.to_string()))?;
            Ok(Self::new(store))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(SecretError::Keychain(
                "no keychain support on this platform yet".to_owned(),
            ))
        }
    }

    fn entry(&self, key: &str) -> Result<keyring_core::Entry, SecretError> {
        self.store
            .build(SERVICE, key, None)
            .map_err(|error| SecretError::Keychain(error.to_string()))
    }

    /// Writes, reads and deletes a probe: a keychain that is there but
    /// cannot be used (no session bus, no keyring daemon) fails here.
    pub fn probe(&self) -> Result<(), SecretError> {
        let key = "probe";
        self.set(key, "ok")?;
        let read = self.get(key)?;
        self.delete(key)?;
        if read.as_deref() == Some("ok") {
            Ok(())
        } else {
            Err(SecretError::Keychain(
                "the keychain did not keep a value".to_owned(),
            ))
        }
    }
}

impl SecretStore for Keychain {
    fn kind(&self) -> StoreKind {
        StoreKind::Keychain
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(SecretError::Keychain(error.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.entry(key)?
            .set_password(value)
            .map_err(|error| SecretError::Keychain(error.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(SecretError::Keychain(error.to_string())),
        }
    }
}

/// The fallback: `credentials.json` with each value encrypted
/// (XChaCha20-Poly1305) by the key in `credentials.key`, both 0600.
pub struct EncryptedFile {
    dir: PathBuf,
    lock: Mutex<()>,
}

const FILE_FORMAT: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Sealed {
    format: u32,
    /// Base64 of nonce and ciphertext, by key.
    entries: BTreeMap<String, String>,
}

impl EncryptedFile {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
            lock: Mutex::new(()),
        }
    }

    fn data_file(&self) -> PathBuf {
        self.dir.join("credentials.json")
    }

    fn key_file(&self) -> PathBuf {
        self.dir.join("credentials.key")
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, SecretError> {
        let path = self.key_file();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut key = [0u8; 32];
                getrandom::fill(&mut key)
                    .map_err(|error| SecretError::Corrupt(error.to_string()))?;
                write_private(&path, &key)?;
                key.to_vec()
            }
            Err(error) => return Err(error.into()),
        };
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| SecretError::Corrupt("the key file is not 32 bytes".to_owned()))?;
        Ok(XChaCha20Poly1305::new(&Key::from(key)))
    }

    fn load(&self) -> Result<Sealed, SecretError> {
        match std::fs::read_to_string(self.data_file()) {
            Ok(text) => {
                let sealed: Sealed = serde_json::from_str(&text)
                    .map_err(|error| SecretError::Corrupt(error.to_string()))?;
                if sealed.format > FILE_FORMAT {
                    return Err(SecretError::Corrupt(format!(
                        "written by a newer app (format {})",
                        sealed.format
                    )));
                }
                Ok(sealed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Sealed {
                format: FILE_FORMAT,
                ..Sealed::default()
            }),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, sealed: &Sealed) -> Result<(), SecretError> {
        let text = serde_json::to_vec_pretty(sealed)
            .map_err(|error| SecretError::Corrupt(error.to_string()))?;
        write_private(&self.data_file(), &text)?;
        Ok(())
    }
}

impl SecretStore for EncryptedFile {
    fn kind(&self) -> StoreKind {
        StoreKind::EncryptedFile
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sealed = self.load()?;
        let Some(value) = sealed.entries.get(key) else {
            return Ok(None);
        };
        let bytes = STANDARD
            .decode(value)
            .map_err(|error| SecretError::Corrupt(error.to_string()))?;
        if bytes.len() < 24 {
            return Err(SecretError::Corrupt("an entry is too short".to_owned()));
        }
        let (nonce, ciphertext) = bytes.split_at(24);
        let nonce = XNonce::try_from(nonce)
            .map_err(|_| SecretError::Corrupt("an entry's nonce is wrong".to_owned()))?;
        let plain = self
            .cipher()?
            .decrypt(&nonce, ciphertext)
            .map_err(|_| SecretError::Corrupt("an entry does not decrypt".to_owned()))?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|error| SecretError::Corrupt(error.to_string()))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut sealed = self.load()?;
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|error| SecretError::Corrupt(error.to_string()))?;
        let ciphertext = self
            .cipher()?
            .encrypt(&XNonce::from(nonce), value.as_bytes())
            .map_err(|_| SecretError::Corrupt("cannot encrypt".to_owned()))?;
        let mut bytes = nonce.to_vec();
        bytes.extend_from_slice(&ciphertext);
        sealed.format = FILE_FORMAT;
        sealed
            .entries
            .insert(key.to_owned(), STANDARD.encode(bytes));
        self.save(&sealed)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut sealed = self.load()?;
        if sealed.entries.remove(key).is_some() {
            self.save(&sealed)?;
        }
        Ok(())
    }
}

/// Writes `bytes` to `path` readable only by its owner, through a
/// temporary file renamed into place.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &temporary,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )?;
    std::fs::rename(&temporary, path)
}

/// The keychain when it works, else the encrypted file in `dir`, and why.
pub fn open(dir: &Path) -> (Box<dyn SecretStore>, Option<String>) {
    match Keychain::system().and_then(|keychain| keychain.probe().map(|()| keychain)) {
        Ok(keychain) => (Box::new(keychain), None),
        Err(error) => {
            tracing::warn!(%error, "no usable keychain; keeping credentials in an encrypted file");
            (
                Box::new(EncryptedFile::new(dir)),
                Some(format!(
                    "No system keychain answered ({error}), so credentials are kept in an \
                     encrypted file in the app's data folder, readable only by you."
                )),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(store: &dyn SecretStore) {
        assert_eq!(store.get("server-1").unwrap(), None);
        store.set("server-1", "ahab_secret").unwrap();
        store.set("server-2", "other").unwrap();
        assert_eq!(
            store.get("server-1").unwrap().as_deref(),
            Some("ahab_secret")
        );
        store.set("server-1", "replaced").unwrap();
        assert_eq!(store.get("server-1").unwrap().as_deref(), Some("replaced"));
        store.delete("server-1").unwrap();
        store.delete("server-1").unwrap();
        assert_eq!(store.get("server-1").unwrap(), None);
        assert_eq!(store.get("server-2").unwrap().as_deref(), Some("other"));
    }

    #[test]
    fn the_keychain_keeps_credentials() {
        let keychain = Keychain::new(keyring_core::mock::Store::new().unwrap());
        keychain.probe().unwrap();
        assert_eq!(keychain.kind(), StoreKind::Keychain);
        exercise(&keychain);
    }

    #[test]
    fn the_encrypted_file_keeps_credentials_privately_and_unreadably() {
        let dir = tempfile::tempdir().unwrap();
        let file = EncryptedFile::new(dir.path());
        assert_eq!(file.kind(), StoreKind::EncryptedFile);
        exercise(&file);

        let text = std::fs::read_to_string(dir.path().join("credentials.json")).unwrap();
        assert!(!text.contains("other"), "{text}");
        #[cfg(unix)]
        for name in ["credentials.json", "credentials.key"] {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
        // Another instance (the next start) reads what this one wrote.
        let again = EncryptedFile::new(dir.path());
        assert_eq!(again.get("server-2").unwrap().as_deref(), Some("other"));
    }

    #[test]
    fn the_encrypted_file_refuses_a_wrong_key_or_newer_format() {
        let dir = tempfile::tempdir().unwrap();
        let file = EncryptedFile::new(dir.path());
        file.set("server-1", "secret").unwrap();
        std::fs::write(dir.path().join("credentials.key"), [7u8; 32]).unwrap();
        assert!(matches!(file.get("server-1"), Err(SecretError::Corrupt(_))));

        std::fs::write(
            dir.path().join("credentials.json"),
            r#"{"format":2,"entries":{}}"#,
        )
        .unwrap();
        assert!(matches!(file.get("server-1"), Err(SecretError::Corrupt(_))));
    }

    /// The real keychain of this session; run with `--ignored` where one
    /// is unlocked. It writes and deletes one test entry.
    #[test]
    #[ignore = "uses this session's keychain"]
    fn the_system_keychain_works_here() {
        let keychain = Keychain::system().unwrap();
        keychain.probe().unwrap();
        keychain.set("test-entry", "value").unwrap();
        assert_eq!(
            keychain.get("test-entry").unwrap().as_deref(),
            Some("value")
        );
        keychain.delete("test-entry").unwrap();
    }
}
