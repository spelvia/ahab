//! `ahab clients …`: list, pair, approve, rename and revoke clients, and
//! create tokens for scripts, through the API. A client is named by its
//! name, its ID or a unique start of its ID.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::json;
use time::OffsetDateTime;
use time::macros::format_description;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::modules::clients::client::api::{
    ClientDto, ClientKindDto, ClientListDto, CreatedTokenDto,
};
use crate::modules::clients::pairing::cli as pairing;
use crate::shared::time::display;

/// `ahab clients` alone lists them.
#[derive(Debug, Args)]
pub struct ClientsArgs {
    #[command(subcommand)]
    pub command: Option<ClientsCommand>,
}

#[derive(Debug, Subcommand)]
pub enum ClientsCommand {
    /// The clients paired with the server, oldest first.
    #[command(visible_alias = "ls")]
    List,
    /// Pair a new client: prints a one-time link, its QR code and a code
    /// to type instead. They work once, for 10 minutes.
    Add,
    /// Approve a client that asks to be paired, by the code it shows.
    Approve {
        /// The code the new client shows, such as 7F3K.
        code: String,
    },
    /// Deny a client that asks to be paired.
    Deny {
        /// The code the new client shows, such as 7F3K.
        code: String,
    },
    /// Rename a client.
    Rename {
        /// The client's name, ID or a unique start of its ID.
        client: String,
        /// Its new name.
        name: String,
    },
    /// Revoke a client: it can no longer reach Ahab, and its open
    /// connections close.
    #[command(visible_alias = "rm")]
    Revoke {
        /// The client's name, ID or a unique start of its ID.
        client: String,
        /// Do not ask first.
        #[arg(long, short)]
        yes: bool,
    },
    /// Create a token for a script or CI job; it is printed once.
    Token {
        /// What it is for, such as "CI deploys".
        name: String,
    },
}

pub async fn run(client: &Client, args: ClientsArgs) -> Result<(), Exit> {
    match args.command.unwrap_or(ClientsCommand::List) {
        ClientsCommand::List => {
            let (clients, answer) = list(client).await?;
            output::print(client.json(), &answer, || table(&clients.items));
            Ok(())
        }
        ClientsCommand::Add => pairing::add(client).await,
        ClientsCommand::Approve { code } => pairing::answer(client, &code, true).await,
        ClientsCommand::Deny { code } => pairing::answer(client, &code, false).await,
        ClientsCommand::Rename {
            client: reference,
            name,
        } => {
            let found = find(client, &reference).await?;
            let answer = client
                .send(
                    Method::PATCH,
                    &format!("/api/v1/clients/{}", found.id),
                    Some(json!({ "name": name })),
                )
                .await?;
            let renamed: ClientDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!("Renamed {} to {}.", found.name, renamed.name)
            });
            Ok(())
        }
        ClientsCommand::Revoke {
            client: reference,
            yes,
        } => {
            let found = find(client, &reference).await?;
            if !yes {
                let warning = match found.kind {
                    ClientKindDto::Local => {
                        " This computer gets a new credential at once; the old one stops working."
                    }
                    _ if found.current => " This is the client you use now: it logs out.",
                    _ => "",
                };
                let question = format!(
                    "Revoke {}? It can no longer reach Ahab.{warning}",
                    found.name
                );
                if !prompt::confirm(client.json(), &question, false, "--yes")? {
                    return Err(output::failure(
                        client.json(),
                        Exit::Other,
                        "Not revoked",
                        &format!("{} stays paired.", found.name),
                    ));
                }
            }
            client
                .send(
                    Method::DELETE,
                    &format!("/api/v1/clients/{}", found.id),
                    None,
                )
                .await?;
            output::print(client.json(), &json!({ "revoked": found.id }), || {
                format!("Revoked {}.", found.name)
            });
            Ok(())
        }
        ClientsCommand::Token { name } => {
            let answer = client
                .send(
                    Method::POST,
                    "/api/v1/clients/tokens",
                    Some(json!({ "name": name })),
                )
                .await?;
            let created: CreatedTokenDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "{}\n\nThis token is shown only now. Send it as `Authorization: Bearer \
                     <token>`, or set AHAB_TOKEN for the ahab command. Revoke it with \
                     `ahab clients revoke \"{}\"`.",
                    created.token, created.client.name
                )
            });
            Ok(())
        }
    }
}

async fn list(client: &Client) -> Result<(ClientListDto, serde_json::Value), Exit> {
    let answer = client.get("/api/v1/clients").await?;
    Ok((client.parse(answer.clone())?, answer))
}

/// The client `reference` names: its name (any case), its ID or a unique
/// start of its ID.
async fn find(client: &Client, reference: &str) -> Result<ClientDto, Exit> {
    let (list, _) = list(client).await?;
    resolve(list.items, reference).map_err(|detail| {
        output::error(
            client.json(),
            &json!({ "title": "Not found", "code": "client_not_found", "detail": detail }),
        );
        Exit::Other
    })
}

fn resolve(clients: Vec<ClientDto>, reference: &str) -> Result<ClientDto, String> {
    let wanted = reference.trim();
    if let Some(found) = clients
        .iter()
        .find(|found| found.id.to_string() == wanted || found.name.eq_ignore_ascii_case(wanted))
    {
        return Ok(found.clone());
    }
    let named: Vec<&ClientDto> = clients
        .iter()
        .filter(|found| wanted.len() >= 4 && found.id.to_string().starts_with(wanted))
        .collect();
    match named.as_slice() {
        [found] => Ok((*found).clone()),
        [] => Err(format!(
            "No client is named {wanted:?}. See them with `ahab clients`."
        )),
        _ => Err(format!(
            "Several clients start with {wanted:?}: give more of the ID."
        )),
    }
}

fn kind(kind: ClientKindDto) -> &'static str {
    match kind {
        ClientKindDto::Local => "this computer",
        ClientKindDto::Browser => "browser",
        ClientKindDto::Cli => "command line",
        ClientKindDto::App => "app",
        ClientKindDto::Token => "token",
    }
}

fn when(time: Option<OffsetDateTime>) -> String {
    time.map_or_else(
        || "never".to_owned(),
        |time| {
            display(
                time,
                format_description!("[year]-[month]-[day] [hour]:[minute] UTC"),
            )
        },
    )
}

fn table(clients: &[ClientDto]) -> String {
    if clients.is_empty() {
        return "No clients are paired.".to_owned();
    }
    let mut rows = vec![vec![
        "NAME".to_owned(),
        "KIND".to_owned(),
        "LAST USED".to_owned(),
        "FROM".to_owned(),
        "ID".to_owned(),
        "NOTES".to_owned(),
    ]];
    for found in clients {
        let mut notes = Vec::new();
        if found.current {
            notes.push("this client");
        }
        if !found.encrypted && found.kind != ClientKindDto::Local {
            notes.push("not encrypted");
        }
        if found.unused {
            notes.push("not used for 3 months");
        }
        rows.push(vec![
            found.name.clone(),
            kind(found.kind).to_owned(),
            when(found.last_used_at),
            found.last_address.clone().unwrap_or_else(|| "-".to_owned()),
            found.id.to_string(),
            notes.join(", "),
        ]);
    }
    output::table(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::clients::client::api::PairedViaDto;

    fn client(name: &str, id: &str) -> ClientDto {
        ClientDto {
            id: id.parse().unwrap(),
            name: name.to_owned(),
            kind: ClientKindDto::Browser,
            paired_via: PairedViaDto::Link,
            paired_by: None,
            user_agent: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            last_used_at: None,
            last_address: None,
            encrypted: false,
            current: false,
            unused: true,
        }
    }

    #[test]
    fn finds_clients_by_name_or_id() {
        let clients = vec![
            client("Firefox on Linux", "0196a1b2-0000-7000-8000-000000000001"),
            client("Safari on iPhone", "0196a1b2-0000-7000-8000-000000000002"),
        ];
        assert_eq!(
            resolve(clients.clone(), "safari on iphone").unwrap().name,
            "Safari on iPhone"
        );
        assert_eq!(
            resolve(clients.clone(), "0196a1b2-0000-7000-8000-000000000001")
                .unwrap()
                .name,
            "Firefox on Linux"
        );
        assert!(resolve(clients.clone(), "0196a1b2").is_err());
        assert!(resolve(clients, "Chrome").is_err());
    }

    #[test]
    fn lists_clients_with_their_notes() {
        let text = table(&[client(
            "Firefox on Linux",
            "0196a1b2-0000-7000-8000-000000000001",
        )]);
        assert!(text.contains("not encrypted, not used for 3 months"));
        assert!(text.contains("never"));
    }
}
