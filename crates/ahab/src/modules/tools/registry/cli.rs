//! `ahab lsp`: the tool registry on a machine (states, runtimes), its
//! running language servers, and installing, updating and removing tools.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::tools::lsp::api::LanguageServerListDto;
use crate::modules::tools::registry::api::{MachineToolsDto, ToolStateDto, ToolStatusDto};

#[derive(Debug, Args)]
pub struct LspArgs {
    #[command(subcommand)]
    command: LspCommand,
}

#[derive(Debug, Subcommand)]
enum LspCommand {
    /// Every tool in the registry with its state on a machine, and the
    /// machine's runtimes.
    List {
        /// The machine: its name or ID. Default: the server's own.
        #[arg(long)]
        machine: Option<String>,
        /// Only this language's tools.
        #[arg(long)]
        language: Option<String>,
        /// Look for runtimes and tools again.
        #[arg(long)]
        rescan: bool,
    },
    /// Install a tool on a machine, into Ahab's data folder there.
    Install(ToolArgs),
    /// Install the registry's newer version of a tool Ahab installed.
    Update(ToolArgs),
    /// Remove a tool Ahab installed.
    Remove(ToolArgs),
    /// The language servers a machine runs.
    Status {
        /// The machine: its name or ID. Default: the server's own.
        #[arg(long)]
        machine: Option<String>,
    },
}

#[derive(Debug, Args)]
struct ToolArgs {
    /// The tool's ID, as `ahab lsp list` shows it (`rust-analyzer`).
    tool: String,
    /// The machine: its name or ID. Default: the server's own.
    #[arg(long)]
    machine: Option<String>,
    /// Return once it started, instead of waiting for it to finish.
    #[arg(long)]
    no_wait: bool,
}

pub async fn run(client: &Client, args: LspArgs) -> Result<(), Exit> {
    match args.command {
        LspCommand::List {
            machine,
            language,
            rescan,
        } => {
            let (machine, name) = machine_of(client, machine.as_deref()).await?;
            let rescan = if rescan { "?rescan=true" } else { "" };
            let answer = client
                .get(&format!("/api/v1/machines/{machine}/tools{rescan}"))
                .await?;
            let tools: MachineToolsDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                table(&name, &tools, language.as_deref())
            });
            Ok(())
        }
        LspCommand::Install(args) => change(client, args, "install").await,
        LspCommand::Update(args) => change(client, args, "update").await,
        LspCommand::Remove(args) => change(client, args, "remove").await,
        LspCommand::Status { machine } => {
            let (machine, name) = machine_of(client, machine.as_deref()).await?;
            let answer = client
                .get(&format!("/api/v1/machines/{machine}/language-servers"))
                .await?;
            let list: LanguageServerListDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                if list.items.is_empty() {
                    return format!("No language servers run on {name}.");
                }
                let mut rows = vec![vec![
                    "TOOL".to_owned(),
                    "STATE".to_owned(),
                    "MEMORY".to_owned(),
                    "ROOT".to_owned(),
                ]];
                for server in &list.items {
                    rows.push(vec![
                        server.tool.clone(),
                        server.state.clone(),
                        server
                            .memory
                            .map_or_else(|| "-".to_owned(), |bytes| format!("{} MB", bytes >> 20)),
                        server.root.clone(),
                    ]);
                }
                output::table(&rows)
            });
            Ok(())
        }
    }
}

/// Starts a job and, unless asked not to, waits until the tool's state
/// shows it done.
async fn change(client: &Client, args: ToolArgs, action: &str) -> Result<(), Exit> {
    let (machine, name) = machine_of(client, args.machine.as_deref()).await?;
    let answer = client
        .send(
            Method::POST,
            &format!("/api/v1/machines/{machine}/tools/{}/{action}", args.tool),
            Some(json!({})),
        )
        .await?;
    if args.no_wait {
        output::print(client.json(), &answer, || {
            format!("Started: {action} {} on {name}.", args.tool)
        });
        return Ok(());
    }
    // The job is done when the machine no longer lists it as running.
    for _ in 0..1200 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let listed = client
            .get(&format!("/api/v1/machines/{machine}/tools"))
            .await?;
        let tools: MachineToolsDto = client.parse(listed)?;
        let Some(job) = tools.jobs.iter().find(|job| job.tool == args.tool) else {
            continue;
        };
        match job.state.as_str() {
            "done" => {
                let tool = tools.tools.iter().find(|tool| tool.id == args.tool);
                let value = json!({ "job": job, "tool": tool });
                output::print(client.json(), &value, || match action {
                    "remove" => format!("Removed {} from {name}.", args.tool),
                    _ => format!(
                        "Installed {} {} on {name}.",
                        args.tool,
                        tool.and_then(|t| t.found_version.clone())
                            .unwrap_or_default()
                    ),
                });
                return Ok(());
            }
            "failed" => {
                let detail = format!(
                    "{} failed at {}: {}{}",
                    action,
                    job.step,
                    job.message.clone().unwrap_or_default(),
                    if job.output.is_empty() {
                        String::new()
                    } else {
                        format!("\n\n{}", job.output.join("\n"))
                    }
                );
                return Err(output::failure(
                    client.json(),
                    Exit::Other,
                    "Failed",
                    &detail,
                ));
            }
            _ => {}
        }
    }
    Err(output::failure(
        client.json(),
        Exit::Temporary,
        "Still running",
        "The job did not finish in ten minutes; `ahab lsp list` shows its state.",
    ))
}

/// The machine named, or the server's own: its ID and name.
async fn machine_of(
    client: &Client,
    reference: Option<&str>,
) -> Result<(uuid::Uuid, String), Exit> {
    let answer = client.get("/api/v1/machines").await?;
    let items = answer["items"].as_array().cloned().unwrap_or_default();
    let found = items.iter().find(|machine| match reference {
        Some(reference) => {
            machine["name"] == reference
                || machine["id"].as_str().is_some_and(|id| {
                    id == reference || (reference.len() >= 4 && id.starts_with(reference))
                })
        }
        None => machine["isLocal"] == true,
    });
    let Some(machine) = found else {
        let detail = match reference {
            Some(reference) => format!("There is no machine called {reference}."),
            None => "The server has no machine of its own: name one with --machine.".to_owned(),
        };
        output::error(
            client.json(),
            &json!({ "title": "Not found", "code": "machine_not_found", "detail": detail }),
        );
        return Err(Exit::Other);
    };
    let id = machine["id"]
        .as_str()
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .ok_or(Exit::Other)?;
    Ok((id, machine["name"].as_str().unwrap_or_default().to_owned()))
}

fn state(tool: &ToolStatusDto) -> String {
    let base = match tool.state {
        ToolStateDto::Installed => "installed by Ahab".to_owned(),
        ToolStateDto::Found => tool.detail.clone().unwrap_or_else(|| "found".to_owned()),
        ToolStateDto::Available => "available".to_owned(),
        ToolStateDto::Missing => "not found".to_owned(),
        ToolStateDto::CannotRun => tool
            .reason
            .clone()
            .unwrap_or_else(|| "cannot run here".to_owned()),
        ToolStateDto::Off => "off".to_owned(),
        ToolStateDto::Unknown => "unknown".to_owned(),
    };
    if tool.update_available {
        format!(
            "{base} (update: {})",
            tool.version.clone().unwrap_or_default()
        )
    } else {
        base
    }
}

fn table(machine: &str, tools: &MachineToolsDto, language: Option<&str>) -> String {
    let runtimes: Vec<String> = tools
        .runtimes
        .iter()
        .map(|runtime| {
            format!(
                "{} {}",
                runtime.name,
                runtime.version.clone().unwrap_or_else(|| "—".to_owned())
            )
        })
        .collect();
    let mut rows = vec![vec![
        "LANGUAGE".to_owned(),
        "TOOL".to_owned(),
        "KIND".to_owned(),
        "STATE".to_owned(),
        "VERSION".to_owned(),
    ]];
    for tool in &tools.tools {
        if language.is_some_and(|language| !tool.languages.iter().any(|l| l == language)) {
            continue;
        }
        rows.push(vec![
            tool.languages.first().cloned().unwrap_or_default(),
            tool.id.clone(),
            tool.kind.clone(),
            state(tool),
            tool.found_version
                .clone()
                .or_else(|| tool.version.clone())
                .unwrap_or_else(|| "-".to_owned()),
        ]);
    }
    format!(
        "{machine} ({}): {}\n\n{}",
        tools.platform,
        runtimes.join(", "),
        output::table(&rows)
    )
}
