//! `ahab settings …`: show and change Ahab's own settings (the editor's),
//! for the server or a project.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Map, Value};

use crate::cli::client::{Client, urlencode};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::platform::setting::api::SettingsDto;

/// `ahab settings <section>` alone shows it.
#[derive(Debug, Args)]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub command: SettingsCommand,
}

#[derive(Debug, Subcommand)]
pub enum SettingsCommand {
    /// A section's settings: each value in effect, and where it is set.
    Show {
        /// The section, such as `editor`.
        section: String,
        /// A project's name or ID: its values, over the server's.
        #[arg(long)]
        project: Option<String>,
    },
    /// Set a setting, for the server or a project.
    Set {
        /// The section, such as `editor`.
        section: String,
        /// The setting, such as `fontSize`.
        name: String,
        /// Its value: `true`, `14`, `afterDelay`…
        value: String,
        /// Set it for this project (name or ID) only.
        #[arg(long)]
        project: Option<String>,
    },
    /// Unset a setting for the server or a project: the wider value, or
    /// its default, applies again.
    Unset {
        /// The section, such as `editor`.
        section: String,
        /// The setting, such as `fontSize`.
        name: String,
        /// Unset it for this project (name or ID).
        #[arg(long)]
        project: Option<String>,
    },
}

/// A value as typed: JSON when it reads as JSON (`true`, `14`), else text.
fn value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// The API's path for a section, for a project when given (resolved from
/// its name or ID).
async fn path(client: &Client, section: &str, project: Option<&str>) -> Result<String, Exit> {
    let base = format!("/api/v1/settings/{}", urlencode(section));
    let Some(project) = project else {
        return Ok(base);
    };
    let found = client
        .get(&format!("/api/v1/projects/{}", urlencode(project)))
        .await?;
    let id = found["id"].as_str().unwrap_or_default();
    Ok(format!("{base}?project={id}"))
}

fn table(settings: &SettingsDto) -> String {
    let mut rows = vec![vec![
        "SETTING".to_owned(),
        "VALUE".to_owned(),
        "SET FOR".to_owned(),
    ]];
    for field in &settings.fields {
        let value = settings
            .values
            .get(&field.name)
            .cloned()
            .unwrap_or(Value::Null);
        let set_for = if settings
            .project
            .as_ref()
            .is_some_and(|project| project.contains_key(&field.name))
        {
            "project"
        } else if settings.server.contains_key(&field.name) {
            "server"
        } else {
            "default"
        };
        rows.push(vec![
            field.name.clone(),
            output::text(&value),
            set_for.to_owned(),
        ]);
    }
    output::table(&rows)
}

pub async fn run(client: &Client, args: SettingsArgs) -> Result<(), Exit> {
    let answer = match args.command {
        SettingsCommand::Show { section, project } => {
            let path = path(client, &section, project.as_deref()).await?;
            client.get(&path).await?
        }
        SettingsCommand::Set {
            section,
            name,
            value: text,
            project,
        } => {
            let path = path(client, &section, project.as_deref()).await?;
            let mut change = Map::new();
            change.insert(name, value(&text));
            client
                .send(Method::PATCH, &path, Some(Value::Object(change)))
                .await?
        }
        SettingsCommand::Unset {
            section,
            name,
            project,
        } => {
            let path = path(client, &section, project.as_deref()).await?;
            let mut change = Map::new();
            change.insert(name, Value::Null);
            client
                .send(Method::PATCH, &path, Some(Value::Object(change)))
                .await?
        }
    };
    let settings: SettingsDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || table(&settings));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_json_or_text() {
        assert_eq!(value("true"), Value::Bool(true));
        assert_eq!(value("14"), Value::from(14));
        assert_eq!(value("afterDelay"), Value::from("afterDelay"));
    }
}
