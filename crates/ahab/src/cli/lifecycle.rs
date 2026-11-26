//! `ahab down`, `ahab status` and `ahab logs`: Ahab in the background on this
//! computer. `ahab up` is in `up.rs`.

use std::io::IsTerminal;
use std::process::{Command, ExitCode};

use serde_json::{Value, json};

use crate::cli::client::Client;
use crate::cli::command::runtime;
use crate::cli::credentials;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::service::{Backend, Runner, ServiceState, SystemRunner};
use crate::config::{self, Flags, Settings};
use crate::modules::machines::identity::store as identity;
use crate::modules::machines::machine::api::{MachineListDto, MachineStateDto};

/// What this computer runs, in words.
pub fn mode(settings: &Settings) -> String {
    match (&settings.joined, settings.daemon) {
        (Some(joined), _) => format!("this computer's daemon, joined to {}", joined.server),
        (None, true) => "the server and this computer's daemon".to_owned(),
        (None, false) => "the server only".to_owned(),
    }
}

/// The address of the server this computer runs, from itself.
pub fn local_url(settings: &Settings) -> String {
    let mut addr = settings.listen_addr;
    if addr.ip().is_unspecified() {
        addr.set_ip(if addr.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    format!("http://{addr}")
}

fn settings(json: bool) -> Result<Settings, Exit> {
    config::load(&Flags::default(), &config::process_env)
        .map(|loaded| loaded.settings)
        .map_err(|error| output::failure(json, Exit::Usage, "Invalid settings", &error.to_string()))
}

fn backend(json: bool, runner: &dyn Runner) -> Result<Backend, Exit> {
    Backend::detect(runner).ok_or_else(|| {
        output::failure(
            json,
            Exit::Other,
            "No service manager",
            "This computer has no systemd user instance or launchd for Ahab to use, so Ahab \
             runs only in a terminal with `ahab up --foreground` (stop it with Ctrl+C).",
        )
    })
}

pub fn down(remove: bool, json: bool) -> ExitCode {
    let runner = SystemRunner;
    let done = (|| {
        let backend = backend(json, &runner)?;
        if backend.is_managed() {
            return Err(output::failure(
                json,
                Exit::Other,
                "Managed elsewhere",
                &format!(
                    "Ahab's service ({}) was not written by `ahab up`, so `ahab down` leaves it \
                     alone: stop it through your system configuration, or with `{}`.",
                    backend.path().display(),
                    backend.managed_stop_hint()
                ),
            ));
        }
        let path = backend.path();
        let was = backend.state(&runner);
        if was.installed || was.running {
            backend.stop(&runner).map_err(|error| {
                output::failure(json, Exit::Other, "Cannot stop Ahab", &error.to_string())
            })?;
        }
        let removed = remove
            && backend.remove(&runner).map_err(|error| {
                output::failure(
                    json,
                    Exit::Other,
                    "Cannot remove the service",
                    &error.to_string(),
                )
            })?;
        output::print(
            json,
            &json!({ "stopped": was.running, "removed": removed, "service": path }),
            || {
                let mut text = if was.running {
                    "Stopped Ahab; it no longer starts when you log in.".to_owned()
                } else {
                    "Ahab was not running in the background.".to_owned()
                };
                if removed {
                    text.push_str(&format!("\nRemoved {}.", path.display()));
                } else if path.exists() {
                    text.push_str(&format!(
                        "\nThe service file stays ({}); `ahab down --remove` deletes it.",
                        path.display()
                    ));
                }
                text
            },
        );
        Ok(())
    })();
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(exit) => exit.into(),
    }
}

pub fn logs(follow: bool, lines: u32, json: bool) -> ExitCode {
    let runner = SystemRunner;
    let backend = match backend(json, &runner) {
        Ok(backend) => backend,
        Err(exit) => return exit.into(),
    };
    let (program, args) = backend.logs_command(follow, lines);
    match Command::new(&program).args(&args).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => Exit::Other.into(),
        Err(error) => output::failure(
            json,
            Exit::Other,
            "Cannot show the logs",
            &format!("{program}: {error}"),
        )
        .into(),
    }
}

pub fn status(json: bool) -> ExitCode {
    let settings = match settings(json) {
        Ok(settings) => settings,
        Err(exit) => return exit.into(),
    };
    let runner = SystemRunner;
    let backend = Backend::detect(&runner);
    let service = backend.as_ref().map(|backend| {
        (
            backend.name(),
            backend.path(),
            backend.is_managed(),
            backend.state(&runner),
        )
    });
    let checked = runtime(false)
        .map_err(|error| error.clone())
        .map(|runtime| runtime.block_on(check(&settings)));
    let (server, machine) =
        checked.unwrap_or_else(|error| (json!({ "error": error }), Value::Null));

    let running = service
        .as_ref()
        .is_some_and(|(_, _, _, state)| state.running);
    let answer = json!({
        "mode": mode(&settings),
        // For the desktop app: what this computer is, without
        // reading `mode`'s words.
        "role": if settings.joined.is_some() { "machine" } else { "server" },
        "joined": settings.joined.as_ref().map(|joined| &joined.server),
        "service": service.as_ref().map(|(name, path, managed, state)| json!({
            "manager": name, "file": path, "managed": managed,
            "installed": state.installed, "enabled": state.enabled,
            "running": state.running, "since": state.since,
        })),
        "server": server,
        "machine": machine,
    });
    output::print(json, &answer, || {
        let mut rows: Vec<(&str, String)> = vec![("Runs", mode(&settings))];
        rows.push(("Service", describe_service(service.as_ref())));
        if let Some(url) = server["url"].as_str() {
            let health = match server["version"].as_str() {
                Some(version) => format!("{url}, answering (Ahab {version})"),
                None => format!("{url}, not answering"),
            };
            rows.push(("Server", health));
        }
        if let Some(state) = machine["state"].as_str() {
            rows.push((
                "Machine",
                format!("{} ({state})", output::text(&machine["name"])),
            ));
        } else if !machine.is_null() {
            rows.push(("Machine", output::text(&machine["note"])));
        }
        rows.iter()
            .map(|(label, value)| format!("{label:<8} {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    if running || service.is_none() && !server["version"].is_null() {
        ExitCode::SUCCESS
    } else {
        Exit::Other.into()
    }
}

fn describe_service(service: Option<&(&str, std::path::PathBuf, bool, ServiceState)>) -> String {
    let Some((name, path, managed, state)) = service else {
        return "none (no service manager here)".to_owned();
    };
    if !state.installed && !path.exists() {
        return "not installed: `ahab up` starts Ahab in the background".to_owned();
    }
    let mut text = if state.running { "running" } else { "stopped" }.to_owned();
    if let Some(since) = &state.since {
        text.push_str(&format!(" since {since}"));
    }
    text.push_str(if state.enabled {
        ", starts at login"
    } else {
        ", not at login"
    });
    text.push_str(&format!(" ({name}: {}", path.display()));
    if *managed {
        text.push_str(", managed by your system configuration");
    }
    text.push(')');
    text
}

/// The server's health and this machine's state, as far as they answer.
async fn check(settings: &Settings) -> (Value, Value) {
    let url = settings
        .joined
        .as_ref()
        .map_or_else(|| local_url(settings), |joined| joined.server.clone());
    let quiet = true;
    let Ok(client) = Client::new(&url, quiet) else {
        return (json!({ "url": url }), Value::Null);
    };
    let health = client.quiet_get("/api/v1/health").await;
    let server = json!({ "url": url, "version": health.as_ref().and_then(|health| health["version"].as_str()) });
    if !settings.daemon && settings.joined.is_none() {
        return (server, Value::Null);
    }
    let key_file = settings.paths.data_dir.join("machine.key");
    if !key_file.exists() {
        return (server, json!({ "note": "no machine key yet" }));
    }
    let Ok(key) = identity::load_or_create(&key_file) else {
        return (server, json!({ "note": "the machine key cannot be read" }));
    };
    let fingerprint = key.public_key().fingerprint().to_string();
    // The machine list needs this command line's credential; health did not.
    let Some((credential, _)) = credentials::find(&url, &config::process_env) else {
        return (
            server,
            json!({ "note": "this command line is not logged in: `ahab login`" }),
        );
    };
    let client = client.with_credential(credential);
    let machine = match client.quiet_get("/api/v1/machines").await {
        Some(answer) => serde_json::from_value::<MachineListDto>(answer)
            .ok()
            .and_then(|list| {
                list.items
                    .into_iter()
                    .find(|machine| machine.fingerprint == fingerprint)
            })
            .map_or_else(
                || json!({ "note": "not known to the server: join it with `ahab setup`" }),
                |machine| {
                    let state = match (machine.state, machine.online) {
                        (MachineStateDto::Pending, _) => "waits for approval",
                        (MachineStateDto::Active, true) => "online",
                        (MachineStateDto::Active, false) => "offline",
                    };
                    json!({ "name": machine.name, "id": machine.id, "state": state })
                },
            ),
        None => json!({ "note": "the server does not answer" }),
    };
    (server, machine)
}

/// Whether to open the browser: a person at a desktop.
pub fn can_open_browser() -> bool {
    std::io::stdout().is_terminal()
        && (cfg!(target_os = "macos")
            || std::env::var_os("DISPLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some())
}
