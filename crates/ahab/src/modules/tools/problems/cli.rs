//! `ahab problems` and `ahab check`: a project's problems, and running
//! its check commands now.

use clap::Args;
use reqwest::Method;
use serde_json::{Value, json};

use crate::cli::client::{Client, urlencode};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::tools::problems::api::{ChecksDto, CountsDto, ProblemListDto, SeverityDto};

#[derive(Debug, Args)]
pub struct ProblemsArgs {
    /// The project's ID. Default: `AHAB_PROJECT` (set in Ahab's
    /// terminals and agents), else the project this folder is in.
    #[arg(long)]
    project: Option<uuid::Uuid>,
    /// The lowest severity shown: error, warning, information, hint.
    #[arg(long)]
    severity: Option<String>,
    /// Only this tool's problems (`rust-analyzer`, `cargo-clippy`).
    #[arg(long)]
    source: Option<String>,
    /// Only this file's, or a folder's.
    #[arg(long)]
    path: Option<String>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// The project's ID. Default: `AHAB_PROJECT`, else the project
    /// this folder is in.
    #[arg(long)]
    project: Option<uuid::Uuid>,
    /// Only this check (by tool ID); may be given more than once.
    #[arg(long = "tool")]
    tools: Vec<String>,
    /// Only within this repository or worktree (its folder in the
    /// project); may be given more than once.
    #[arg(long = "repository")]
    repositories: Vec<String>,
}

/// Prints a project's problems.
pub async fn problems(client: &Client, args: ProblemsArgs) -> Result<(), Exit> {
    let project = project_of(client, args.project).await?;
    let mut query = Vec::new();
    for (name, value) in [
        ("severity", &args.severity),
        ("source", &args.source),
        ("path", &args.path),
    ] {
        if let Some(value) = value {
            query.push(format!("{name}={}", urlencode(value)));
        }
    }
    let query = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
    };
    let answer = client
        .get(&format!("/api/v1/projects/{project}/problems{query}"))
        .await?;
    let list: ProblemListDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || {
        if list.items.is_empty() {
            return format!("No problems. ({})", counts(&list.counts));
        }
        let mut lines: Vec<String> = list
            .items
            .iter()
            .map(|problem| {
                format!(
                    "{}:{}:{}: {}: {}{} [{}]",
                    problem.path,
                    problem.line,
                    problem.column,
                    severity(problem.severity),
                    problem.message.lines().next().unwrap_or_default(),
                    problem
                        .code
                        .as_ref()
                        .map(|code| format!(" ({code})"))
                        .unwrap_or_default(),
                    problem.source
                )
            })
            .collect();
        lines.push(format!("\n{}", counts(&list.counts)));
        lines.join("\n")
    });
    Ok(())
}

/// Runs the check commands now; exits 1 when errors remain, like a linter.
pub async fn check(client: &Client, args: CheckArgs) -> Result<(), Exit> {
    let project = project_of(client, args.project).await?;
    let mut body = json!({});
    if !args.tools.is_empty() {
        body["tools"] = json!(args.tools);
    }
    if !args.repositories.is_empty() {
        body["checkouts"] = json!(args.repositories);
    }
    let answer = client
        .send(
            Method::POST,
            &format!("/api/v1/projects/{project}/checks"),
            Some(body),
        )
        .await?;
    let checks: ChecksDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || {
        let mut lines: Vec<String> = checks
            .checks
            .iter()
            .map(|check| {
                let at = if check.root.is_empty() {
                    String::new()
                } else {
                    format!(" in {}", check.root)
                };
                if check.ok {
                    format!("{}{at}: {}", check.name, counts(&check.counts))
                } else {
                    format!(
                        "{}{at}: did not run: {}",
                        check.name,
                        check.message.clone().unwrap_or_default()
                    )
                }
            })
            .collect();
        if lines.is_empty() {
            lines.push("No check commands run on request here.".to_owned());
        }
        lines.push(format!("\nThe project now: {}", counts(&checks.counts)));
        lines.join("\n")
    });
    if checks.counts.errors > 0 || checks.checks.iter().any(|check| !check.ok) {
        return Err(Exit::Other);
    }
    Ok(())
}

fn severity(severity: SeverityDto) -> &'static str {
    match severity {
        SeverityDto::Error => "error",
        SeverityDto::Warning => "warning",
        SeverityDto::Information => "info",
        SeverityDto::Hint => "hint",
    }
}

fn counts(counts: &CountsDto) -> String {
    format!(
        "{} errors, {} warnings, {} information, {} hints",
        counts.errors, counts.warnings, counts.information, counts.hints
    )
}

/// The project given, else `AHAB_PROJECT`, else the one whose folder
/// holds the current directory on the server's own machine.
async fn project_of(client: &Client, given: Option<uuid::Uuid>) -> Result<uuid::Uuid, Exit> {
    if let Some(given) = given {
        return Ok(given);
    }
    if let Some(id) = std::env::var("AHAB_PROJECT")
        .ok()
        .and_then(|id| uuid::Uuid::parse_str(&id).ok())
    {
        return Ok(id);
    }
    let here = std::env::current_dir()
        .ok()
        .and_then(|dir| std::fs::canonicalize(dir).ok());
    let machines = client.get("/api/v1/machines").await?;
    let local: Vec<Value> = machines["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|machine| machine["isLocal"] == true)
        .collect();
    let projects = client.get("/api/v1/projects").await?;
    let mut best: Option<(usize, uuid::Uuid)> = None;
    if let Some(here) = &here {
        for project in projects["items"].as_array().into_iter().flatten() {
            let on_local = local.iter().any(|m| m["id"] == project["machineId"]);
            let path = project["path"].as_str().unwrap_or_default();
            if on_local
                && !path.is_empty()
                && here.starts_with(path)
                && best.is_none_or(|(length, _)| path.len() > length)
                && let Some(id) = project["id"]
                    .as_str()
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
            {
                best = Some((path.len(), id));
            }
        }
    }
    best.map(|(_, id)| id).ok_or_else(|| {
        output::failure(
            client.json(),
            Exit::Usage,
            "Which project?",
            "This folder is in no project on this server's machine: give --project.",
        )
    })
}
