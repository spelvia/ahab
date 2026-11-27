//! `config.toml`: the settings saved on this computer. Every key is optional;
//! a missing file is an empty one.

use std::fs;
use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::ConfigError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Run this computer's daemon with the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon: Option<bool>,
    /// Serve the web app (the API is always served).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen_addr: Option<SocketAddr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,
    /// Names the server answers to besides `localhost` and IP addresses,
    /// such as the name a reverse proxy serves it under.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
    /// The address people use to reach the server, such as
    /// `https://ahab.example`: its host is allowed, and pairing links use
    /// it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    /// More origins whose pages may call the API, such as the desktop app's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_origins: Vec<String>,
    /// Who may set forwarding headers (`X-Forwarded-For`, `Forwarded`):
    /// addresses or ranges. Absent: loopback only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_proxies: Option<Vec<String>>,
    /// Accept client credentials over plain HTTP to public addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_insecure_http: Option<bool>,
    /// The server this computer joined (`ahab setup`): this computer then
    /// runs only its daemon, connected there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// That server's key fingerprint, pinned when joining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_fingerprint: Option<String>,
    /// `[plugins]`: how plugins run here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugins: Option<PluginsSection>,
    /// `[update]`: checking for a newer Ahab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<UpdateSection>,
}

/// `[update]` in `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSection {
    /// Check GitHub for a newer release on start and daily (on by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<bool>,
    /// `stable` (default) or `pre`, which counts prereleases too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Where releases are listed (GitHub's API for Ahab's repository).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// `[plugins]` in `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginsSection {
    /// The Node (24 or later) that runs and builds plugins; `node` on `PATH`
    /// when absent. A login service may not see the shell's `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<std::path::PathBuf>,
}

const HEADER: &str = "# Ahab's settings on this computer. `ahab up --daemon=… --web=…` updates\n# them; flags and AHAB_* environment variables override them.\n\n";

impl ConfigFile {
    /// Reads `path`; a missing file is an empty configuration.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|error| ConfigError::Unreadable {
                path: path.to_owned(),
                reason: error.to_string(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ConfigError::Unreadable {
                path: path.to_owned(),
                reason: error.to_string(),
            }),
        }
    }

    /// Writes `path`, creating its folder, through a temporary file so a
    /// failed write never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let unwritable = |error: &dyn std::fmt::Display| ConfigError::Unwritable {
            path: path.to_owned(),
            reason: error.to_string(),
        };
        let text = toml::to_string(self).map_err(|error| unwritable(&error))?;
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|error| unwritable(&error))?;
        }
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, format!("{HEADER}{text}")).map_err(|error| unwritable(&error))?;
        fs::rename(&temporary, path).map_err(|error| unwritable(&error))
    }
}
