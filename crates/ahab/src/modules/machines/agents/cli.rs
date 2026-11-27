//! `ahab machines agents <machine>`: which coding agents a machine can run.

use reqwest::Method;
use serde_json::json;
use time::macros::format_description;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::machines::agents::api::{AgentStateDto, AgentStatusDto, AgentStatusListDto};
use crate::shared::time::display;

/// Prints the agents on the machine with this ID, detecting them again first
/// when `refresh` is set.
pub async fn run(
    client: &Client,
    machine: uuid::Uuid,
    name: &str,
    refresh: bool,
) -> Result<(), Exit> {
    let path = format!("/api/v1/machines/{machine}/agents");
    let answer = if refresh {
        client
            .send(Method::POST, &format!("{path}/refresh"), Some(json!({})))
            .await?
    } else {
        client.get(&path).await?
    };
    let list: AgentStatusListDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || table(name, &list.items));
    Ok(())
}

fn state(agent: &AgentStatusDto) -> &'static str {
    match agent.state {
        AgentStateDto::Ready => "ready",
        AgentStateDto::NotSignedIn => "not signed in",
        AgentStateDto::UnsupportedVersion => "unsupported version",
        AgentStateDto::NotInstalled => "not installed",
        AgentStateDto::Error => "cannot tell",
    }
}

fn table(machine: &str, agents: &[AgentStatusDto]) -> String {
    if agents.is_empty() {
        return format!(
            "No agents detected on {machine} yet. Run `ahab machines agents {machine} --refresh`."
        );
    }
    let mut lines = vec![format!(
        "{:<13} {:<15} {:<10} {}",
        "AGENT", "STATE", "VERSION", "DETAIL"
    )];
    for agent in agents {
        let detail = agent
            .account
            .clone()
            .or_else(|| agent.detail.clone())
            .unwrap_or_default();
        lines.push(format!(
            "{:<13} {:<15} {:<10} {}",
            agent.label,
            state(agent),
            agent.version.as_deref().unwrap_or("-"),
            detail
        ));
    }
    if let Some(checked) = agents.iter().map(|agent| agent.checked_at).max() {
        lines.push(format!(
            "\nChecked {}.",
            display(
                checked,
                format_description!("[year]-[month]-[day] [hour]:[minute] UTC")
            )
        ));
    }
    lines.join("\n")
}
