//! Ahab on another computer over SSH.
//!
//! `ahab machines add you@host` installs Ahab there, joins it to this
//! server and starts its daemon. `ahab login ssh://you@host` installs and
//! starts a server there and pairs this command line with it. SSH is the
//! proof: the join code, the server's fingerprint and the pairing approval
//! travel over it, so nobody types a code or compares a fingerprint.
//!
//! Until releases are published, Ahab is installed by copying this
//! computer's own `ahab` to `~/.local/bin/ahab` there, when the operating
//! system and architecture match. Running either command again repairs and
//! updates instead of adding anything twice.

use std::io::IsTerminal;
use std::path::Path;
use std::time::{Duration, Instant};

use reqwest::Method;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::cli::client::{self, Client};
use crate::cli::credentials::{self, Source};
use crate::cli::exit::Exit;
use crate::cli::lifecycle;
use crate::cli::output;
use crate::cli::prompt;
use crate::config;
use crate::integrations::ssh::{Destination, Ran, Ssh, SshError, quote};
use crate::modules::clients::pairing::api::{
    CreatedPairingDto, CreatedPairingRequestDto, PairingAnswerDto, PairingStateDto,
};
use crate::modules::clients::pairing::cli::LoginArgs;
use crate::modules::machines::enrollment::api::CreatedJoinCodeDto;
use crate::modules::machines::machine::api::{MachineDto, MachineStateDto};

/// Where Ahab is installed on the other computer, from its home folder.
const REMOTE_BIN: &str = ".local/bin/ahab";
/// The port a server installed over SSH listens on unless told otherwise.
const DEFAULT_PORT: u16 = 8180;
/// How long to wait for a machine to come online after it started.
const ONLINE_WITHIN: Duration = Duration::from_secs(30);

/// `ahab machines add <destination>`.
#[derive(Debug, Clone)]
pub struct AddMachine {
    pub destination: String,
    pub name: Option<String>,
    pub server_url: Option<String>,
    pub yes: bool,
    pub no_start: bool,
}

/// Progress for the person at the terminal; nothing with `--json`.
fn say(json: bool, text: &str) {
    if !json {
        eprintln!("{text}");
    }
}

/// A failure of this command, with a code for scripts.
fn fail(json: bool, exit: Exit, code: &str, title: &str, detail: &str) -> Exit {
    output::error(
        json,
        &json!({ "title": title, "detail": detail, "code": code }),
    );
    exit
}

fn ssh_failed(json: bool, error: &SshError) -> Exit {
    let detail = error.to_string();
    match error {
        SshError::Missing(..) => fail(json, Exit::Other, "ssh_missing", "No SSH", &detail),
        SshError::NeedsInteraction(_) => fail(
            json,
            Exit::NotAllowed,
            "ssh_needs_interaction",
            "SSH needs interaction",
            &detail,
        ),
        SshError::Unreachable(_) => fail(
            json,
            Exit::Temporary,
            "ssh_unreachable",
            "The host cannot be reached",
            &detail,
        ),
        SshError::Failed(_) => fail(json, Exit::Other, "ssh_failed", "SSH failed", &detail),
    }
}

/// What the other computer is and has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Host {
    /// As Rust names them: `linux`, `macos`; `x86_64`, `aarch64`.
    os: String,
    arch: String,
    /// `~/.local/bin/ahab` there, if any.
    installed: Option<Installed>,
    /// `systemd`, `launchd` or `none`.
    service: String,
    /// `ahab status --json` there.
    status: Option<Value>,
}

impl Host {
    fn platform(&self) -> String {
        format!("{} {}", self.os, self.arch)
    }

    /// The server it is a machine of, if it joined one.
    fn joined_to(&self) -> Option<String> {
        let mode = self.status.as_ref()?["mode"].as_str()?;
        mode.strip_prefix("this computer's daemon, joined to ")
            .map(str::to_owned)
    }

    /// Whether it runs (or ran) a server of its own.
    fn runs_server(&self) -> bool {
        let Some(status) = &self.status else {
            return false;
        };
        self.joined_to().is_none()
            && (status["server"]["version"].is_string() || status["service"]["installed"] == true)
    }

    fn running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status["service"]["running"] == true)
    }
}

/// The `ahab` on the other computer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Installed {
    /// What `ahab --version` says, such as `ahab 0.1.0`.
    version: String,
    size: Option<u64>,
    sha256: Option<String>,
}

/// `uname -s` and `uname -m` as Rust names them.
fn platform_of(os: &str, arch: &str) -> (String, String) {
    let os = match os.trim() {
        "Linux" => "linux".to_owned(),
        "Darwin" => "macos".to_owned(),
        other => other.to_ascii_lowercase(),
    };
    let arch = match arch.trim() {
        "x86_64" | "amd64" => "x86_64".to_owned(),
        "aarch64" | "arm64" => "aarch64".to_owned(),
        other => other.to_owned(),
    };
    (os, arch)
}

const PROBE: &str = r#"printf 'os=%s\n' "$(uname -s)"
printf 'arch=%s\n' "$(uname -m)"
a="$HOME/.local/bin/ahab"
if [ -x "$a" ]; then
  printf 'version=%s\n' "$("$a" --version 2>/dev/null)"
  printf 'size=%s\n' "$(wc -c < "$a" | tr -d ' ')"
  printf 'sha256=%s\n' "$( (sha256sum "$a" 2>/dev/null || shasum -a 256 "$a" 2>/dev/null) | cut -d' ' -f1)"
  printf 'status=%s\n' "$("$a" status --json 2>/dev/null | tr -d '\n')"
fi
if [ "$(uname -s)" = Darwin ]; then echo service=launchd
elif systemctl --user show-environment >/dev/null 2>&1; then echo service=systemd
else echo service=none; fi"#;

fn read_probe(stdout: &str) -> Host {
    let value = |key: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let (os, arch) = platform_of(value("os").unwrap_or("?"), value("arch").unwrap_or("?"));
    Host {
        os,
        arch,
        installed: value("version").map(|version| Installed {
            version: version.to_owned(),
            size: value("size").and_then(|size| size.parse().ok()),
            sha256: value("sha256").map(str::to_ascii_lowercase),
        }),
        service: value("service").unwrap_or("none").to_owned(),
        status: value("status").and_then(|text| serde_json::from_str(text).ok()),
    }
}

fn probe(ssh: &Ssh, json: bool) -> Result<Host, Exit> {
    say(
        json,
        &format!("Connecting to {} over SSH…", ssh.destination()),
    );
    let ran = ssh.run(PROBE).map_err(|error| ssh_failed(json, &error))?;
    if !ran.success {
        return Err(fail(
            json,
            Exit::Other,
            "ssh_failed",
            "Cannot look around",
            &format!(
                "{} did not answer as expected: {}",
                ssh.destination(),
                ran.stderr.trim()
            ),
        ));
    }
    let host = read_probe(&ran.stdout);
    let has = host
        .installed
        .as_ref()
        .map(|installed| format!(", with {}", installed.version))
        .unwrap_or_default();
    say(
        json,
        &format!("{} runs {}{has}.", ssh.destination(), host.platform()),
    );
    Ok(host)
}

/// How Ahab got onto the other computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Install {
    /// It had this very `ahab` already.
    Current,
    /// This computer's `ahab` was copied there.
    Copied,
    /// It has the same version, built for its platform.
    Existing,
}

impl Install {
    fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Copied => "copied",
            Self::Existing => "existing",
        }
    }
}

fn sha256_of(path: &Path) -> Result<String, std::io::Error> {
    let bytes = std::fs::read(path)?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            hex.push_str(&format!("{byte:02x}"));
            hex
        }))
}

fn this_version() -> String {
    format!("ahab {}", env!("CARGO_PKG_VERSION"))
}

/// Puts this computer's `ahab` on the other one, unless it has it.
fn install(ssh: &Ssh, host: &Host, json: bool) -> Result<Install, Exit> {
    let destination = ssh.destination();
    let local = |error: &dyn std::fmt::Display| {
        fail(
            json,
            Exit::Other,
            "install_failed",
            "Cannot read this computer's ahab",
            &error.to_string(),
        )
    };
    let exe = std::env::current_exe().map_err(|error| local(&error))?;
    // Hashing a large binary takes a moment: only when the sizes match.
    let size = std::fs::metadata(&exe)
        .map_err(|error| local(&error))?
        .len();
    let same_file = match &host.installed {
        Some(Installed {
            size: Some(theirs_size),
            sha256: Some(theirs),
            ..
        }) if *theirs_size == size => *theirs == sha256_of(&exe).map_err(|error| local(&error))?,
        _ => false,
    };
    let same_platform = host.os == std::env::consts::OS && host.arch == std::env::consts::ARCH;
    match &host.installed {
        _ if same_file => {
            say(
                json,
                &format!("{destination} has this computer's ahab already."),
            );
            return Ok(Install::Current);
        }
        _ if same_platform => {}
        Some(installed) if installed.version == this_version() => {
            say(
                json,
                &format!(
                    "{destination} has {} for its platform; using it.",
                    installed.version
                ),
            );
            return Ok(Install::Existing);
        }
        _ => {
            return Err(fail(
                json,
                Exit::Other,
                "platform_mismatch",
                "Another platform",
                &format!(
                    "{destination} runs {}, and this computer's ahab is built for {} {}. There \
                     are no published releases to download yet, so install {} for {} there \
                     yourself (build it on that computer, or for it, and put it in \
                     ~/.local/bin/ahab), then run this again: it uses an ahab of the same \
                     version that it finds there.",
                    host.platform(),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    this_version(),
                    host.platform(),
                ),
            ));
        }
    }

    if host.installed.is_some() && host.running() {
        // The new binary runs from the next start: stop the old one first.
        say(
            json,
            &format!("Stopping Ahab on {destination} to update it…"),
        );
        let stop = format!("\"$HOME/{REMOTE_BIN}\" down --json >/dev/null 2>&1; true");
        ssh.run(&stop).map_err(|error| ssh_failed(json, &error))?;
    }
    say(
        json,
        &format!(
            "Copying {} to {destination}:~/{REMOTE_BIN}…",
            this_version()
        ),
    );
    let ran = ssh
        .run("mkdir -p \"$HOME/.local/bin\"")
        .map_err(|error| ssh_failed(json, &error))?;
    if !ran.success {
        return Err(fail(
            json,
            Exit::Other,
            "install_failed",
            "Cannot install",
            &format!(
                "Cannot create ~/.local/bin on {destination}: {}",
                ran.stderr.trim()
            ),
        ));
    }
    ssh.copy(&exe, &format!("{REMOTE_BIN}.new"))
        .map_err(|error| ssh_failed(json, &error))?;
    let replace = format!(
        "chmod u+rx \"$HOME/{REMOTE_BIN}.new\" && mv -f \"$HOME/{REMOTE_BIN}.new\" \
         \"$HOME/{REMOTE_BIN}\" && \"$HOME/{REMOTE_BIN}\" --version"
    );
    let ran = ssh
        .run(&replace)
        .map_err(|error| ssh_failed(json, &error))?;
    if !ran.success {
        return Err(fail(
            json,
            Exit::Other,
            "install_failed",
            "The copied ahab does not run there",
            &format!(
                "{destination} cannot run this computer's ahab: {}. It may need libraries \
                 that computer lacks (a binary built by Nix, say); build one for it there and \
                 put it in ~/.local/bin/ahab, then run this again.",
                ran.stderr.trim()
            ),
        ));
    }
    Ok(Install::Copied)
}

/// Runs `ahab <args> --json` on the other computer: what it printed on
/// standard output, as JSON values.
fn ahab(ssh: &Ssh, args: &[&str], json: bool) -> Result<(Ran, Vec<Value>), Exit> {
    let words: Vec<String> = args.iter().map(|arg| quote(arg)).collect();
    let script = format!("\"$HOME/{REMOTE_BIN}\" {} --json", words.join(" "));
    let ran = ssh.run(&script).map_err(|error| ssh_failed(json, &error))?;
    let values = serde_json::Deserializer::from_str(&ran.stdout)
        .into_iter::<Value>()
        .map_while(Result::ok)
        .collect();
    Ok((ran, values))
}

/// The failure of `ahab <step>` on the other computer, passed on.
fn remote_failed(json: bool, ssh: &Ssh, step: &str, ran: &Ran) -> Exit {
    let problem = ran
        .stderr
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter(Value::is_object);
    let detail = problem
        .as_ref()
        .and_then(|problem| problem["detail"].as_str().or(problem["title"].as_str()))
        .map_or_else(|| ran.stderr.trim().to_owned(), str::to_owned);
    let code = problem
        .as_ref()
        .and_then(|problem| problem["code"].as_str())
        .unwrap_or("remote_failed")
        .to_owned();
    let exit = match ran.code {
        Some(2) => Exit::Usage,
        Some(3) => Exit::Rejected,
        Some(4) => Exit::NotAllowed,
        Some(5) => Exit::Temporary,
        _ => Exit::Other,
    };
    fail(
        json,
        exit,
        &code,
        &format!("`ahab {step}` failed on {}", ssh.destination()),
        &detail,
    )
}

/// On Linux, a user service stops when its user logs out unless lingering
/// is on: turns it on, which a user may usually do for themselves. `on`,
/// `enabled`, or `failed`.
fn linger(ssh: &Ssh) -> String {
    let script = r#"u=$(id -un)
if [ "$(loginctl show-user "$u" --property=Linger --value 2>/dev/null)" = yes ]; then echo on
elif loginctl enable-linger "$u" >/dev/null 2>&1; then echo enabled
else echo failed; fi"#;
    ssh.run(script)
        .map_or_else(|_| "failed".to_owned(), |ran| ran.stdout.trim().to_owned())
}

fn linger_note(json: bool, ssh: &Ssh, linger: &str) {
    match linger {
        "enabled" => say(
            json,
            &format!(
                "Turned on lingering on {}, so Ahab keeps running after you log out there.",
                ssh.destination()
            ),
        ),
        "failed" => say(
            json,
            &format!(
                "Warning: Ahab on {0} stops when you log out there, unless lingering is on. \
                 Turn it on there with `sudo loginctl enable-linger $USER`.",
                ssh.destination()
            ),
        ),
        _ => {}
    }
}

fn is_loopback_url(url: &str) -> bool {
    let Some(host) = reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// `ahab machines add <destination>`: installs Ahab there over SSH, joins it
/// to the server `client` talks to (with a join code this command creates,
/// and the server's fingerprint it knows), starts its daemon and waits until
/// it is online.
pub async fn add_machine(client: &Client, add: AddMachine) -> Result<(), Exit> {
    let json = client.json();
    let Some(destination) = Destination::parse(&add.destination) else {
        return Err(fail(
            json,
            Exit::Usage,
            "destination_invalid",
            "Not an SSH destination",
            &format!(
                "{:?} is not an SSH destination such as you@build-box or \
                 ssh://you@build-box:2222.",
                add.destination
            ),
        ));
    };
    let explicit_url = match &add.server_url {
        Some(url) => Some(client::normalize_server(url).ok_or_else(|| {
            fail(
                json,
                Exit::Usage,
                "server_url_invalid",
                "Invalid server address",
                &format!("{url} is not an http(s) address."),
            )
        })?),
        None => None,
    };
    let ssh = Ssh::new(destination);

    let body = add
        .name
        .as_ref()
        .map_or_else(|| json!({}), |name| json!({ "name": name }));
    let answer = client
        .send(Method::POST, "/api/v1/join-codes", Some(body))
        .await?;
    let code: CreatedJoinCodeDto = client.parse(answer)?;
    let done = join(client, &ssh, &add, explicit_url, &code).await;
    // Used or not (a machine that had joined already), the code goes.
    let _ = client
        .try_send(
            Method::DELETE,
            &format!("/api/v1/join-codes/{}", code.join_code.id),
        )
        .await;
    done
}

async fn join(
    client: &Client,
    ssh: &Ssh,
    add: &AddMachine,
    explicit_url: Option<String>,
    code: &CreatedJoinCodeDto,
) -> Result<(), Exit> {
    let json = client.json();
    let destination = ssh.destination().clone();
    let server = if let Some(url) = explicit_url {
        url
    } else {
        let url = code
            .server_url
            .clone()
            .unwrap_or_else(|| client.base().to_owned());
        if is_loopback_url(&url) {
            return Err(fail(
                json,
                Exit::Usage,
                "server_url_needed",
                "Which address reaches this server?",
                &format!(
                    "This server answers at {url}, on this computer only, which \
                     {destination} cannot reach. Give the address {destination} reaches it \
                     at with --server-url (such as http://192.168.1.4:8180, or a tailnet \
                     name), or set public_url in the server's config.toml.",
                ),
            ));
        }
        url
    };

    let host = probe(ssh, json)?;
    let mut yes = add.yes;
    if let Some(previous) = host.joined_to().filter(|previous| *previous != server)
        && !yes
    {
        let question = format!("{destination} is a machine of {previous}. Move it to {server}?");
        yes = prompt::confirm(json, &question, false, "--yes")?;
        if !yes {
            return Err(fail(
                json,
                Exit::Other,
                "not_joined",
                "Not joined",
                "Nothing changed.",
            ));
        }
    } else if host.runs_server() && !yes {
        let question = format!(
            "{destination} runs its own Ahab server. Joining makes it a machine of {server}; \
             its own data stays there but is no longer served. Continue?"
        );
        yes = prompt::confirm(json, &question, false, "--yes")?;
        if !yes {
            return Err(fail(
                json,
                Exit::Other,
                "not_joined",
                "Not joined",
                "Nothing changed.",
            ));
        }
    }
    let installed = install(ssh, &host, json)?;

    say(json, &format!("Joining {destination} to {server}…"));
    let mut args = vec!["setup", "--server", &server, "--code", &code.code];
    if let Some(fingerprint) = &code.server_fingerprint {
        args.extend(["--server-fingerprint", fingerprint]);
    }
    if yes {
        args.push("--yes");
    }
    if add.no_start {
        args.push("--no-start");
    }
    let (ran, values) = ahab(ssh, &args, json)?;
    if !ran.success {
        return Err(remote_failed(json, ssh, "setup", &ran));
    }
    let joined = values.first().cloned().unwrap_or_default();
    let Some(id) = joined["machine"]["id"]
        .as_str()
        .and_then(|id| id.parse::<Uuid>().ok())
    else {
        return Err(fail(
            json,
            Exit::Other,
            "remote_failed",
            "Unexpected answer",
            &format!(
                "`ahab setup` on {destination} did not say which machine it is: {}",
                ran.stdout.trim()
            ),
        ));
    };

    // The server's record must be the key that sits on the host.
    let path = format!("/api/v1/machines/{id}");
    let mut machine: MachineDto = client.parse(client.get(&path).await?)?;
    if joined["machine"]["fingerprint"].as_str() != Some(machine.fingerprint.as_str())
        || machine.state != MachineStateDto::Active
    {
        return Err(fail(
            json,
            Exit::NotAllowed,
            "fingerprint_mismatch",
            "Not the machine that joined",
            &format!(
                "The server's machine {} has the key {}, but {destination} reports {}. Remove \
                 it with `ahab machines rm {}` and run this again.",
                machine.name,
                machine.fingerprint,
                output::text(&joined["machine"]["fingerprint"]),
                machine.name
            ),
        ));
    }
    if let Some(name) = add.name.as_deref().filter(|name| *name != machine.name) {
        let answer = client
            .send(Method::PATCH, &path, Some(json!({ "name": name })))
            .await?;
        machine = client.parse(answer)?;
    }
    say(
        json,
        &format!(
            "{destination} joined as {} (fingerprint {}, pinned server key {}).",
            machine.name,
            machine.fingerprint,
            output::text(&joined["serverFingerprint"])
        ),
    );

    let service = values.get(1);
    let started = !add.no_start && service.is_some_and(|service| service["running"] == true);
    let linger = (started && service.is_some_and(|service| service["manager"] == "systemd"))
        .then(|| linger(ssh));
    if let Some(linger) = &linger {
        linger_note(json, ssh, linger);
    }
    let online = if started {
        say(
            json,
            &format!("Waiting for {} to come online…", machine.name),
        );
        wait_online(client, &path).await
    } else {
        machine.online
    };
    if let Ok(answer) = client.get(&path).await
        && let Ok(fresh) = client.parse::<MachineDto>(answer)
    {
        machine = fresh;
    }

    let result = json!({
        "host": destination.to_string(),
        "platform": host.platform(),
        "install": installed.as_str(),
        "server": server,
        "machine": machine,
        "started": started,
        "online": online,
        "linger": linger,
    });
    if add.no_start {
        output::print(json, &result, || {
            format!(
                "{destination} is installed and joined as {}. Start it there with `ahab up`.",
                machine.name
            )
        });
        return Ok(());
    }
    if !started {
        output::print(json, &result, String::new);
        return Err(fail(
            json,
            Exit::Other,
            "no_service_manager",
            "Joined, but not started",
            &format!(
                "{destination} joined as {}, but has no service manager Ahab can use (a \
                 systemd user instance or launchd), so its daemon is not running. Start it \
                 there with `ahab up --foreground` (in tmux or screen), or as a service of \
                 your own.",
                machine.name
            ),
        ));
    }
    if !online {
        output::print(json, &result, String::new);
        return Err(fail(
            json,
            Exit::Temporary,
            "machine_not_online",
            "Started, but not online",
            &format!(
                "Ahab started on {destination}, but {} did not connect within 30 seconds. See \
                 `ssh {destination} .local/bin/ahab logs`; it keeps trying.",
                machine.name
            ),
        ));
    }
    output::print(json, &result, || {
        format!(
            "{} is online: {destination} is a machine of {server}.",
            machine.name
        )
    });
    Ok(())
}

async fn wait_online(client: &Client, path: &str) -> bool {
    let started = Instant::now();
    while started.elapsed() < ONLINE_WITHIN {
        if let Some(machine) = client.quiet_get(path).await
            && machine["online"] == true
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// The address a server installed over SSH gets unless `--url` says: the
/// host's name after `~/.ssh/config`, on the default port.
fn default_url(ssh: &Ssh, port: u16) -> String {
    let host = ssh
        .resolved_host()
        .unwrap_or_else(|| ssh.destination().host().to_owned());
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    format!("http://{host}:{port}")
}

/// `ahab login ssh://you@host`: installs Ahab on the host, starts it there
/// as a server (with its daemon, as `ahab up` does), pairs this command line
/// with it (approved over SSH) and offers a link that pairs a browser.
pub async fn login(args: &LoginArgs, json: bool) -> Result<(), Exit> {
    let Some(destination) = Destination::parse(&args.server) else {
        return Err(fail(
            json,
            Exit::Usage,
            "destination_invalid",
            "Not an SSH destination",
            &format!(
                "{:?} is not an SSH destination such as ssh://you@build-box.",
                args.server
            ),
        ));
    };
    if args.code.is_some() {
        return Err(fail(
            json,
            Exit::Usage,
            "usage",
            "No code over SSH",
            "--code pairs with a code from Add client; over SSH, the approval comes over SSH.",
        ));
    }
    let explicit_url = match &args.url {
        Some(url) => Some(client::normalize_server(url).ok_or_else(|| {
            fail(
                json,
                Exit::Usage,
                "server_url_invalid",
                "Invalid server address",
                &format!("{url} is not an http(s) address."),
            )
        })?),
        None => None,
    };
    let ssh = Ssh::new(destination.clone());
    let host = probe(&ssh, json)?;
    if let Some(server) = host.joined_to() {
        return Err(fail(
            json,
            Exit::Rejected,
            "joined_elsewhere",
            "A machine of another server",
            &format!(
                "{destination} is a machine of {server}, not a server. Log in to {server} \
                 instead, or remove `server` from its config.toml to make it a server."
            ),
        ));
    }
    if host.service == "none" {
        return Err(fail(
            json,
            Exit::Other,
            "no_service_manager",
            "No service manager",
            &format!(
                "{destination} has no service manager Ahab can use (a systemd user instance or \
                 launchd), so Ahab cannot run there in the background. Start it there yourself \
                 (`ahab up --foreground` in tmux, or a service of your own), then run `ahab \
                 login <its address>`."
            ),
        ));
    }
    let installed = install(&ssh, &host, json)?;

    // Where the server answers: --url and --listen-addr as given; else what
    // it saved before, else the host's name on port 8180, on every address.
    let port = args.listen_addr.map_or(DEFAULT_PORT, |addr| addr.port());
    let url = explicit_url
        .clone()
        .unwrap_or_else(|| default_url(&ssh, port));
    let listen = args.listen_addr.map(|addr| addr.to_string()).or_else(|| {
        // Plain http is reached directly; behind https a proxy is likely.
        let parsed = reqwest::Url::parse(&url).ok()?;
        (parsed.scheme() == "http").then(|| {
            format!(
                "0.0.0.0:{}",
                parsed.port_or_known_default().unwrap_or(DEFAULT_PORT)
            )
        })
    });
    let mut up = vec!["up".to_owned()];
    let save = if explicit_url.is_some() {
        "--save-public-url"
    } else {
        "--default-public-url"
    };
    up.extend([save.to_owned(), url.clone()]);
    if let Some(listen) = listen {
        let save = if args.listen_addr.is_some() || explicit_url.is_some() {
            "--save-listen-addr"
        } else {
            "--default-listen-addr"
        };
        up.extend([save.to_owned(), listen]);
    }
    say(json, &format!("Starting Ahab's server on {destination}…"));
    let words: Vec<&str> = up.iter().map(String::as_str).collect();
    let (ran, values) = ahab(&ssh, &words, json)?;
    if !ran.success {
        return Err(remote_failed(json, &ssh, "up", &ran));
    }
    let service = values.last().cloned().unwrap_or_default();
    let url = service["publicUrl"].as_str().map_or(url, str::to_owned);
    if service["running"] != true {
        return Err(fail(
            json,
            Exit::Temporary,
            "server_not_running",
            "The server does not answer",
            &format!(
                "Ahab started on {destination} but does not answer yet. See `ssh \
                 {destination} .local/bin/ahab logs`, then run this again."
            ),
        ));
    }
    if service["manager"] == "systemd" {
        linger_note(json, &ssh, &linger(&ssh));
    }

    say(json, &format!("Pairing this command line with {url}…"));
    let anonymous = Client::new(&url, json)?;
    if anonymous.quiet_get("/api/v1/health").await.is_none() {
        return Err(fail(
            json,
            Exit::Temporary,
            "server_unreachable",
            "The server cannot be reached from here",
            &format!(
                "Ahab runs on {destination}, but this computer cannot reach it at {url}. Run \
                 this again with --url and the address that reaches it (a tailnet name, or a \
                 reverse proxy's https address)."
            ),
        ));
    }
    let paired = match logged_in(&url).await {
        Some(paired) => {
            say(
                json,
                &format!("This command line is logged in to {url} already."),
            );
            paired
        }
        None => pair(&ssh, &anonymous, &url, args, json).await?,
    };

    // A browser: a one-time link, made on the server's own computer.
    let (ran, values) = ahab(&ssh, &["clients", "add"], json)?;
    let link = (ran.success)
        .then(|| values.first().cloned())
        .flatten()
        .and_then(|value| serde_json::from_value::<CreatedPairingDto>(value).ok())
        .map(|created| created.url);
    let opened = !json
        && !args.no_browser
        && lifecycle::can_open_browser()
        && link
            .as_deref()
            .is_some_and(|link| webbrowser::open(link).is_ok());

    output::print(
        json,
        &json!({
            "host": destination.to_string(),
            "platform": host.platform(),
            "install": installed.as_str(),
            "server": url,
            "client": paired,
            "pairingLink": link,
            "openedBrowser": opened,
        }),
        || {
            let mut text = format!(
                "Ahab runs on {destination} at {url}, and this command line is logged in: \
                 use `--server {url}` (or AHAB_SERVER) to reach it."
            );
            match (&link, opened) {
                (_, true) => text.push_str("\nOpened it in your browser, paired."),
                (Some(link), false) => {
                    text.push_str(&format!(
                        "\n\nTo pair a browser, open this link (once, within 10 minutes):\n\n  \
                         {link}"
                    ));
                    if std::io::stdout().is_terminal()
                        && let Ok(code) = qrcode::QrCode::new(link.as_bytes())
                    {
                        let picture = code
                            .render::<qrcode::render::unicode::Dense1x2>()
                            .quiet_zone(true)
                            .build();
                        text.push_str(&format!("\n\n{picture}"));
                    }
                }
                (None, false) => {}
            }
            text
        },
    );
    Ok(())
}

/// This command line's client on `url`, when its login there still works.
async fn logged_in(url: &str) -> Option<Value> {
    let (credential, source) = credentials::find(url, &config::process_env)?;
    if !matches!(source, Source::Login) {
        return None;
    }
    let client = Client::new(url, true).ok()?.with_credential(credential);
    client.quiet_get("/api/v1/clients/current").await
}

/// Pairs this command line with the server at `url`: it asks, and the
/// server's own computer approves over SSH. The client, as JSON.
async fn pair(
    ssh: &Ssh,
    anonymous: &Client,
    url: &str,
    args: &LoginArgs,
    json: bool,
) -> Result<Value, Exit> {
    let name = args.name.clone().unwrap_or_else(|| {
        format!(
            "ahab CLI on {}",
            gethostname::gethostname().to_string_lossy()
        )
    });
    let answer = anonymous
        .send(
            Method::POST,
            "/api/v1/pairing-requests",
            Some(json!({ "name": name, "kind": "cli" })),
        )
        .await?;
    let request: CreatedPairingRequestDto = anonymous.parse(answer)?;
    let (ran, _) = ahab(ssh, &["clients", "approve", &request.code], json)?;
    if !ran.success {
        return Err(remote_failed(json, ssh, "clients approve", &ran));
    }
    let path = format!(
        "/api/v1/pairing-requests/{}?secret={}&wait=25",
        request.id,
        client::urlencode(&request.secret)
    );
    let mut paired = None;
    for _ in 0..5 {
        let answer: PairingAnswerDto = anonymous.parse(anonymous.get(&path).await?)?;
        match answer.state {
            PairingStateDto::Used => {
                paired = Some(answer);
                break;
            }
            PairingStateDto::Open | PairingStateDto::Approved => {}
            PairingStateDto::Denied | PairingStateDto::Expired => break,
        }
    }
    let Some((credential, record)) =
        paired.and_then(|answer| Some((answer.credential?, answer.client?)))
    else {
        return Err(fail(
            json,
            Exit::Other,
            "not_paired",
            "Not paired",
            "The server approved this command line but sent no credential. Run this again.",
        ));
    };
    client::remember_login(
        url,
        &credential,
        Some(record.id.to_string()),
        Some(record.name.clone()),
    )
    .map_err(|error| {
        fail(
            json,
            Exit::Other,
            "login_not_kept",
            "Cannot keep the login",
            &error,
        )
    })?;
    say(
        json,
        &format!("This command line is paired as \"{}\".", record.name),
    );
    Ok(serde_json::to_value(&record).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_host_said() {
        let host = read_probe(
            "os=Linux\narch=x86_64\nversion=ahab 0.1.0\nsha256=ABC\nstatus={\"mode\":\"this \
             computer's daemon, joined to http://a:8180\"}\nservice=systemd\n",
        );
        assert_eq!(host.platform(), "linux x86_64");
        assert_eq!(
            host.installed,
            Some(Installed {
                version: "ahab 0.1.0".into(),
                size: None,
                sha256: Some("abc".into()),
            })
        );
        assert_eq!(host.service, "systemd");
        assert_eq!(host.joined_to().as_deref(), Some("http://a:8180"));
        assert!(!host.runs_server());

        let fresh = read_probe("os=Darwin\narch=arm64\nservice=launchd\n");
        assert_eq!(fresh.platform(), "macos aarch64");
        assert_eq!(fresh.installed, None);
        assert_eq!(fresh.joined_to(), None);

        let server = read_probe(
            "os=Linux\narch=aarch64\nversion=ahab 0.1.0\nstatus={\"mode\":\"the server and \
             this computer's daemon\",\"server\":{\"version\":\"0.1.0\"},\"service\":\
             {\"installed\":true,\"running\":true}}\nservice=systemd\n",
        );
        assert!(server.runs_server());
        assert!(server.running());
    }

    #[test]
    fn knows_addresses_only_this_computer_reaches() {
        assert!(is_loopback_url("http://127.0.0.1:8180"));
        assert!(is_loopback_url("http://localhost:8180"));
        assert!(is_loopback_url("http://[::1]:8180"));
        assert!(!is_loopback_url("http://192.168.1.4:8180"));
        assert!(!is_loopback_url("https://ahab.example"));
    }
}
