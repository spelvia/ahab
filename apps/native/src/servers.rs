//! The servers this app is paired with: each a client of kind
//! `app` on its server, with its credential in the keychain under the
//! server's ID. The list itself (`servers.json`) holds no secret.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::secrets::{SecretError, SecretStore, write_private};

/// One paired server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    /// What the switcher shows: the host, or "This computer".
    pub name: String,
    /// Its origin: `https://ahab.example`, `http://127.0.0.1:8180`.
    pub url: String,
    /// The server this computer runs.
    #[serde(default)]
    pub local: bool,
    /// This app's client on that server, to revoke it when removed.
    /// Stored as `deviceId`, its name before devices were called clients.
    #[serde(default, rename = "deviceId")]
    pub client_id: Option<String>,
    /// The last page shown, restored when it opens again.
    #[serde(default)]
    pub last_path: Option<String>,
}

/// What `servers.json` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerList {
    #[serde(default = "format")]
    pub format: u32,
    #[serde(default)]
    pub servers: Vec<Server>,
    /// The server opened last.
    #[serde(default)]
    pub last: Option<String>,
}

const FORMAT: u32 = 1;

fn format() -> u32 {
    FORMAT
}

#[derive(Debug, thiserror::Error)]
pub enum ServersError {
    #[error("cannot read or write the server list: {0}")]
    Io(#[from] std::io::Error),
    #[error("the server list is damaged: {0}")]
    Corrupt(String),
    #[error("{0}")]
    Secret(#[from] SecretError),
    #[error("there is no such server")]
    NotFound,
}

/// The list in a file, with the credentials in a [`SecretStore`].
pub struct Servers {
    file: PathBuf,
    secrets: Box<dyn SecretStore>,
}

impl Servers {
    pub fn new(dir: &Path, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            file: dir.join("servers.json"),
            secrets,
        }
    }

    pub fn secrets(&self) -> &dyn SecretStore {
        self.secrets.as_ref()
    }

    pub fn load(&self) -> Result<ServerList, ServersError> {
        match std::fs::read_to_string(&self.file) {
            Ok(text) => {
                let list: ServerList = serde_json::from_str(&text)
                    .map_err(|error| ServersError::Corrupt(error.to_string()))?;
                if list.format > FORMAT {
                    return Err(ServersError::Corrupt(format!(
                        "written by a newer app (format {})",
                        list.format
                    )));
                }
                Ok(list)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ServerList {
                format: FORMAT,
                ..ServerList::default()
            }),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, list: &ServerList) -> Result<(), ServersError> {
        let text = serde_json::to_vec_pretty(list)
            .map_err(|error| ServersError::Corrupt(error.to_string()))?;
        write_private(&self.file, &text)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Server, ServersError> {
        self.load()?
            .servers
            .into_iter()
            .find(|server| server.id == id)
            .ok_or(ServersError::NotFound)
    }

    pub fn find_by_url(&self, url: &str) -> Result<Option<Server>, ServersError> {
        Ok(self
            .load()?
            .servers
            .into_iter()
            .find(|server| server.url == url))
    }

    /// Adds a newly paired server, or replaces the credential of the one
    /// at the same address (paired again). Returns it.
    pub fn add(
        &self,
        url: &str,
        name: &str,
        local: bool,
        client_id: Option<String>,
        credential: &str,
    ) -> Result<Server, ServersError> {
        let mut list = self.load()?;
        let server =
            if let Some(existing) = list.servers.iter_mut().find(|server| server.url == url) {
                existing.client_id = client_id;
                existing.local |= local;
                existing.clone()
            } else {
                let server = Server {
                    id: uuid::Uuid::now_v7().to_string(),
                    name: name.to_owned(),
                    url: url.to_owned(),
                    local,
                    client_id,
                    last_path: None,
                };
                list.servers.push(server.clone());
                server
            };
        // The credential first: a listed server always has one.
        self.secrets.set(&server.id, credential)?;
        list.last = Some(server.id.clone());
        self.save(&list)?;
        Ok(server)
    }

    pub fn credential(&self, id: &str) -> Result<Option<String>, ServersError> {
        Ok(self.secrets.get(id)?)
    }

    /// Forgets a server and its credential.
    pub fn remove(&self, id: &str) -> Result<Server, ServersError> {
        let mut list = self.load()?;
        let index = list
            .servers
            .iter()
            .position(|server| server.id == id)
            .ok_or(ServersError::NotFound)?;
        let server = list.servers.remove(index);
        if list.last.as_deref() == Some(id) {
            list.last = list.servers.first().map(|server| server.id.clone());
        }
        self.save(&list)?;
        self.secrets.delete(id)?;
        Ok(server)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), ServersError> {
        self.update(id, |server| name.clone_into(&mut server.name))
    }

    /// Remembers the server and page a window showed last.
    pub fn visited(&self, id: &str, path: &str) -> Result<(), ServersError> {
        let mut list = self.load()?;
        let Some(server) = list.servers.iter_mut().find(|server| server.id == id) else {
            return Err(ServersError::NotFound);
        };
        if server.last_path.as_deref() == Some(path) && list.last.as_deref() == Some(id) {
            return Ok(());
        }
        server.last_path = Some(path.to_owned());
        list.last = Some(id.to_owned());
        self.save(&list)
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Server)) -> Result<(), ServersError> {
        let mut list = self.load()?;
        let server = list
            .servers
            .iter_mut()
            .find(|server| server.id == id)
            .ok_or(ServersError::NotFound)?;
        change(server);
        self.save(&list)
    }
}

/// The name a server gets in the list: "This computer" for the local one,
/// else its host.
pub fn default_name(url: &str, local: bool) -> String {
    if local {
        return "This computer".to_owned();
    }
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::Keychain;

    fn servers(dir: &Path) -> Servers {
        Servers::new(
            dir,
            Box::new(Keychain::new(keyring_core::mock::Store::new().unwrap())),
        )
    }

    #[test]
    fn keeps_servers_and_their_credentials_apart() {
        let dir = tempfile::tempdir().unwrap();
        let servers = servers(dir.path());
        assert!(servers.load().unwrap().servers.is_empty());

        let atlas = servers
            .add(
                "https://atlas.lan",
                "atlas.lan",
                false,
                Some("d1".into()),
                "ahab_a",
            )
            .unwrap();
        let local = servers
            .add(
                "http://127.0.0.1:8180",
                "This computer",
                true,
                None,
                "ahab_b",
            )
            .unwrap();
        let list = servers.load().unwrap();
        assert_eq!(list.servers.len(), 2);
        assert_eq!(list.last.as_deref(), Some(local.id.as_str()));
        assert_eq!(
            servers.credential(&atlas.id).unwrap().as_deref(),
            Some("ahab_a")
        );

        // No secret in the file.
        let text = std::fs::read_to_string(dir.path().join("servers.json")).unwrap();
        assert!(
            !text.contains("ahab_a") && !text.contains("ahab_b"),
            "{text}"
        );

        // Pairing the same address again replaces its credential, not the entry.
        let again = servers
            .add(
                "https://atlas.lan",
                "atlas",
                false,
                Some("d2".into()),
                "ahab_c",
            )
            .unwrap();
        assert_eq!(again.id, atlas.id);
        assert_eq!(again.client_id.as_deref(), Some("d2"));
        assert_eq!(
            servers.credential(&atlas.id).unwrap().as_deref(),
            Some("ahab_c")
        );

        servers.visited(&atlas.id, "/p/1").unwrap();
        assert_eq!(
            servers.get(&atlas.id).unwrap().last_path.as_deref(),
            Some("/p/1")
        );
        assert_eq!(
            servers.load().unwrap().last.as_deref(),
            Some(atlas.id.as_str())
        );

        servers.rename(&atlas.id, "Homelab").unwrap();
        assert_eq!(servers.get(&atlas.id).unwrap().name, "Homelab");

        let removed = servers.remove(&atlas.id).unwrap();
        assert_eq!(removed.url, "https://atlas.lan");
        assert_eq!(servers.credential(&atlas.id).unwrap(), None);
        assert_eq!(
            servers.load().unwrap().last.as_deref(),
            Some(local.id.as_str())
        );
        assert!(matches!(
            servers.remove(&atlas.id),
            Err(ServersError::NotFound)
        ));
    }

    #[test]
    fn refuses_a_list_from_a_newer_app() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("servers.json"),
            r#"{"format":9,"servers":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            servers(dir.path()).load(),
            Err(ServersError::Corrupt(_))
        ));
    }

    #[test]
    fn tolerates_fields_it_does_not_know() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("servers.json"),
            r#"{"format":1,"servers":[{"id":"a","name":"x","url":"https://x","color":"red"}],"later":1}"#,
        )
        .unwrap();
        assert_eq!(servers(dir.path()).load().unwrap().servers[0].name, "x");
    }

    #[test]
    fn names_servers_by_host() {
        assert_eq!(default_name("https://atlas.lan:8443", false), "atlas.lan");
        assert_eq!(default_name("http://127.0.0.1:8180", true), "This computer");
    }
}
