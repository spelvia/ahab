//! `ahab machines …`: list, show, edit and remove machines through the API,
//! in the shape of `gh repo …` and `docker container …`: the machine is a
//! positional argument (name, ID or unique ID prefix), changes are `edit`
//! with one flag per field, and removing asks first unless `--yes`.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::format_description;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::modules::machines::enrollment::api::CreatedJoinCodeDto;
use crate::modules::machines::machine::api::{MachineDto, MachineListDto, MachineStateDto};
use crate::shared::time::display;

/// `ahab machines` alone lists them.
#[derive(Debug, Args)]
pub struct MachinesArgs {
    #[command(subcommand)]
    pub command: Option<MachinesCommand>,
}

#[derive(Debug, Subcommand)]
pub enum MachinesCommand {
    /// The machines: the server's own first, then by name.
    #[command(visible_alias = "ls")]
    List,
    /// Everything about one machine.
    Show {
        /// The machine's name, ID or a unique start of its ID.
        machine: String,
    },
    /// Change a machine.
    Edit {
        /// The machine's name, ID or a unique start of its ID.
        machine: String,
        /// A new name: lowercase letters, digits and hyphens.
        #[arg(long)]
        name: Option<String>,
    },
    /// Add a computer: over SSH (`ahab machines add you@host`), install Ahab
    /// there, join it and start it; without a destination, create a one-time
    /// code to join by hand with `ahab setup`.
    Add {
        /// Where to install Ahab over SSH, such as you@build-box or
        /// `ssh://you@build-box:2222`; `~/.ssh/config` applies.
        destination: Option<String>,
        /// The name the machine gets; else its hostname.
        #[arg(long)]
        name: Option<String>,
        /// Over SSH: the address the machine reaches this server at.
        /// Default: the server's `public_url`, else the address it answers
        /// on.
        #[arg(long, value_name = "URL", requires = "destination")]
        server_url: Option<String>,
        /// Over SSH: answer yes to every question (the computer runs its own
        /// server, or is a machine of another).
        #[arg(long, short, requires = "destination")]
        yes: bool,
        /// Over SSH: install and join, but do not start Ahab there.
        #[arg(long, requires = "destination")]
        no_start: bool,
    },
    /// Approve a machine that waits to join, after comparing its check code.
    Approve {
        /// The machine's name, ID or a unique start of its ID.
        machine: String,
    },
    /// The coding agents a machine can run: installed, version, signed in.
    Agents {
        /// The machine's name, ID or a unique start of its ID.
        machine: String,
        /// Detect them again now instead of showing the last report.
        #[arg(long)]
        refresh: bool,
    },
    /// Remove a machine; it must join again to come back.
    #[command(visible_alias = "rm")]
    Remove {
        /// The machine's name, ID or a unique start of its ID.
        machine: String,
        /// Do not ask first.
        #[arg(long, short)]
        yes: bool,
    },
}

pub async fn run(client: &Client, args: MachinesArgs) -> Result<(), Exit> {
    match args.command.unwrap_or(MachinesCommand::List) {
        MachinesCommand::List => {
            let (list, answer) = list(client).await?;
            output::print(client.json(), &answer, || table(&list.items));
            Ok(())
        }
        MachinesCommand::Show { machine } => {
            let machine = find(client, &machine).await?;
            let path = format!("/api/v1/machines/{}", machine.id);
            let answer = client.get(&path).await?;
            let machine: MachineDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || details(&machine));
            Ok(())
        }
        MachinesCommand::Edit { machine, name } => {
            let Some(name) = name else {
                return Err(output::failure(
                    client.json(),
                    Exit::Usage,
                    "Nothing to change",
                    "Say what to change, such as --name build-box.",
                ));
            };
            let machine = find(client, &machine).await?;
            let path = format!("/api/v1/machines/{}", machine.id);
            let answer = client
                .send(Method::PATCH, &path, Some(json!({ "name": name })))
                .await?;
            let edited: MachineDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                if edited.name == machine.name {
                    format!("{} is unchanged.", edited.name)
                } else {
                    format!("Renamed {} to {}.", machine.name, edited.name)
                }
            });
            Ok(())
        }
        MachinesCommand::Add {
            destination: Some(_),
            ..
        } => Err(output::failure(
            client.json(),
            Exit::Other,
            "Not here",
            "Adding a machine over SSH runs from the command tree.",
        )),
        MachinesCommand::Add {
            destination: None,
            name,
            ..
        } => {
            let body = name.map_or_else(|| json!({}), |name| json!({ "name": name }));
            let answer = client
                .send(Method::POST, "/api/v1/join-codes", Some(body))
                .await?;
            let created: CreatedJoinCodeDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "One-time code {} (works once, until {}).\n\
                     On the computer to join, run:\n\n  ahab setup --server {} --code {}",
                    created.code,
                    display(
                        created.join_code.expires_at,
                        format_description!("[hour]:[minute] UTC")
                    ),
                    client.base(),
                    created.code,
                )
            });
            Ok(())
        }
        MachinesCommand::Approve { machine } => {
            let machine = find(client, &machine).await?;
            let path = format!("/api/v1/machines/{}/approve", machine.id);
            let answer = client.send(Method::POST, &path, Some(json!({}))).await?;
            let approved: MachineDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "Approved {} (check code {}). It connects within a few seconds.",
                    approved.name, approved.check_code
                )
            });
            Ok(())
        }
        MachinesCommand::Agents { machine, refresh } => {
            let machine = find(client, &machine).await?;
            crate::modules::machines::agents::cli::run(client, machine.id, &machine.name, refresh)
                .await
        }
        MachinesCommand::Remove { machine, yes } => {
            let machine = find(client, &machine).await?;
            if !yes {
                let question = format!("Remove {}? It must join again to come back.", machine.name);
                if !prompt::confirm(client.json(), &question, false, "--yes")? {
                    return Err(output::failure(
                        client.json(),
                        Exit::Other,
                        "Not removed",
                        &format!("{} stays.", machine.name),
                    ));
                }
            }
            let path = format!("/api/v1/machines/{}", machine.id);
            client.send(Method::DELETE, &path, None).await?;
            output::print(client.json(), &json!({ "removed": machine.id }), || {
                format!("Removed {}.", machine.name)
            });
            Ok(())
        }
    }
}

async fn list(client: &Client) -> Result<(MachineListDto, Value), Exit> {
    let answer = client.get("/api/v1/machines").await?;
    Ok((client.parse(answer.clone())?, answer))
}

/// The machine `reference` names: its name, its ID or a unique start of its
/// ID, as docker accepts.
async fn find(client: &Client, reference: &str) -> Result<MachineDto, Exit> {
    let (list, _) = list(client).await?;
    resolve(list.items, reference).map_err(|(code, detail)| {
        output::error(
            client.json(),
            &json!({ "title": "Not found", "code": code, "detail": detail }),
        );
        Exit::Other
    })
}

/// The machine `reference` names, or an error code and message.
fn resolve(
    machines: Vec<MachineDto>,
    reference: &str,
) -> Result<MachineDto, (&'static str, String)> {
    if let Some(exact) = machines
        .iter()
        .find(|machine| machine.name == reference || machine.id.to_string() == reference)
    {
        return Ok(exact.clone());
    }
    let reference = reference.to_ascii_lowercase();
    let mut by_prefix = machines
        .into_iter()
        .filter(|machine| reference.len() >= 4 && machine.id.to_string().starts_with(&reference));
    match (by_prefix.next(), by_prefix.next()) {
        (Some(machine), None) => Ok(machine),
        (Some(_), Some(_)) => Err((
            "machine_ambiguous",
            format!("More than one machine's ID starts with {reference}: give more of it."),
        )),
        (None, _) => Err((
            "machine_not_found",
            format!("There is no machine called {reference}."),
        )),
    }
}

/// One machine, a field per line.
fn details(machine: &MachineDto) -> String {
    let dash = |value: &Option<String>| value.clone().unwrap_or_else(|| "-".to_owned());
    let time = |value: Option<OffsetDateTime>| {
        value.map_or_else(
            || "never".to_owned(),
            |time| {
                display(
                    time,
                    format_description!("[year]-[month]-[day] [hour]:[minute] UTC"),
                )
            },
        )
    };
    let rows = [
        ("Name", machine.name.clone()),
        ("ID", machine.id.to_string()),
        ("State", status(machine).to_owned()),
        (
            "This server",
            if machine.is_local { "yes" } else { "no" }.to_owned(),
        ),
        (
            "Platform",
            format!("{} {}", dash(&machine.os), dash(&machine.arch)),
        ),
        ("Version", dash(&machine.version)),
        ("Check code", machine.check_code.clone()),
        ("Fingerprint", machine.fingerprint.clone()),
        ("Joined", time(Some(machine.created_at))),
        ("Approved", time(machine.approved_at)),
        ("Last seen", time(machine.last_seen_at)),
    ];
    rows.iter()
        .map(|(label, value)| format!("{label:<12} {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn status(machine: &MachineDto) -> &'static str {
    match (machine.state, machine.online) {
        (MachineStateDto::Pending, _) => "pending",
        (MachineStateDto::Active, true) => "online",
        (MachineStateDto::Active, false) => "offline",
    }
}

/// The machines as a table.
fn table(machines: &[MachineDto]) -> String {
    if machines.is_empty() {
        return "No machines yet.".to_owned();
    }
    let mut rows = vec![
        ["NAME", "STATE", "OS", "ARCH", "VERSION", "LAST SEEN"]
            .map(str::to_owned)
            .to_vec(),
    ];
    for machine in machines {
        let name = if machine.is_local {
            format!("{} (this server)", machine.name)
        } else {
            machine.name.clone()
        };
        let state = status(machine);
        let dash = |value: &Option<String>| value.clone().unwrap_or_else(|| "-".to_owned());
        rows.push(vec![
            name,
            state.to_owned(),
            dash(&machine.os),
            dash(&machine.arch),
            dash(&machine.version),
            machine.last_seen_at.map_or_else(
                || "never".to_owned(),
                |time| {
                    display(
                        time,
                        format_description!("[year]-[month]-[day] [hour]:[minute]"),
                    )
                },
            ),
        ]);
    }
    output::table(&rows)
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;

    fn machine(name: &str, is_local: bool) -> MachineDto {
        let time = datetime!(2026-11-13 9:30 UTC);
        MachineDto {
            id: Uuid::nil(),
            name: name.to_owned(),
            state: MachineStateDto::Active,
            is_local,
            online: is_local,
            check_code: "K7-4QXM".to_owned(),
            fingerprint: "SHA256:x".to_owned(),
            os: Some("linux".to_owned()),
            arch: Some("x86_64".to_owned()),
            version: Some("0.1.0".to_owned()),
            created_at: time,
            approved_at: Some(time),
            last_seen_at: is_local.then_some(time),
            outdated: false,
            missing_methods: Vec::new(),
        }
    }

    #[test]
    fn lists_machines_as_a_table() {
        let text = table(&[machine("atlas", true), machine("build-box", false)]);
        assert_eq!(
            text,
            "NAME                 STATE    OS     ARCH    VERSION  LAST SEEN\n\
             atlas (this server)  online   linux  x86_64  0.1.0    2026-11-13 09:30\n\
             build-box            offline  linux  x86_64  0.1.0    never\n"
        );
    }

    #[test]
    fn says_when_there_are_no_machines() {
        assert_eq!(table(&[]), "No machines yet.");
    }

    fn with_id(name: &str, id: &str) -> MachineDto {
        MachineDto {
            id: id.parse().unwrap(),
            ..machine(name, false)
        }
    }

    #[test]
    fn finds_a_machine_by_name_id_or_unique_id_prefix() {
        let machines = || {
            vec![
                with_id("atlas", "0193a7b0-0000-7000-8000-000000000001"),
                with_id("build-box", "0193a7b0-1111-7000-8000-000000000002"),
                with_id("laptop", "0194ffff-0000-7000-8000-000000000003"),
            ]
        };
        assert_eq!(resolve(machines(), "atlas").unwrap().name, "atlas");
        assert_eq!(
            resolve(machines(), "0193a7b0-1111-7000-8000-000000000002")
                .unwrap()
                .name,
            "build-box"
        );
        assert_eq!(resolve(machines(), "0194").unwrap().name, "laptop");
        assert_eq!(resolve(machines(), "0193A7B0-1").unwrap().name, "build-box");
        assert_eq!(
            resolve(machines(), "0193a7b0").unwrap_err().0,
            "machine_ambiguous"
        );
        assert_eq!(
            resolve(machines(), "019").unwrap_err().0,
            "machine_not_found"
        );
        assert_eq!(
            resolve(machines(), "nope").unwrap_err().0,
            "machine_not_found"
        );
    }

    #[test]
    fn shows_one_machine_field_by_field() {
        let text = details(&machine("atlas", true));
        assert!(text.starts_with("Name         atlas\nID           00000000-"));
        assert!(text.contains("State        online"));
        assert!(text.contains("This server  yes"));
        assert!(text.contains("Platform     linux x86_64"));
        assert!(text.contains("Last seen    2026-11-13 09:30 UTC"));
    }
}
