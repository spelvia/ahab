//! `ahab run <plugin>.<command> [field=value…]`: runs a plugin's command
//! through the API and prints what it answered. `ahab run <plugin>` lists
//! the plugin's commands and their fields.

use clap::Args;
use reqwest::Method;
use serde_json::{Map, Value, json};

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// The command, such as `docker.containers.list`; a plugin's ID alone
    /// lists its commands.
    pub command: String,
    /// The input's fields, such as `all=true` or `name=web` (values that
    /// read as JSON are JSON, the rest are text).
    pub fields: Vec<String>,
    /// The whole input as JSON instead.
    #[arg(long, conflicts_with = "fields")]
    pub input: Option<String>,
    /// The machine to run it on (its name or ID).
    #[arg(long, conflicts_with = "project")]
    pub machine: Option<String>,
    /// A project's ID: the command runs on its machine with its folder.
    #[arg(long)]
    pub project: Option<String>,
    /// Print a list of objects as a table.
    #[arg(long)]
    pub table: bool,
}

/// The input from `field=value` arguments.
fn input_of(fields: &[String]) -> Result<Value, String> {
    let mut input = Map::new();
    for field in fields {
        let Some((name, value)) = field.split_once('=') else {
            return Err(format!("{field}: write fields as name=value"));
        };
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
        input.insert(name.to_owned(), value);
    }
    Ok(Value::Object(input))
}

/// A value as a table cell.
fn cell(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A list of objects as a table, its columns the first object's keys.
fn table(items: &[Value]) -> Option<String> {
    let first = items.first()?.as_object()?;
    let columns: Vec<&String> = first.keys().collect();
    let mut rows = vec![columns.iter().map(|c| c.to_uppercase()).collect::<Vec<_>>()];
    for item in items {
        rows.push(
            columns
                .iter()
                .map(|column| cell(&item[column.as_str()]))
                .collect(),
        );
    }
    Some(output::table(&rows))
}

/// The plugin's commands, as its running entry declared them.
async fn describe(client: &Client, plugin: &str) -> Result<(), Exit> {
    let states = client.get("/api/v1/plugin-states").await?;
    let declared = states["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["plugin"] == plugin && !item["declared"].is_null())
        })
        .map(|item| item["declared"].clone());
    let Some(declared) = declared else {
        return Err(output::failure(
            client.json(),
            Exit::Other,
            "Not running",
            &format!("{plugin} is not running on any machine, so its commands are unknown."),
        ));
    };
    let commands = declared["commands"].clone();
    output::print(client.json(), &commands, || {
        let mut lines = Vec::new();
        for command in commands.as_array().into_iter().flatten() {
            let name = command["name"].as_str().unwrap_or_default();
            let required: Vec<&str> = command["input"]["required"]
                .as_array()
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let fields: Vec<String> = command["input"]["properties"]
                .as_object()
                .map(|properties| {
                    properties
                        .iter()
                        .map(|(field, schema)| {
                            let kind = schema["type"].as_str().unwrap_or("any");
                            if required.contains(&field.as_str()) {
                                format!("{field}=<{kind}>")
                            } else {
                                format!("[{field}=<{kind}>]")
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            lines.push(
                format!("ahab run {plugin}.{name} {}", fields.join(" "))
                    .trim_end()
                    .to_owned(),
            );
            if let Some(description) = command["description"].as_str() {
                lines.push(format!("    {description}"));
            }
        }
        if lines.is_empty() {
            format!("{plugin} has no commands.")
        } else {
            lines.join("\n")
        }
    });
    Ok(())
}

/// A machine's ID from its name or ID.
async fn machine_id(client: &Client, named: &str) -> Result<String, Exit> {
    let answer = client.get("/api/v1/machines").await?;
    answer["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|machine| machine["name"] == named || machine["id"] == named)
        })
        .and_then(|machine| machine["id"].as_str().map(str::to_owned))
        .ok_or_else(|| {
            output::failure(
                client.json(),
                Exit::Usage,
                "Not found",
                &format!("There is no machine {named}."),
            )
        })
}

pub async fn run(client: &Client, args: RunArgs) -> Result<(), Exit> {
    let Some((plugin, name)) = args.command.split_once('.') else {
        return describe(client, &args.command).await;
    };
    let input = match &args.input {
        Some(json) => serde_json::from_str(json).map_err(|error| {
            output::failure(
                client.json(),
                Exit::Usage,
                "Not JSON",
                &format!("--input: {error}"),
            )
        })?,
        None => input_of(&args.fields).map_err(|error| {
            output::failure(client.json(), Exit::Usage, "Unclear input", &error)
        })?,
    };
    let mut body = json!({ "input": input });
    if let Some(machine) = &args.machine {
        body["machine"] = json!(machine_id(client, machine).await?);
    }
    if let Some(project) = &args.project {
        body["project"] = json!(project);
    }
    let answer = client
        .send(
            Method::POST,
            &format!("/api/v1/plugins/{plugin}/commands/{name}"),
            Some(body),
        )
        .await?;
    let output_value = answer["output"].clone();
    output::print(client.json(), &output_value, || {
        if args.table
            && let Some(text) = output_value.as_array().and_then(|items| table(items))
        {
            return text;
        }
        serde_json::to_string_pretty(&output_value).unwrap_or_default()
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::input_of;

    #[test]
    fn fields_become_the_input() {
        let input = input_of(&["all=true".into(), "name=web".into(), "count=3".into()]).unwrap();
        assert_eq!(input, json!({ "all": true, "name": "web", "count": 3 }));
        assert!(input_of(&["nope".into()]).is_err());
    }
}
