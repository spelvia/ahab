//! Which credential a client command sends: `AHAB_TOKEN`, else
//! the login for that server (`ahab login`, kept in `logins.json` in the
//! data folder, readable only by its owner), else, for the server on this
//! computer, its local credential file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli::exit::Exit;
use crate::cli::output;
use crate::config::{self, Paths};
use crate::modules::clients::client::store::{read_credential, write_private};

/// The format `logins.json` is written in.
const FORMAT: u32 = 1;

/// The command lines's logins, one per server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Logins {
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub servers: BTreeMap<String, Login>,
}

/// This command line's client on one server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub credential: String,
    /// Stored as `deviceId`, its name before devices were called clients,
    /// so existing logins still read.
    #[serde(default, rename = "deviceId")]
    pub client_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Where the credential came from, for what to say when it is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `AHAB_TOKEN`.
    Token,
    /// `ahab login`.
    Login,
    /// The local credential file of the server on this computer.
    Local(PathBuf),
}

/// A server address as a key: scheme, host and port in lower case, without
/// a path or a trailing slash.
pub fn server_key(server: &str) -> String {
    let server = server.trim().trim_end_matches('/').to_ascii_lowercase();
    match server.split_once("://") {
        Some((scheme, rest)) => {
            format!("{scheme}://{}", rest.split('/').next().unwrap_or_default())
        }
        None => server,
    }
}

/// Whether `server` is on this computer: `localhost` or a loopback address.
pub fn is_local(server: &str) -> bool {
    let key = server_key(server);
    let authority = key.split_once("://").map_or(key.as_str(), |(_, rest)| rest);
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
    };
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn logins_file(paths: &Paths) -> PathBuf {
    paths.data_dir.join("logins.json")
}

/// The logins kept on this computer; none when there is no file.
pub fn load(paths: &Paths) -> Logins {
    std::fs::read_to_string(logins_file(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn store(paths: &Paths, logins: &Logins) -> Result<(), String> {
    let text = serde_json::to_string_pretty(logins).map_err(|error| error.to_string())?;
    write_private(&logins_file(paths), &format!("{text}\n")).map_err(|error| error.to_string())
}

/// Keeps `login` for `server`.
pub fn save(paths: &Paths, server: &str, login: Login) -> Result<(), String> {
    let mut logins = load(paths);
    logins.format = FORMAT;
    logins.servers.insert(server_key(server), login);
    store(paths, &logins)
}

/// Forgets the login for `server`; the one forgotten, if there was one.
pub fn forget(paths: &Paths, server: &str) -> Result<Option<Login>, String> {
    let mut logins = load(paths);
    let removed = logins.servers.remove(&server_key(server));
    if removed.is_some() {
        logins.format = FORMAT;
        store(paths, &logins)?;
    }
    Ok(removed)
}

/// The credential to send to `server`, and where it came from.
pub fn find(server: &str, env: &config::Env<'_>) -> Option<(String, Source)> {
    if let Some(token) = env("AHAB_TOKEN") {
        return Some((token.trim().to_owned(), Source::Token));
    }
    let paths = Paths::resolve(env).ok()?;
    if let Some(login) = load(&paths).servers.get(&server_key(server)) {
        return Some((login.credential.clone(), Source::Login));
    }
    if is_local(server) {
        let file = local_file(&paths);
        if let Ok(Some(credential)) = read_credential(&file) {
            return Some((credential, Source::Local(file)));
        }
    }
    None
}

/// The local credential file the server on this computer writes.
pub fn local_file(paths: &Paths) -> PathBuf {
    paths.data_dir.join("credential")
}

/// Whether this runs in a terminal Ahab started (on any machine).
fn in_ahab_terminal(env: &config::Env<'_>) -> bool {
    env("AHAB_SESSION").is_some() || env("AHAB_PROJECT").is_some()
}

/// The failure when there is no credential for `server`: exit code 2, and
/// how to get one.
pub fn missing(server: &str, json: bool, env: &config::Env<'_>) -> Exit {
    let paths = Paths::resolve(env).ok();
    let mut detail = if is_local(server) {
        let file = paths.as_ref().map_or_else(
            || "the data folder".to_owned(),
            |paths| local_file(paths).display().to_string(),
        );
        format!(
            "The Ahab server on this computer writes its credential to {file} when it starts, \
             and there is none. Is Ahab running here (`ahab up`)? If {server} is another \
             server, run `ahab login {server}`."
        )
    } else {
        format!(
            "This command line is not paired with {server}. Run `ahab login {server}` to pair \
             it (a paired client approves it), or set AHAB_TOKEN to a token from Settings → \
             Clients."
        )
    };
    if in_ahab_terminal(env) && !is_local(server) {
        detail.push_str(
            " This terminal runs on a machine connected to that server: the ahab command here \
             needs its own login, once per machine.",
        );
    }
    output::failure(json, Exit::Usage, "Not paired", &detail)
}

/// What to say when `server` refused the credential from `source`.
pub fn refused_hint(server: &str, source: Option<&Source>) -> String {
    match source {
        Some(Source::Token) => {
            "AHAB_TOKEN was refused: the token may have been revoked. Create a new one in \
             Settings → Clients."
                .to_owned()
        }
        Some(Source::Local(file)) => format!(
            "The server refused this computer's credential ({}). If {server} is not the server \
             running on this computer, run `ahab login {server}`.",
            file.display()
        ),
        Some(Source::Login) => format!(
            "The server no longer accepts this command line's login; it may have been revoked. \
             Run `ahab login {server}` again."
        ),
        None => format!("Run `ahab login {server}` to pair this command line."),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn env_in(dir: &Path, token: Option<&str>) -> impl Fn(&str) -> Option<String> {
        let dir = dir.to_owned();
        let token = token.map(str::to_owned);
        move |name| match name {
            "AHAB_CONFIG_DIR" | "AHAB_DATA_DIR" => Some(dir.display().to_string()),
            "AHAB_TOKEN" => token.clone(),
            _ => None,
        }
    }

    #[test]
    fn tells_local_servers_apart() {
        assert!(is_local("http://127.0.0.1:8180"));
        assert!(is_local("http://localhost:8180/"));
        assert!(is_local("http://[::1]:8180"));
        assert!(!is_local("https://ahab.example"));
        assert!(!is_local("http://192.168.1.4:8180"));
        assert_eq!(
            server_key("HTTPS://Ahab.Example/x/"),
            "https://ahab.example"
        );
    }

    #[test]
    fn finds_the_token_then_the_login_then_the_local_credential() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), None);
        let paths = Paths::resolve(&env).unwrap();
        assert_eq!(find("https://ahab.example", &env), None);
        assert_eq!(find("http://127.0.0.1:8180", &env), None);

        write_private(&local_file(&paths), "ahab_local\n").unwrap();
        assert_eq!(
            find("http://127.0.0.1:8180", &env),
            Some(("ahab_local".to_owned(), Source::Local(local_file(&paths))))
        );
        // Never sent to another server.
        assert_eq!(find("https://ahab.example", &env), None);

        save(
            &paths,
            "https://ahab.example/",
            Login {
                credential: "ahab_login".into(),
                client_id: None,
                name: None,
            },
        )
        .unwrap();
        assert_eq!(
            find("https://AHAB.example", &env),
            Some(("ahab_login".to_owned(), Source::Login))
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(logins_file(&paths))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let with_token = env_in(dir.path(), Some("ahab_token"));
        assert_eq!(
            find("https://ahab.example", &with_token),
            Some(("ahab_token".to_owned(), Source::Token))
        );

        assert!(forget(&paths, "https://ahab.example").unwrap().is_some());
        assert!(forget(&paths, "https://ahab.example").unwrap().is_none());
        assert_eq!(find("https://ahab.example", &env), None);
    }
}
