//! `ahab setup`: join this computer to an existing server.
//! It asks to join with this machine's key, waits for approval (or is
//! approved at once with a one-time code), pins the server's key and saves
//! the server in `config.toml`, so `ahab up` runs only this computer's daemon.

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use clap::Args;
use reqwest::{Method, Url};
use serde_json::json;

use crate::cli::client::{Client, normalize_server};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::cli::service::{Backend, SystemRunner};
use crate::config::{self, ConfigFile, Paths};
use crate::daemon;
use crate::modules::machines::enrollment::api::{EnrollRequestDto, EnrollmentDto};
use crate::modules::machines::enrollment::model::enroll_message;
use crate::modules::machines::identity::store as identity;
use crate::modules::machines::machine::api::MachineStateDto;

/// How long `ahab setup` waits for approval: as long as a request lasts.
const WAIT_AT_MOST: Duration = Duration::from_mins(15);
const ASK_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Args)]
pub struct Setup {
    /// The server to join, such as <https://ahab.example.internal>.
    #[arg(long)]
    pub server: Option<String>,

    /// A one-time code from the server (`ahab machines add`, or "Add machine"
    /// in the web app): this computer is approved at once.
    #[arg(long)]
    pub code: Option<String>,

    /// Answer yes to every question: replacing an earlier join, and trusting
    /// a server reached without https.
    #[arg(long, short)]
    pub yes: bool,

    /// Only join; do not start the daemon (`ahab up` starts it later).
    #[arg(long)]
    pub no_start: bool,

    /// The server's key fingerprint (`SHA256:…`), known beforehand, as
    /// `ahab machines add you@host` knows it: joining fails if the server
    /// shows another, and is not asked about over plain http.
    #[arg(long, value_name = "FINGERPRINT")]
    pub server_fingerprint: Option<String>,
}

pub async fn run(setup: Setup, json: bool) -> Result<(), Exit> {
    let paths = Paths::resolve(&config::process_env)
        .map_err(|error| output::failure(json, Exit::Other, "Cannot set up", &error.to_string()))?;
    let mut file = ConfigFile::load(&paths.config_file())
        .map_err(|error| output::failure(json, Exit::Other, "Cannot set up", &error.to_string()))?;

    let server = match setup.server {
        Some(server) => server,
        None => prompt::input(
            json,
            "Server address (such as https://ahab.example.internal)",
            "--server",
        )?,
    };
    let server = normalize_server(&server).ok_or_else(|| {
        output::failure(
            json,
            Exit::Usage,
            "Invalid server address",
            &format!("{server} is not an http(s) address."),
        )
    })?;

    if let Some(previous) = file
        .server
        .as_deref()
        .filter(|previous| *previous != server)
    {
        let question = format!("This computer joined {previous}. Join {server} instead?");
        if !setup.yes && !prompt::confirm(json, &question, false, "--yes")? {
            return Err(output::failure(
                json,
                Exit::Other,
                "Not joined",
                "Nothing changed.",
            ));
        }
    }
    if file.server.is_none() && paths.database_file().exists() {
        say(
            json,
            "This computer ran its own Ahab server. After joining, `ahab up` runs only this \
             computer's daemon, connected to the server you join; its own data stays where it is.",
        );
    }

    let key = identity::load_or_create(&paths.data_dir.join("machine.key"))
        .map_err(|error| output::failure(json, Exit::Other, "Cannot set up", &error.to_string()))?;
    let url = Url::parse(&server)
        .map_err(|_| output::failure(json, Exit::Usage, "Invalid server address", &server))?;
    let host = daemon::host_of(&url).unwrap_or_default();
    let public_key = STANDARD.encode(key.public_key().as_bytes());
    let request = EnrollRequestDto {
        signature: STANDARD.encode(key.sign(&enroll_message(&host, &public_key))),
        public_key,
        hostname: daemon::local_facts().hostname,
        code: setup.code,
    };

    let client = Client::new(&server, json)?;
    let answer = client
        .send(
            Method::POST,
            "/api/v1/enrollments",
            Some(serde_json::to_value(&request).unwrap_or_default()),
        )
        .await?;
    let mut enrollment: EnrollmentDto = client.parse(answer)?;

    // A fingerprint known beforehand settles it; else https proves the
    // server's name, and over plain http, ask before trusting its key.
    if let Some(expected) = setup.server_fingerprint.as_deref() {
        if expected.trim() != enrollment.server_fingerprint {
            return Err(output::failure(
                json,
                Exit::NotAllowed,
                "Not the expected server",
                &format!(
                    "{server} shows the key fingerprint {}, not {}: this may not be the server \
                     you meant. Nothing was saved.",
                    enrollment.server_fingerprint,
                    expected.trim()
                ),
            ));
        }
    } else if url.scheme() != "https" && !setup.yes {
        let question = format!(
            "{server} is not reached over https. Its key fingerprint is {}. Trust it?",
            enrollment.server_fingerprint
        );
        if !prompt::confirm(json, &question, false, "--yes")? {
            return Err(output::failure(
                json,
                Exit::Other,
                "Not joined",
                "The server's key was not trusted.",
            ));
        }
    }

    if enrollment.state == MachineStateDto::Pending {
        show_approval(json, &server, &enrollment);
        enrollment = wait_for_approval(&client, &enrollment).await?;
    }

    file.server = Some(server.clone());
    file.server_fingerprint = Some(enrollment.server_fingerprint.clone());
    file.daemon = Some(true);
    file.save(&paths.config_file()).map_err(|error| {
        output::failure(
            json,
            Exit::Other,
            "Cannot save the settings",
            &error.to_string(),
        )
    })?;

    output::print(
        json,
        &json!({
            "server": server,
            "machine": {
                "id": enrollment.id,
                "name": enrollment.name,
                "fingerprint": key.public_key().fingerprint().to_string(),
            },
            "serverFingerprint": enrollment.server_fingerprint,
        }),
        || {
            format!(
                "Joined {server} as {}.\nServer key {} is pinned in {}.",
                enrollment.name,
                enrollment.server_fingerprint,
                paths.config_file().display(),
            )
        },
    );
    Ok(())
}

/// `ahab leave`: this computer stops being a machine of the server it
/// joined: its daemon-only service stops and the
/// server is forgotten in `config.toml`; the machine key stays, so joining
/// again is quick. The server keeps its record of the machine until it is
/// removed there.
pub fn leave(yes: bool, json: bool) -> ExitCode {
    match leave_inner(yes, json) {
        Ok(()) => ExitCode::SUCCESS,
        Err(exit) => exit.into(),
    }
}

fn leave_inner(yes: bool, json: bool) -> Result<(), Exit> {
    let fail = |error: &dyn std::fmt::Display| {
        output::failure(json, Exit::Other, "Cannot leave", &error.to_string())
    };
    let paths = Paths::resolve(&config::process_env).map_err(|error| fail(&error))?;
    let path = paths.config_file();
    let mut file = ConfigFile::load(&path).map_err(|error| fail(&error))?;
    let Some(server) = file.server.clone() else {
        return Err(output::failure(
            json,
            Exit::Rejected,
            "Not joined",
            "This computer is not a machine of another server: there is nothing to leave.",
        ));
    };
    let question = format!(
        "Leave {server}? This computer's daemon stops, and agents can no longer work here."
    );
    if !yes && !prompt::confirm(json, &question, false, "--yes")? {
        return Err(output::failure(
            json,
            Exit::Other,
            "Not left",
            "Nothing changed.",
        ));
    }
    let runner = SystemRunner;
    let backend = Backend::detect(&runner);
    let mut stopped = false;
    let mut managed = None;
    if let Some(backend) = &backend {
        if backend.is_managed() {
            managed = Some(backend.managed_stop_hint());
        } else {
            let state = backend.state(&runner);
            if state.installed || state.running {
                backend.stop(&runner).map_err(|error| fail(&error))?;
                stopped = state.running;
            }
        }
    }
    file.server = None;
    file.server_fingerprint = None;
    file.save(&path).map_err(|error| fail(&error))?;
    output::print(
        json,
        &json!({ "left": server, "stopped": stopped, "managed": managed.is_some() }),
        || {
            let mut text = format!(
                "This computer left {server}{}. Its key stays in {}, so `ahab setup` joins again \
                 quickly; remove the machine on the server (Settings → Machines) if it should go.",
                if stopped { "; its daemon stopped" } else { "" },
                paths.data_dir.display()
            );
            if let Some(hint) = managed {
                text.push_str(&format!(
                    "\nIts service is managed by your system configuration: stop it there (`{hint}`)."
                ));
            }
            text
        },
    );
    Ok(())
}

/// Text for the person at the terminal; nothing with `--json`.
fn say(json: bool, text: &str) {
    if !json {
        eprintln!("{text}");
    }
}

/// Where to approve this machine, as a link, a QR code on a terminal, and the
/// codes to compare.
fn show_approval(json: bool, server: &str, enrollment: &EnrollmentDto) {
    if json {
        return;
    }
    let link = format!("{server}/machines/{}", enrollment.id);
    eprintln!(
        "This computer asked to join as {}. Approve it on the server:\n",
        enrollment.name
    );
    eprintln!("  {link}\n");
    if std::io::stderr().is_terminal()
        && let Ok(code) = qrcode::QrCode::new(link.as_bytes())
    {
        let picture = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        eprintln!("{picture}\n");
    }
    eprintln!("Compare before approving:");
    eprintln!("  Check code   {}", enrollment.check_code);
    eprintln!("  Fingerprint  {}\n", enrollment.fingerprint);
    eprintln!(
        "Or approve it from the server's computer with `ahab machines approve {}`.",
        enrollment.name
    );
    eprintln!("Waiting for approval (Ctrl+C to stop; run `ahab setup` again to keep waiting)…");
}

async fn wait_for_approval(
    client: &Client,
    enrollment: &EnrollmentDto,
) -> Result<EnrollmentDto, Exit> {
    let path = format!("/api/v1/enrollments/{}", enrollment.id);
    let started = Instant::now();
    loop {
        tokio::time::sleep(ASK_EVERY).await;
        let answer = client.get(&path).await?;
        let current: EnrollmentDto = client.parse(answer)?;
        if current.state == MachineStateDto::Active {
            return Ok(current);
        }
        if started.elapsed() > WAIT_AT_MOST {
            return Err(output::failure(
                client.json(),
                Exit::Temporary,
                "Not approved in time",
                "Nobody approved this computer within 15 minutes. Run `ahab setup` again.",
            ));
        }
    }
}
