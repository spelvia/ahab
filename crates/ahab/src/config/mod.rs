//! The settings of Ahab on this computer, and where it keeps its files.
//!
//! Each setting comes from, in order: a command-line flag, an `AHAB_*`
//! environment variable (or `DATABASE_URL`), `config.toml`, the default.
//! `--daemon` and `--web` given on the command line are saved to
//! `config.toml`, so they stay set.

pub mod file;
pub mod paths;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::ValueEnum;

use crate::shared::net::{IpRange, LOOPBACK};

pub use file::ConfigFile;
pub use paths::Paths;

/// Looks up an environment variable. Passed in, so tests need no real
/// environment.
pub type Env<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// The real environment.
pub fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

pub const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:8180";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot find the home folder; set AHAB_CONFIG_DIR and AHAB_DATA_DIR")]
    NoHome,
    #[error("cannot read {}: {reason}", path.display())]
    Unreadable { path: PathBuf, reason: String },
    #[error("cannot write {}: {reason}", path.display())]
    Unwritable { path: PathBuf, reason: String },
    #[error("{name} is {value:?}: {expected}")]
    InvalidValue {
        name: &'static str,
        value: String,
        expected: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    /// Readable text, for development.
    Text,
    /// One JSON object per line, for production.
    Json,
}

/// What the command line was given; `None` when a flag was not used.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    pub daemon: Option<bool>,
    pub web: Option<bool>,
    pub listen_addr: Option<SocketAddr>,
    pub database_url: Option<String>,
    pub ui_dir: Option<PathBuf>,
    pub log_format: Option<LogFormat>,
}

/// The settings Ahab runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub paths: Paths,
    /// Run this computer's daemon with the server.
    pub daemon: bool,
    /// Serve the web app. The API is always served.
    pub web: bool,
    pub listen_addr: SocketAddr,
    /// Names the server answers to besides `localhost` and IP addresses
    /// (`allowed_hosts`, or `AHAB_ALLOWED_HOSTS` separated by commas).
    pub allowed_hosts: Vec<String>,
    /// The address people use, such as `https://ahab.example`, without a
    /// trailing slash (`public_url`, `AHAB_PUBLIC_URL`).
    pub public_url: Option<String>,
    /// More origins whose pages may call the API (`allowed_origins`,
    /// `AHAB_ALLOWED_ORIGINS`), each as `scheme://host[:port]`.
    pub allowed_origins: Vec<String>,
    /// Who may set forwarding headers (`trusted_proxies`,
    /// `AHAB_TRUSTED_PROXIES`); loopback by default.
    pub trusted_proxies: Vec<IpRange>,
    /// Accept client credentials over plain HTTP to public addresses
    /// (`allow_insecure_http`, `AHAB_ALLOW_INSECURE_HTTP`).
    pub allow_insecure_http: bool,
    pub database_url: String,
    /// A built web app to serve instead of the embedded one (development).
    pub ui_dir: Option<PathBuf>,
    pub log_format: LogFormat,
    /// The server this computer joined; it then runs only its daemon.
    pub joined: Option<Joined>,
    /// The Node that runs and builds plugins (`[plugins] node`); `node` on
    /// `PATH` when absent.
    pub plugin_node: Option<PathBuf>,
    /// Checking for a newer Ahab (`[update]`, `AHAB_UPDATE_CHECK`).
    pub update: UpdateCheck,
}

/// Checking for a newer Ahab release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub enabled: bool,
    /// Prereleases count too (`channel = "pre"`).
    pub prereleases: bool,
    /// GitHub's list of Ahab's releases.
    pub url: String,
}

/// Where Ahab's releases are listed until the repository's home is decided;
/// `[update] url` sets another.
pub const RELEASES_URL: &str = "https://api.github.com/repos/spelvia/ahab/releases";

impl Default for UpdateCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            prereleases: false,
            url: RELEASES_URL.to_owned(),
        }
    }
}

/// A server this computer joined with `ahab setup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    /// Such as `https://ahab.example.internal`.
    pub server: String,
    /// Its key fingerprint, pinned when joining.
    pub fingerprint: String,
}

/// The settings, and whether flags were saved to `config.toml`.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    /// The file the flags were saved to, when they were.
    pub saved_to: Option<PathBuf>,
}

/// Puts the settings together from `flags`, `env` and `config.toml`, and
/// saves `--daemon` and `--web` when they were given.
pub fn load(flags: &Flags, env: &Env<'_>) -> Result<Loaded, ConfigError> {
    let paths = Paths::resolve(env)?;
    let mut file = ConfigFile::load(&paths.config_file())?;

    let mut saved_to = None;
    if flags.daemon.is_some() || flags.web.is_some() {
        file.daemon = flags.daemon.or(file.daemon);
        file.web = flags.web.or(file.web);
        file.save(&paths.config_file())?;
        saved_to = Some(paths.config_file());
    }

    let daemon = match flags.daemon {
        Some(daemon) => daemon,
        None => env_bool(env, "AHAB_DAEMON")?
            .or(file.daemon)
            .unwrap_or(true),
    };
    let web = match flags.web {
        Some(web) => web,
        None => env_bool(env, "AHAB_WEB")?.or(file.web).unwrap_or(true),
    };
    let listen_addr = match flags.listen_addr {
        Some(addr) => addr,
        None => match env_parse(env, "AHAB_LISTEN_ADDR", "an address such as 127.0.0.1:8180")? {
            Some(addr) => addr,
            None => match file.listen_addr {
                Some(addr) => addr,
                None => DEFAULT_LISTEN_ADDR
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue {
                        name: "the default listen address",
                        value: DEFAULT_LISTEN_ADDR.to_owned(),
                        expected: "an address",
                    })?,
            },
        },
    };
    let allowed_hosts = match env("AHAB_ALLOWED_HOSTS") {
        Some(names) => list(&names),
        None => file.allowed_hosts.clone(),
    };
    let public_url = env("AHAB_PUBLIC_URL")
        .map(|url| (url, "AHAB_PUBLIC_URL"))
        .or_else(|| file.public_url.clone().map(|url| (url, "public_url")))
        .map(|(url, name)| {
            normalize_url(&url).ok_or(ConfigError::InvalidValue {
                name,
                value: url,
                expected: "an address such as https://ahab.example",
            })
        })
        .transpose()?;
    let allowed_origins = match env("AHAB_ALLOWED_ORIGINS") {
        Some(origins) => (list(&origins), "AHAB_ALLOWED_ORIGINS"),
        None => (file.allowed_origins.clone(), "allowed_origins"),
    };
    let allowed_origins = allowed_origins
        .0
        .iter()
        .map(|origin| {
            normalize_url(origin).ok_or_else(|| ConfigError::InvalidValue {
                name: allowed_origins.1,
                value: origin.clone(),
                expected: "origins such as https://app.example or tauri://localhost",
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trusted_proxies = match env("AHAB_TRUSTED_PROXIES") {
        Some(ranges) => Some((list(&ranges), "AHAB_TRUSTED_PROXIES")),
        None => file
            .trusted_proxies
            .clone()
            .map(|ranges| (ranges, "trusted_proxies")),
    };
    let trusted_proxies = match trusted_proxies {
        None => LOOPBACK.to_vec(),
        Some((ranges, name)) => ranges
            .iter()
            .map(|range| {
                range.parse().map_err(|_| ConfigError::InvalidValue {
                    name,
                    value: range.clone(),
                    expected: "addresses or ranges such as 127.0.0.1 or 10.0.0.0/8",
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let allow_insecure_http = env_bool(env, "AHAB_ALLOW_INSECURE_HTTP")?
        .or(file.allow_insecure_http)
        .unwrap_or(false);
    let database_url = flags
        .database_url
        .clone()
        .or_else(|| env("DATABASE_URL"))
        .or(file.database_url.clone())
        .unwrap_or_else(|| {
            format!(
                "sqlite://{}?mode=rwc",
                paths.database_file().to_string_lossy()
            )
        });
    let ui_dir = flags
        .ui_dir
        .clone()
        .or_else(|| env("AHAB_UI_DIR").map(PathBuf::from));
    let log_format = match flags.log_format {
        Some(format) => format,
        None => match env("AHAB_LOG_FORMAT").as_deref() {
            None | Some("text") => LogFormat::Text,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(ConfigError::InvalidValue {
                    name: "AHAB_LOG_FORMAT",
                    value: other.to_owned(),
                    expected: "text or json",
                });
            }
        },
    };

    let joined = match (&file.server, &file.server_fingerprint) {
        (Some(server), Some(fingerprint)) => Some(Joined {
            server: server.clone(),
            fingerprint: fingerprint.clone(),
        }),
        _ => None,
    };
    Ok(Loaded {
        settings: Settings {
            paths,
            daemon,
            web,
            listen_addr,
            allowed_hosts,
            public_url,
            allowed_origins,
            trusted_proxies,
            allow_insecure_http,
            database_url,
            ui_dir,
            log_format,
            joined,
            plugin_node: file
                .plugins
                .as_ref()
                .and_then(|plugins| plugins.node.clone()),
            update: UpdateCheck {
                enabled: match env_bool(env, "AHAB_UPDATE_CHECK")? {
                    Some(enabled) => enabled,
                    None => file
                        .update
                        .as_ref()
                        .and_then(|update| update.check)
                        .unwrap_or(true),
                },
                prereleases: file
                    .update
                    .as_ref()
                    .and_then(|update| update.channel.as_deref())
                    == Some("pre"),
                url: file
                    .update
                    .as_ref()
                    .and_then(|update| update.url.clone())
                    .unwrap_or_else(|| RELEASES_URL.to_owned()),
            },
        },
        saved_to,
    })
}

/// The server a client command talks to when `--server` and `AHAB_SERVER`
/// are not given: the server this computer joined, else the one it runs, at
/// the address `config.toml` sets (or the default). A wildcard address means
/// this computer's loopback.
pub fn local_server_url(env: &Env<'_>) -> String {
    let file = Paths::resolve(env)
        .ok()
        .and_then(|paths| ConfigFile::load(&paths.config_file()).ok())
        .unwrap_or_default();
    // A computer that joined another server talks to that one.
    if let Some(server) = file.server {
        return server;
    }
    let saved = file.listen_addr;
    let mut addr = env_parse::<SocketAddr>(env, "AHAB_LISTEN_ADDR", "")
        .ok()
        .flatten()
        .or(saved)
        .or_else(|| DEFAULT_LISTEN_ADDR.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8180)));
    if addr.ip().is_unspecified() {
        addr.set_ip(if addr.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    format!("http://{addr}")
}

/// A list in an environment variable: values separated by commas.
fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

/// An address or origin as `scheme://host[:port][/path]`, without a
/// trailing slash; `None` when it has no scheme and host.
pub fn normalize_url(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/');
    let (scheme, rest) = text.split_once("://")?;
    let valid_scheme = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let host = rest.split('/').next().unwrap_or_default();
    (valid_scheme && !host.is_empty() && !host.contains(char::is_whitespace))
        .then(|| format!("{}://{rest}", scheme.to_ascii_lowercase()))
}

fn env_bool(env: &Env<'_>, name: &'static str) -> Result<Option<bool>, ConfigError> {
    env_parse(env, name, "true or false")
}

fn env_parse<T: std::str::FromStr>(
    env: &Env<'_>,
    name: &'static str,
    expected: &'static str,
) -> Result<Option<T>, ConfigError> {
    env(name)
        .map(|value| {
            value.parse().map_err(|_| ConfigError::InvalidValue {
                name,
                value,
                expected,
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// An environment with Ahab's folders in `dir`, plus `vars`.
    fn env_in<'a>(
        dir: &'a std::path::Path,
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        let vars: HashMap<&str, &str> = vars.iter().copied().collect();
        move |name| match name {
            "AHAB_CONFIG_DIR" => Some(dir.join("config").to_string_lossy().into_owned()),
            "AHAB_DATA_DIR" => Some(dir.join("data").to_string_lossy().into_owned()),
            other => vars.get(other).map(|value| (*value).to_owned()),
        }
    }

    #[test]
    fn uses_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(&Flags::default(), &env_in(dir.path(), &[])).unwrap();
        let settings = loaded.settings;

        assert!(settings.daemon);
        assert!(settings.web);
        assert_eq!(settings.listen_addr.to_string(), "127.0.0.1:8180");
        assert_eq!(
            settings.database_url,
            format!(
                "sqlite://{}?mode=rwc",
                dir.path().join("data/ahab.db").display()
            )
        );
        assert_eq!(settings.log_format, LogFormat::Text);
        assert_eq!(loaded.saved_to, None);
        assert!(!dir.path().join("config/config.toml").exists());
    }

    #[test]
    fn a_flag_beats_the_environment_which_beats_the_file() {
        let dir = tempfile::tempdir().unwrap();
        ConfigFile {
            daemon: Some(false),
            web: Some(false),
            listen_addr: Some("127.0.0.1:9000".parse().unwrap()),
            database_url: Some("sqlite://file.db".into()),
            ..ConfigFile::default()
        }
        .save(&dir.path().join("config/config.toml"))
        .unwrap();

        let file_only = load(&Flags::default(), &env_in(dir.path(), &[])).unwrap();
        assert!(!file_only.settings.daemon);
        assert!(!file_only.settings.web);
        assert_eq!(file_only.settings.listen_addr.to_string(), "127.0.0.1:9000");
        assert_eq!(file_only.settings.database_url, "sqlite://file.db");

        let vars = [
            ("AHAB_DAEMON", "true"),
            ("AHAB_LISTEN_ADDR", "127.0.0.1:9100"),
            ("DATABASE_URL", "sqlite://env.db"),
        ];
        let from_env = load(&Flags::default(), &env_in(dir.path(), &vars)).unwrap();
        assert!(from_env.settings.daemon);
        assert_eq!(from_env.settings.listen_addr.to_string(), "127.0.0.1:9100");
        assert_eq!(from_env.settings.database_url, "sqlite://env.db");

        let flags = Flags {
            listen_addr: Some("127.0.0.1:9200".parse().unwrap()),
            database_url: Some("sqlite://flag.db".into()),
            ..Flags::default()
        };
        let from_flags = load(&flags, &env_in(dir.path(), &vars)).unwrap();
        assert_eq!(
            from_flags.settings.listen_addr.to_string(),
            "127.0.0.1:9200"
        );
        assert_eq!(from_flags.settings.database_url, "sqlite://flag.db");
    }

    #[test]
    fn saves_daemon_and_web_flags() {
        let dir = tempfile::tempdir().unwrap();
        let flags = Flags {
            daemon: Some(false),
            ..Flags::default()
        };

        let loaded = load(&flags, &env_in(dir.path(), &[])).unwrap();
        assert_eq!(loaded.saved_to, Some(dir.path().join("config/config.toml")));

        // The next start without flags keeps it.
        let next = load(&Flags::default(), &env_in(dir.path(), &[])).unwrap();
        assert!(!next.settings.daemon);
        assert!(next.settings.web);
        assert_eq!(next.saved_to, None);

        // Turning the web app off keeps the daemon setting.
        let web_off = Flags {
            web: Some(false),
            ..Flags::default()
        };
        load(&web_off, &env_in(dir.path(), &[])).unwrap();
        let file = ConfigFile::load(&dir.path().join("config/config.toml")).unwrap();
        assert_eq!(file.daemon, Some(false));
        assert_eq!(file.web, Some(false));
    }

    #[test]
    fn reads_the_access_settings() {
        let dir = tempfile::tempdir().unwrap();
        let defaults = load(&Flags::default(), &env_in(dir.path(), &[]))
            .unwrap()
            .settings;
        assert_eq!(defaults.public_url, None);
        assert!(defaults.allowed_origins.is_empty());
        assert_eq!(defaults.trusted_proxies, LOOPBACK.to_vec());
        assert!(!defaults.allow_insecure_http);

        ConfigFile {
            public_url: Some("https://Ahab.Example/".into()),
            allowed_origins: vec!["tauri://localhost".into()],
            trusted_proxies: Some(vec!["10.0.0.0/8".into(), "192.168.1.5".into()]),
            allow_insecure_http: Some(true),
            ..ConfigFile::default()
        }
        .save(&dir.path().join("config/config.toml"))
        .unwrap();
        let file = load(&Flags::default(), &env_in(dir.path(), &[]))
            .unwrap()
            .settings;
        assert_eq!(file.public_url.as_deref(), Some("https://Ahab.Example"));
        assert_eq!(file.allowed_origins, vec!["tauri://localhost".to_owned()]);
        assert_eq!(file.trusted_proxies.len(), 2);
        assert!(file.allow_insecure_http);

        let vars = [
            ("AHAB_PUBLIC_URL", "http://ahab.lan:8180"),
            (
                "AHAB_ALLOWED_ORIGINS",
                "https://a.example, https://b.example",
            ),
            ("AHAB_TRUSTED_PROXIES", ""),
            ("AHAB_ALLOW_INSECURE_HTTP", "false"),
        ];
        let from_env = load(&Flags::default(), &env_in(dir.path(), &vars))
            .unwrap()
            .settings;
        assert_eq!(from_env.public_url.as_deref(), Some("http://ahab.lan:8180"));
        assert_eq!(from_env.allowed_origins.len(), 2);
        assert!(!from_env.allow_insecure_http);
    }

    #[test]
    fn rejects_invalid_environment_values() {
        let dir = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("AHAB_DAEMON", "maybe"),
            ("AHAB_LISTEN_ADDR", "nowhere"),
            ("AHAB_LOG_FORMAT", "xml"),
            ("AHAB_PUBLIC_URL", "ahab.example"),
            ("AHAB_TRUSTED_PROXIES", "10.0.0.0/99"),
            ("AHAB_ALLOW_INSECURE_HTTP", "sure"),
            ("AHAB_ALLOWED_ORIGINS", "nope"),
        ] {
            let vars = [(name, value)];
            let error = load(&Flags::default(), &env_in(dir.path(), &vars)).unwrap_err();
            assert!(
                matches!(error, ConfigError::InvalidValue { .. }),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn finds_the_local_server() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            local_server_url(&env_in(dir.path(), &[])),
            "http://127.0.0.1:8180"
        );

        ConfigFile {
            listen_addr: Some("0.0.0.0:9000".parse().unwrap()),
            ..ConfigFile::default()
        }
        .save(&dir.path().join("config/config.toml"))
        .unwrap();
        assert_eq!(
            local_server_url(&env_in(dir.path(), &[])),
            "http://127.0.0.1:9000"
        );

        let vars = [("AHAB_LISTEN_ADDR", "[::]:9100")];
        assert_eq!(
            local_server_url(&env_in(dir.path(), &vars)),
            "http://[::1]:9100"
        );

        ConfigFile {
            server: Some("https://ahab.example.internal".into()),
            server_fingerprint: Some("SHA256:x".into()),
            ..ConfigFile::default()
        }
        .save(&dir.path().join("config/config.toml"))
        .unwrap();
        assert_eq!(
            local_server_url(&env_in(dir.path(), &[])),
            "https://ahab.example.internal"
        );
        let joined = load(&Flags::default(), &env_in(dir.path(), &[])).unwrap();
        assert_eq!(
            joined.settings.joined,
            Some(Joined {
                server: "https://ahab.example.internal".into(),
                fingerprint: "SHA256:x".into(),
            })
        );
    }

    #[test]
    fn rejects_an_unreadable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "daemon = \"sometimes\"").unwrap();

        let error = load(&Flags::default(), &env_in(dir.path(), &[])).unwrap_err();
        assert!(matches!(error, ConfigError::Unreadable { .. }), "{error}");
    }
}
