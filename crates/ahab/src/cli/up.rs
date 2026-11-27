//! `ahab up`: start Ahab on this computer: the server, and this computer's
//! daemon unless it is turned off; on a computer that joined another server,
//! only its daemon. In the background as a login service, or with
//! `--foreground` in this terminal (which the service itself runs).

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use std::time::Duration;

use serde_json::json;

use crate::cli::client::Client;
use crate::cli::command::runtime;
use crate::cli::credentials;
use crate::cli::exit::Exit;
use crate::cli::lifecycle;
use crate::cli::output;
use crate::cli::service::{Backend, Runner, ServiceSpec, SystemRunner};
use crate::config::{self, Flags, Joined, LogFormat, Settings};
use crate::daemon::{self, DaemonConfig};
use crate::modules::machines::identity::store as identity;
use crate::server::start;

#[derive(Debug, Args)]
pub struct Up {
    /// Stay in this terminal instead of running in the background. The login
    /// service, containers and `make dev` use it.
    #[arg(long)]
    pub foreground: bool,

    /// Also run this computer's daemon, so it is a machine too. Default: true.
    /// Saved in config.toml. Environment: `AHAB_DAEMON`.
    #[arg(
        long,
        value_name = "true|false",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub daemon: Option<bool>,

    /// Serve the web app; the API is always served. Default: true. Saved in
    /// config.toml. Environment: `AHAB_WEB`.
    #[arg(
        long,
        value_name = "true|false",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub web: Option<bool>,

    /// The address the server listens on. Default: 127.0.0.1:8180.
    /// Environment: `AHAB_LISTEN_ADDR`.
    #[arg(long, value_name = "ADDRESS")]
    pub listen_addr: Option<SocketAddr>,

    /// The database: a sqlite:// or postgres:// URL. Default: a SQLite file
    /// in the data folder. Environment: `DATABASE_URL`.
    #[arg(long, value_name = "URL")]
    pub database_url: Option<String>,

    /// A built web app to serve instead of the one in the binary
    /// (development). Environment: `AHAB_UI_DIR`.
    #[arg(long, value_name = "DIR")]
    pub ui_dir: Option<PathBuf>,

    /// The log format. Default: text. Environment: `AHAB_LOG_FORMAT`.
    #[arg(long, value_enum)]
    pub log_format: Option<LogFormat>,

    /// Saves `public_url` in config.toml first (`ahab login ssh://…` uses
    /// it on the computer it installs).
    #[arg(long, hide = true, value_name = "URL")]
    pub save_public_url: Option<String>,
    /// Saves `public_url` only when none is saved yet.
    #[arg(long, hide = true, value_name = "URL")]
    pub default_public_url: Option<String>,
    /// Saves `listen_addr` in config.toml first.
    #[arg(long, hide = true, value_name = "ADDRESS")]
    pub save_listen_addr: Option<SocketAddr>,
    /// Saves `listen_addr` only when none is saved yet.
    #[arg(long, hide = true, value_name = "ADDRESS")]
    pub default_listen_addr: Option<SocketAddr>,
}

impl Up {
    fn flags(&self) -> Flags {
        Flags {
            daemon: self.daemon,
            web: self.web,
            listen_addr: self.listen_addr,
            database_url: self.database_url.clone(),
            ui_dir: self.ui_dir.clone(),
            log_format: self.log_format,
        }
    }
}

/// Saves the settings `ahab login ssh://…` gives (`--save-…`,
/// `--default-…`); whether config.toml changed. A computer joined to
/// another server runs no server, so it refuses them.
fn save_settings(up: &Up, json: bool) -> Result<bool, Exit> {
    if up.save_public_url.is_none()
        && up.default_public_url.is_none()
        && up.save_listen_addr.is_none()
        && up.default_listen_addr.is_none()
    {
        return Ok(false);
    }
    let fail = |error: &dyn std::fmt::Display| {
        output::failure(
            json,
            Exit::Other,
            "Cannot save the settings",
            &error.to_string(),
        )
    };
    let paths = config::Paths::resolve(&config::process_env).map_err(|error| fail(&error))?;
    let path = paths.config_file();
    let mut file = config::ConfigFile::load(&path).map_err(|error| fail(&error))?;
    if let Some(server) = &file.server {
        return Err(output::failure(
            json,
            Exit::Rejected,
            "Joined to another server",
            &format!(
                "This computer is a machine of {server} and runs no server of its own. Remove \
                 `server` from {} to make it a server.",
                path.display()
            ),
        ));
    }
    let before = file.clone();
    if let Some(url) = &up.save_public_url {
        file.public_url = Some(url.clone());
    } else if file.public_url.is_none() {
        file.public_url.clone_from(&up.default_public_url);
    }
    if let Some(addr) = up.save_listen_addr {
        file.listen_addr = Some(addr);
    } else if file.listen_addr.is_none() {
        file.listen_addr = up.default_listen_addr;
    }
    if file == before {
        return Ok(false);
    }
    file.save(&path).map_err(|error| fail(&error))?;
    Ok(true)
}

pub fn run(up: Up, json: bool) -> ExitCode {
    let saved = match save_settings(&up, json) {
        Ok(saved) => saved,
        Err(exit) => return exit.into(),
    };
    let loaded = match config::load(&up.flags(), &config::process_env) {
        Ok(loaded) => loaded,
        Err(error) => {
            return output::failure(json, Exit::Usage, "Invalid settings", &error.to_string())
                .into();
        }
    };
    if let Some(path) = &loaded.saved_to
        && !json
    {
        eprintln!("Saved the daemon and web settings to {}.", path.display());
    }
    // Flags other than --daemon and --web apply to one start only: the
    // service would not see them.
    let one_off = up.listen_addr.is_some()
        || up.database_url.is_some()
        || up.ui_dir.is_some()
        || up.log_format.is_some();
    if up.foreground {
        return serve(&loaded.settings);
    }
    if one_off {
        return output::failure(
            json,
            Exit::Usage,
            "Not saved for the background",
            "--listen-addr, --database-url, --ui-dir and --log-format apply to one start only. \
             Put them in config.toml (or the environment) for the background service, or add \
             --foreground.",
        )
        .into();
    }
    match background(&loaded.settings, json, saved) {
        Ok(Some(code)) => code,
        Ok(None) => serve(&loaded.settings),
        Err(exit) => exit.into(),
    }
}

/// After `ahab setup`: starts this computer's daemon in the background, as
/// `ahab up` does (restarting what ran, which may have been a server of its
/// own or a daemon for another server), or says how to start it when there
/// is no service manager.
pub fn start_after_setup(json: bool) -> ExitCode {
    let settings = match config::load(&Flags::default(), &config::process_env) {
        Ok(loaded) => loaded.settings,
        Err(error) => {
            return output::failure(json, Exit::Usage, "Invalid settings", &error.to_string())
                .into();
        }
    };
    match background(&settings, json, true) {
        Ok(Some(code)) => code,
        Ok(None) => {
            if !json {
                eprintln!("Start this computer's daemon with `ahab up --foreground`.");
            }
            ExitCode::SUCCESS
        }
        Err(exit) => exit.into(),
    }
}

/// Runs Ahab as a login service: installs or updates the service file,
/// starts it (restarting it when it changed or runs an older Ahab), and waits
/// until it answers; `restart` restarts it anyway (its settings changed).
/// `None` when there is no service manager: then Ahab runs in this
/// terminal instead.
fn background(settings: &Settings, json: bool, restart: bool) -> Result<Option<ExitCode>, Exit> {
    let runner = SystemRunner;
    let Some(backend) = Backend::detect(&runner) else {
        if !json {
            eprintln!(
                "No service manager (a systemd user instance or launchd) is available here, \
                 so Ahab runs in this terminal. Stop it with Ctrl+C."
            );
        }
        return Ok(None);
    };
    if backend.is_managed() {
        let state = backend.state(&runner);
        output::print(
            json,
            &json!({ "managed": true, "running": state.running, "service": backend.path() }),
            || {
                format!(
                    "Ahab's service ({}) is managed by your system configuration, so `ahab up` \
                     leaves it alone. It is {}. Restart it with `{}`.",
                    backend.path().display(),
                    if state.running { "running" } else { "stopped" },
                    backend.managed_hint()
                )
            },
        );
        return Ok(Some(if state.running {
            ExitCode::SUCCESS
        } else {
            Exit::Other.into()
        }));
    }
    if settings.daemon {
        refuse_root().map_err(|error| output::failure(json, Exit::Usage, "Not as root", &error))?;
    }
    let fail = |title: &str, error: &dyn std::fmt::Display| {
        output::failure(json, Exit::Other, title, &error.to_string())
    };
    let exe =
        std::env::current_exe().map_err(|error| fail("Cannot find the ahab binary", &error))?;
    let env = ["AHAB_CONFIG_DIR", "AHAB_DATA_DIR"]
        .iter()
        .filter_map(|name| {
            config::process_env(name).map(|value| {
                // Relative folders would be relative to the service's folder.
                let absolute =
                    std::path::absolute(&value).map_or(value, |path| path.display().to_string());
                ((*name).to_owned(), absolute)
            })
        })
        .collect();
    let spec = ServiceSpec {
        exe,
        env,
        description: format!("Ahab: {}", lifecycle::mode(settings)),
    };
    let first = !backend.path().exists();
    let changed = backend
        .install(&runner, &spec)
        .map_err(|error| fail("Cannot write the service", &error))?;
    let was = backend.state(&runner);
    let runtime = runtime(false).map_err(|error| fail("Cannot start", &error))?;
    let outdated = was.running
        && settings.joined.is_none()
        && runtime
            .block_on(running_version(settings))
            .is_some_and(|version| version != env!("CARGO_PKG_VERSION"));
    backend
        .start(&runner, was.running && (changed || outdated || restart))
        .map_err(|error| fail("Cannot start the service", &error))?;
    let answering = runtime.block_on(wait_until_up(settings, &backend, &runner));

    let url = lifecycle::local_url(settings);
    let path = backend.path();
    output::print(
        json,
        &json!({
            "service": path, "manager": backend.name(), "running": answering,
            "url": settings.joined.is_none().then_some(&url),
            "publicUrl": settings.joined.is_none().then_some(settings.public_url.as_ref()).flatten(),
        }),
        || {
            let mut text = match &settings.joined {
                Some(joined) => format!(
                    "This computer's daemon runs in the background, connected to {}.",
                    joined.server
                ),
                None if settings.web => format!("Ahab runs in the background at {url}."),
                None => format!("Ahab runs in the background; its API is at {url}/api/v1."),
            };
            if !answering {
                text = format!(
                    "Ahab's service started but does not answer yet: see `ahab logs`.\n{text}"
                );
            }
            text.push_str(&format!(
                "\nIt starts again when you log in ({}: {}).\n\
                 Stop it with `ahab down`; see it with `ahab status` and `ahab logs`.",
                backend.name(),
                path.display()
            ));
            if let Some(hint) = linger_hint(&runner, &backend) {
                text.push_str(&format!("\n\n{hint}"));
            }
            text
        },
    );
    if answering && settings.web && settings.joined.is_none() && !json {
        runtime.block_on(welcome(&url, first));
    }
    Ok(Some(if answering {
        ExitCode::SUCCESS
    } else {
        Exit::Temporary.into()
    }))
}

/// After Ahab started in the background: while no client but this computer
/// is paired, opens a paired browser (on a desktop) or prints the first-run
/// link. Once a client is paired, opens the browser, paired, only
/// the first time Ahab is set up.
async fn welcome(url: &str, first: bool) {
    let Some((credential, _)) = credentials::find(url, &config::process_env) else {
        return;
    };
    let Ok(client) = Client::new(url, true).map(|client| client.with_credential(credential)) else {
        return;
    };
    let paired = client
        .quiet_get("/api/v1/clients")
        .await
        .and_then(|clients| clients["items"].as_array().cloned())
        .is_some_and(|items| items.iter().any(|client| client["kind"] != "local"));
    if lifecycle::can_open_browser() {
        if (first || !paired) && webbrowser::open(&client.browser_link("/").await).is_ok() {
            eprintln!("Opened Ahab in your browser; it is paired with this server.");
        }
        return;
    }
    if paired {
        return;
    }
    let Some(created) = client
        .send(
            reqwest::Method::POST,
            "/api/v1/pairings",
            Some(json!({ "firstRun": true })),
        )
        .await
        .ok()
    else {
        return;
    };
    let Some(link) = created["url"].as_str() else {
        return;
    };
    eprintln!(
        "\nNo client is paired yet. Open this link to pair your first one (once, within 24 \
         hours):\n\n  {link}\n"
    );
    if std::io::IsTerminal::is_terminal(&std::io::stderr())
        && let Ok(code) = qrcode::QrCode::new(link.as_bytes())
    {
        let picture = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        eprintln!("{picture}\n");
    }
    eprintln!(
        "Or open {} in a browser and approve the code it shows with `ahab clients approve \
         <code>` on this computer.",
        link.split("/pair#").next().unwrap_or(url)
    );
}

/// The version the running server reports, if it answers.
async fn running_version(settings: &Settings) -> Option<String> {
    let client = Client::new(&lifecycle::local_url(settings), true).ok()?;
    let health = client.quiet_get("/api/v1/health").await?;
    health["version"].as_str().map(str::to_owned)
}

/// Waits up to 20 seconds until the server answers (or, on a computer that
/// joined another server, until the service runs).
async fn wait_until_up(settings: &Settings, backend: &Backend, runner: &dyn Runner) -> bool {
    for _ in 0..40 {
        let up = if settings.joined.is_some() {
            backend.state(runner).running
        } else {
            running_version(settings).await.is_some()
        };
        if up {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// On Linux, a user service stops at logout unless lingering is on: worth
/// saying on a computer without a desktop session.
fn linger_hint(runner: &dyn Runner, backend: &Backend) -> Option<String> {
    if !matches!(backend, Backend::Systemd(_)) {
        return None;
    }
    let user = config::process_env("USER")?;
    let linger = runner
        .run(
            "loginctl",
            &["show-user", &user, "--property=Linger", "--value"],
        )
        .ok()?;
    let desktop = config::process_env("XDG_SESSION_TYPE")
        .is_some_and(|session| matches!(session.as_str(), "x11" | "wayland"));
    (linger.success && linger.stdout.trim() == "no" && !desktop).then(|| {
        format!(
            "This login has no desktop session: Ahab stops when you log out unless lingering \
             is on. Turn it on with `loginctl enable-linger {user}` (it may ask for an \
             administrator)."
        )
    })
}

/// Runs the server, and this computer's daemon when it is on, in this
/// process until it is stopped.
fn serve(settings: &Settings) -> ExitCode {
    crate::server::telemetry::init(settings.log_format);
    tracing::info!(
        config = %settings.paths.config_file().display(),
        data = %settings.paths.data_dir.display(),
        daemon = settings.daemon,
        web = settings.web,
        "starting"
    );
    let ran = runtime(true).and_then(|runtime| runtime.block_on(run_all(settings)));
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "Ahab stopped");
            ExitCode::FAILURE
        }
    }
}

async fn run_all(settings: &Settings) -> Result<(), String> {
    if let Some(joined) = &settings.joined {
        return run_joined(settings, joined).await;
    }
    let started = start::start(settings)
        .await
        .map_err(|error| error.to_string())?;
    let (stop, stopped) = watch::channel(false);
    let daemon = if settings.daemon {
        Some(local_daemon(settings, &started, stopped.clone()).await?)
    } else {
        None
    };
    tokio::spawn(async move {
        start::shutdown_signal().await;
        let _ = stop.send(true);
    });
    let mut server_stopped = stopped;
    started
        .run(async move {
            let _ = server_stopped.changed().await;
        })
        .await
        .map_err(|error| error.to_string())?;
    if let Some(daemon) = daemon
        && tokio::time::timeout(Duration::from_secs(5), daemon)
            .await
            .is_err()
    {
        tracing::info!("this computer's daemon did not stop in time; stopping anyway");
    }
    Ok(())
}

/// A computer that joined another server runs only its daemon, connected
/// there with the server key it pinned when it joined.
async fn run_joined(settings: &Settings, joined: &Joined) -> Result<(), String> {
    if !settings.daemon {
        return Err(format!(
            "This computer joined {} and runs only its daemon; with --daemon=false there is \
             nothing to run.",
            joined.server
        ));
    }
    refuse_root()?;
    let key = identity::load_or_create(&settings.paths.data_dir.join("machine.key"))
        .map_err(|error| error.to_string())?;
    tracing::info!(server = %joined.server, "starting this computer's daemon");
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(async move {
        start::shutdown_signal().await;
        let _ = stop.send(true);
    });
    let config = DaemonConfig {
        server: joined.server.clone(),
        key,
        facts: daemon::local_facts(),
        server_fingerprint: Some(joined.fingerprint.clone()),
        data_dir: settings.paths.data_dir.clone(),
        executable: None,
        plugin_node: settings.plugin_node.clone(),
        tools: crate::modules::tools::registry::daemon::ToolOptions::default(),
    };
    daemon::run(config, stopped)
        .await
        .map_err(|error| error.to_string())
}

fn refuse_root() -> Result<(), String> {
    if rustix::process::geteuid().is_root() {
        return Err(
            "The daemon does not run as root, because agents would run as root too. \
             Run Ahab as your own user, or turn the daemon off with --daemon=false."
                .to_owned(),
        );
    }
    Ok(())
}

/// This computer's daemon, inside the server's process: it registers the
/// server's own host as the local machine and connects to the server over
/// loopback, exactly as a remote daemon would.
async fn local_daemon(
    settings: &Settings,
    started: &start::Started,
    stop: watch::Receiver<bool>,
) -> Result<JoinHandle<()>, String> {
    refuse_root()?;
    let key = identity::load_or_create(&settings.paths.data_dir.join("machine.key"))
        .map_err(|error| error.to_string())?;
    let facts = daemon::local_facts();
    let machine = started
        .state
        .machines
        .ensure_local(key.public_key(), &facts.hostname)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(machine = %machine.name, fingerprint = %machine.public_key.fingerprint(), "this computer is a machine");
    let mut addr = started.addr;
    if addr.ip().is_unspecified() {
        addr.set_ip(if addr.is_ipv4() {
            Ipv4Addr::LOCALHOST.into()
        } else {
            Ipv6Addr::LOCALHOST.into()
        });
    }
    let config = DaemonConfig {
        server: format!("http://{addr}"),
        key,
        facts,
        // Its own server, over loopback: nothing to pin.
        server_fingerprint: None,
        data_dir: settings.paths.data_dir.clone(),
        executable: None,
        plugin_node: settings.plugin_node.clone(),
        tools: crate::modules::tools::registry::daemon::ToolOptions::default(),
    };
    Ok(tokio::spawn(async move {
        if let Err(error) = daemon::run(config, stop).await {
            tracing::error!(%error, "the daemon stopped");
        }
    }))
}
