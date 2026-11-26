//! The machine key file: `machine.key` in the data folder, readable only by
//! its owner.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::modules::machines::identity::model::MachineKey;

const HEADER: &str = "ahab-machine-key-v1";

#[derive(Debug, thiserror::Error)]
pub enum KeyFileError {
    #[error("cannot read the machine key {}: {reason}", path.display())]
    Unreadable { path: PathBuf, reason: String },
    #[error("cannot create the machine key {}: {reason}", path.display())]
    Unwritable { path: PathBuf, reason: String },
}

/// The key in `path`, or a new one written there when there is none.
pub fn load_or_create(path: &Path) -> Result<MachineKey, KeyFileError> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text).ok_or_else(|| KeyFileError::Unreadable {
            path: path.to_owned(),
            reason: "not an Ahab machine key".to_owned(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create(path),
        Err(error) => Err(KeyFileError::Unreadable {
            path: path.to_owned(),
            reason: error.to_string(),
        }),
    }
}

fn parse(text: &str) -> Option<MachineKey> {
    let encoded = text.trim().strip_prefix(HEADER)?.trim();
    let bytes = STANDARD.decode(encoded).ok()?;
    let secret = <[u8; 32]>::try_from(bytes.as_slice()).ok()?;
    Some(MachineKey::from_secret(secret))
}

fn create(path: &Path) -> Result<MachineKey, KeyFileError> {
    let unwritable = |reason: String| KeyFileError::Unwritable {
        path: path.to_owned(),
        reason,
    };
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|error| unwritable(error.to_string()))?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|error| unwritable(error.to_string()))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| unwritable(error.to_string()))?;
    writeln!(file, "{HEADER} {}", STANDARD.encode(secret))
        .and_then(|()| file.sync_all())
        .map_err(|error| unwritable(error.to_string()))?;
    Ok(MachineKey::from_secret(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_key_once_and_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/machine.key");

        let created = load_or_create(&path).unwrap();
        let loaded = load_or_create(&path).unwrap();

        assert_eq!(created.public_key(), loaded.public_key());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn refuses_a_file_that_is_not_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machine.key");
        fs::write(&path, "hello").unwrap();

        assert!(matches!(
            load_or_create(&path),
            Err(KeyFileError::Unreadable { .. })
        ));
    }
}
