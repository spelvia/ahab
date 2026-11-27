//! Pairing from the command line: `ahab clients add`, `approve` and
//! `deny`, and pairing the command line itself with `ahab login` and
//! `ahab logout`.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use clap::Args;
use reqwest::Method;
use serde_json::json;

use crate::cli::client::{self, Client};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::clients::pairing::api::{
    CreatedPairingDto, CreatedPairingRequestDto, PairedDto, PairingAnswerDto, PairingRequestDto,
    PairingStateDto,
};

/// How long `ahab login` waits for approval at most: a request's life.
const WAIT_AT_MOST: Duration = Duration::from_mins(10);

/// `ahab login <server>`.
#[derive(Debug, Args)]
pub struct LoginArgs {
    /// The server, such as <https://ahab.example>. Or `ssh://you@host`:
    /// install Ahab there over SSH, start it as a server, and pair this
    /// command line with it, approved over SSH.
    pub server: String,
    /// A pairing code from Add client (on a paired client) instead of
    /// asking for approval.
    #[arg(long)]
    pub code: Option<String>,
    /// This command line's name on the server. Default: "ahab CLI on
    /// <this computer's name>".
    #[arg(long)]
    pub name: Option<String>,
    /// Do not open a browser: the server's approval page, or (over SSH)
    /// Ahab itself, paired.
    #[arg(long)]
    pub no_browser: bool,
    /// Over SSH: the address this computer and browsers reach the server at,
    /// saved there as `public_url`. Default: what it saved before, else
    /// `http://<host>:8180`.
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Over SSH: the address the server listens on there. Default: every
    /// address, on the port of a plain `http://` address; unchanged for
    /// `https://` (a reverse proxy).
    #[arg(long, value_name = "ADDRESS")]
    pub listen_addr: Option<std::net::SocketAddr>,
}

/// `ahab clients add`: a one-time link, its QR code and a code.
pub async fn add(client: &Client) -> Result<(), Exit> {
    let answer = client
        .send(Method::POST, "/api/v1/pairings", Some(json!({})))
        .await?;
    let created: CreatedPairingDto = client.parse(answer.clone())?;
    if client.json() {
        output::print(true, &answer, String::new);
        return Ok(());
    }
    let until = crate::shared::time::display(
        created.expires_at,
        time::macros::format_description!("[hour]:[minute] UTC"),
    );
    println!("Open this link on the new client to pair it (once, until {until}):\n");
    println!("  {}\n", created.url);
    show_qr(&created.url);
    if let Some(code) = &created.code {
        println!(
            "Or open {} there and type the code {code}.",
            created.url.split("/pair#").next().unwrap_or_default()
        );
    }
    Ok(())
}

/// A QR code of `text`, when standard output is a terminal.
fn show_qr(text: &str) {
    if !std::io::stdout().is_terminal() {
        return;
    }
    if let Ok(code) = qrcode::QrCode::new(text.as_bytes()) {
        let picture = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        println!("{picture}\n");
    }
}

/// `ahab clients approve <code>` and `deny <code>`.
pub async fn answer(client: &Client, code: &str, approve: bool) -> Result<(), Exit> {
    let path = format!(
        "/api/v1/pairing-requests/{}/{}",
        client::urlencode(code.trim()),
        if approve { "approve" } else { "deny" }
    );
    let answer = client.send(Method::POST, &path, Some(json!({}))).await?;
    let request: PairingRequestDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || {
        let from = request
            .address
            .as_deref()
            .map(|address| format!(" ({address})"))
            .unwrap_or_default();
        if approve {
            format!(
                "Approved {}{from}, code {}: it is paired now.",
                request.client_name, request.code
            )
        } else {
            format!(
                "Denied {}{from}, code {}.",
                request.client_name, request.code
            )
        }
    });
    Ok(())
}

fn this_computer() -> String {
    format!(
        "ahab CLI on {}",
        gethostname::gethostname().to_string_lossy()
    )
}

/// `ahab login <server>`: pairs this command line with the server, by
/// asking (a paired client approves) or with a code from Add client, and
/// keeps the credential in the data folder.
pub async fn login(args: LoginArgs, json: bool) -> Result<(), Exit> {
    if args.url.is_some() || args.listen_addr.is_some() {
        return Err(output::failure(
            json,
            Exit::Usage,
            "Only over SSH",
            "--url and --listen-addr are for `ahab login ssh://you@host`.",
        ));
    }
    let Some(server) = client::normalize_server(&args.server) else {
        return Err(output::failure(
            json,
            Exit::Usage,
            "Invalid address",
            &format!(
                "{:?} is not a server address such as https://ahab.example.",
                args.server
            ),
        ));
    };
    let anonymous = Client::new(&server, json)?;
    let name = args.name.clone().unwrap_or_else(this_computer);
    let (credential, record) = if let Some(code) = &args.code {
        let answer = anonymous
            .send(
                Method::POST,
                "/api/v1/pair",
                Some(json!({ "code": code, "name": name, "kind": "cli" })),
            )
            .await?;
        let paired: PairedDto = anonymous.parse(answer)?;
        (paired.credential, paired.client)
    } else {
        let answer = ask(&anonymous, &name, &args).await?;
        (answer.credential, answer.client.ok_or(Exit::Other)?)
    };
    let Some(credential) = credential else {
        return Err(output::failure(
            json,
            Exit::Other,
            "No credential",
            "The server paired this command line but sent no credential.",
        ));
    };
    client::remember_login(
        &server,
        &credential,
        Some(record.id.to_string()),
        Some(record.name.clone()),
    )
    .map_err(|error| output::failure(json, Exit::Other, "Cannot keep the login", &error))?;
    output::print(json, &json!({ "server": server, "client": record }), || {
        format!("Logged in to {server} as \"{}\".", record.name)
    });
    Ok(())
}

/// Asks to be paired and waits for the answer.
async fn ask(client: &Client, name: &str, args: &LoginArgs) -> Result<PairingAnswerDto, Exit> {
    let json = client.json();
    let answer = client
        .send(
            Method::POST,
            "/api/v1/pairing-requests",
            Some(json!({ "name": name, "kind": "cli" })),
        )
        .await?;
    let request: CreatedPairingRequestDto = client.parse(answer)?;
    let approve_page = format!(
        "{}/settings/clients?approve={}",
        client.base(),
        request.code
    );
    if json {
        eprintln!(
            "{}",
            json!({ "waiting": true, "code": request.code, "approve": approve_page })
        );
    } else {
        eprintln!(
            "To pair this command line, approve code {} on a client paired with {}:\n\n  \
             {approve_page}\n\nor run `ahab clients approve {}` where Ahab is paired.",
            request.code,
            client.base(),
            request.code
        );
        if !args.no_browser && can_open_browser() && webbrowser::open(&approve_page).is_ok() {
            eprintln!("Opened the approval page in your browser.");
        }
        eprintln!("Waiting for approval (Ctrl+C to stop)…");
    }
    let path = format!(
        "/api/v1/pairing-requests/{}?secret={}&wait=25",
        request.id,
        client::urlencode(&request.secret)
    );
    let started = Instant::now();
    loop {
        let answer = client.get(&path).await?;
        let answer: PairingAnswerDto = client.parse(answer)?;
        match answer.state {
            PairingStateDto::Used => return Ok(answer),
            PairingStateDto::Denied => {
                return Err(output::failure(
                    json,
                    Exit::NotAllowed,
                    "Denied",
                    "The request to pair this command line was denied.",
                ));
            }
            PairingStateDto::Expired => {
                return Err(output::failure(
                    json,
                    Exit::Temporary,
                    "Expired",
                    "Nobody approved this command line in time. Run `ahab login` again.",
                ));
            }
            PairingStateDto::Open | PairingStateDto::Approved => {}
        }
        if started.elapsed() > WAIT_AT_MOST {
            return Err(output::failure(
                json,
                Exit::Temporary,
                "Not approved in time",
                "Nobody approved this command line within 10 minutes. Run `ahab login` again.",
            ));
        }
    }
}

/// Whether a browser can be opened here: a terminal on a desktop.
fn can_open_browser() -> bool {
    std::io::stderr().is_terminal()
        && (cfg!(target_os = "macos")
            || std::env::var_os("DISPLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some())
}

/// `ahab logout [<server>]`: revokes this command line's client on the
/// server and forgets its credential. The local credential of the server on
/// this computer is never touched.
pub async fn logout(server: &str, json: bool) -> Result<(), Exit> {
    let server = client::normalize_server(server).unwrap_or_else(|| server.to_owned());
    let forgotten = client::forget_login(&server)
        .map_err(|error| output::failure(json, Exit::Other, "Cannot forget the login", &error))?;
    let Some((credential, id)) = forgotten else {
        return Err(output::failure(
            json,
            Exit::Other,
            "Not logged in",
            &format!(
                "This command line has no login for {server}. (On the server's own computer it \
                 uses the server's credential file, which `ahab logout` leaves alone.)"
            ),
        ));
    };
    let mut revoked = false;
    if let Some(id) = id {
        let client = Client::new(&server, json)?.with_credential(credential);
        revoked = client
            .try_send(Method::DELETE, &format!("/api/v1/clients/{id}"))
            .await
            .is_ok();
    }
    output::print(
        json,
        &json!({ "server": server, "revoked": revoked }),
        || {
            if revoked {
                format!("Logged out of {server}: this command line's client is revoked.")
            } else {
                format!(
                    "Forgot the login for {server}. The server could not be told; revoke the \
                     client in Settings → Clients if it is still listed."
                )
            }
        },
    );
    Ok(())
}
