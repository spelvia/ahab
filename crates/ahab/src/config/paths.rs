//! Where Ahab keeps its configuration and data on this computer.

use std::path::PathBuf;

use super::{ConfigError, Env};

/// The folders Ahab uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `config.toml`. Linux: `~/.config/ahab`; macOS:
    /// `~/Library/Application Support/ahab`. `AHAB_CONFIG_DIR` overrides it.
    pub config_dir: PathBuf,
    /// The database and keys. Linux: `~/.local/share/ahab`; macOS:
    /// `~/Library/Application Support/ahab`. `AHAB_DATA_DIR` overrides it.
    pub data_dir: PathBuf,
}

impl Paths {
    /// The folders for this computer: the overrides from `env`, else the
    /// platform's (which respect `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`).
    pub fn resolve(env: &Env<'_>) -> Result<Self, ConfigError> {
        let platform = directories::ProjectDirs::from("", "", "ahab");
        let config_dir = match env("AHAB_CONFIG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => platform
                .as_ref()
                .map(|dirs| dirs.config_dir().to_owned())
                .ok_or(ConfigError::NoHome)?,
        };
        let data_dir = match env("AHAB_DATA_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => platform
                .as_ref()
                .map(|dirs| dirs.data_dir().to_owned())
                .ok_or(ConfigError::NoHome)?,
        };
        Ok(Self {
            config_dir,
            data_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The default SQLite database.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("ahab.db")
    }
}
