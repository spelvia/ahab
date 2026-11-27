//! `ahab plugins …`: install, enable, disable, update and remove plugins
//! through the API. A plugin is named by its ID.

use std::fmt::Write as _;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Value, json};

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::modules::plugins::plugin::api::{InstalledPluginDto, PluginDto, PluginListDto};

/// `ahab plugins` alone lists them.
#[derive(Debug, Args)]
pub struct PluginsArgs {
    #[command(subcommand)]
    pub command: Option<PluginsCommand>,
}

#[derive(Debug, Subcommand)]
pub enum PluginsCommand {
    /// The installed plugins, in the order their interceptors run.
    #[command(visible_alias = "ls")]
    List,
    /// Everything about one plugin.
    Show {
        /// The plugin's ID.
        plugin: String,
    },
    /// Install a plugin from a folder on the server's host or a Git URL. It
    /// starts disabled.
    Add {
        /// A folder (`./ahab-docker`) or a Git URL.
        location: String,
    },
    /// Turn a plugin on.
    Enable {
        /// The plugin's ID.
        plugin: String,
        /// Where it runs, for a plugin that asks to be placed (repeat it).
        #[arg(long = "machine")]
        machines: Vec<String>,
    },
    /// Turn a plugin off everywhere.
    Disable {
        /// The plugin's ID.
        plugin: String,
    },
    /// Work on a plugin: install the folder (on the server's host), enable
    /// it, rebuild it as it changes and update it after every build, until
    /// Ctrl+C. Ahab is not restarted: the plugin's machine side restarts
    /// and its web side reloads.
    Dev {
        /// The plugin's folder.
        folder: String,
        /// Do not run its build in watch mode (it is built some other way).
        #[arg(long)]
        no_build: bool,
    },
    /// Install a plugin again from where it came from.
    Update {
        /// The plugin's ID.
        plugin: String,
    },
    /// A plugin's logs on a machine: what it printed and logged.
    Logs {
        /// The plugin's ID.
        plugin: String,
        /// The machine; the server's own when there is one.
        #[arg(long)]
        machine: Option<String>,
    },
    /// Remove a disabled plugin.
    #[command(visible_alias = "rm")]
    Remove {
        /// The plugin's ID.
        plugin: String,
        /// Do not ask first.
        #[arg(long, short)]
        yes: bool,
    },
}

/// Whether `location` is a Git URL rather than a folder.
fn is_git(location: &str) -> bool {
    location.starts_with("https://")
        || location.starts_with("http://")
        || location.starts_with("git@")
        || location.starts_with("ssh://")
        || std::path::Path::new(location)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("git"))
}

fn warn(installed: &InstalledPluginDto) -> String {
    installed
        .warnings
        .iter()
        .fold(String::new(), |mut text, warning| {
            let _ = write!(text, "\n  warning: {warning}");
            text
        })
}

fn details(plugin: &PluginDto) -> String {
    let state = if plugin.enabled {
        "enabled"
    } else {
        "disabled"
    };
    let mut text = format!(
        "{} {} ({})\n  {state}, from {} ({})\n  entries: {}",
        plugin.name,
        plugin.version,
        plugin.id,
        plugin.location,
        serde_json::to_value(plugin.source)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        plugin.entries.join(", "),
    );
    if let Some(description) = &plugin.description {
        text = format!("{text}\n  {description}");
    }
    if !plugin.hooks.is_empty() {
        text = format!("{text}\n  hooks: {}", plugin.hooks.join(", "));
    }
    text
}

/// Machines named on the command line, as IDs.
async fn machine_ids(client: &Client, names: &[String]) -> Result<Vec<String>, Exit> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let answer = client.get("/api/v1/machines").await?;
    let items = answer["items"].as_array().cloned().unwrap_or_default();
    names
        .iter()
        .map(|name| {
            items
                .iter()
                .find(|machine| machine["name"] == name.as_str() || machine["id"] == name.as_str())
                .and_then(|machine| machine["id"].as_str().map(str::to_owned))
                .ok_or_else(|| {
                    output::failure(
                        client.json(),
                        Exit::Other,
                        "Not found",
                        &format!("There is no machine {name}."),
                    )
                })
        })
        .collect()
}

/// Every plugin's state on the machines online, as `(plugin, machine, state,
/// message)`.
async fn states(client: &Client) -> Result<Vec<(String, String, String, Option<String>)>, Exit> {
    let answer = client.get("/api/v1/plugin-states").await?;
    let machines = client.get("/api/v1/machines").await?;
    let name = |id: &str| {
        machines["items"]
            .as_array()
            .and_then(|items| items.iter().find(|machine| machine["id"] == id))
            .and_then(|machine| machine["name"].as_str())
            .unwrap_or(id)
            .to_owned()
    };
    Ok(answer["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|item| {
            (
                item["plugin"].as_str().unwrap_or_default().to_owned(),
                name(item["machine"].as_str().unwrap_or_default()),
                item["state"].as_str().unwrap_or_default().to_owned(),
                item["message"].as_str().map(str::to_owned),
            )
        })
        .collect())
}

/// The machine to read logs from: the one named, else the server's own.
async fn logs_machine(client: &Client, named: Option<String>) -> Result<String, Exit> {
    let answer = client.get("/api/v1/machines").await?;
    let items = answer["items"].as_array().cloned().unwrap_or_default();
    let found = match &named {
        Some(name) => items
            .iter()
            .find(|machine| machine["name"] == name.as_str() || machine["id"] == name.as_str()),
        None => items.iter().find(|machine| machine["isLocal"] == true),
    };
    found
        .and_then(|machine| machine["id"].as_str().map(str::to_owned))
        .ok_or_else(|| {
            output::failure(
                client.json(),
                Exit::Usage,
                "Which machine?",
                &named.map_or_else(
                    || {
                        "This server has no machine of its own; say which with --machine."
                            .to_owned()
                    },
                    |name| format!("There is no machine {name}."),
                ),
            )
        })
}

pub async fn run(client: &Client, args: PluginsArgs) -> Result<(), Exit> {
    match args.command.unwrap_or(PluginsCommand::List) {
        PluginsCommand::List => {
            let answer = client.get("/api/v1/plugins").await?;
            let list: PluginListDto = client.parse(answer.clone())?;
            let running = states(client).await?;
            output::print(client.json(), &answer, || {
                if list.items.is_empty() {
                    return "No plugins. Add one with `ahab plugins add <folder or Git URL>`."
                        .to_owned();
                }
                let mut rows = vec![vec![
                    "ID".to_owned(),
                    "NAME".to_owned(),
                    "VERSION".to_owned(),
                    "STATE".to_owned(),
                    "ENTRIES".to_owned(),
                    "MACHINES".to_owned(),
                ]];
                rows.extend(list.items.iter().map(|plugin| {
                    vec![
                        plugin.id.clone(),
                        plugin.name.clone(),
                        plugin.version.clone(),
                        if plugin.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                        .to_owned(),
                        plugin.entries.join(", "),
                        running
                            .iter()
                            .filter(|(id, ..)| id == &plugin.id)
                            .map(|(_, machine, state, _)| format!("{machine}: {state}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ]
                }));
                output::table(&rows)
            });
            Ok(())
        }
        PluginsCommand::Show { plugin } => {
            let answer = client.get(&format!("/api/v1/plugins/{plugin}")).await?;
            let shown: PluginDto = client.parse(answer.clone())?;
            let running = states(client).await?;
            output::print(client.json(), &answer, || {
                let mut text = details(&shown);
                for (_, machine, state, message) in
                    running.iter().filter(|(id, ..)| id == &shown.id)
                {
                    let _ = write!(text, "\n  on {machine}: {state}");
                    if let Some(message) = message {
                        let _ = write!(text, " ({message})");
                    }
                }
                text
            });
            Ok(())
        }
        PluginsCommand::Add { location } => {
            let body = if is_git(&location) {
                json!({ "source": "git", "location": location })
            } else {
                let folder = std::fs::canonicalize(&location)
                    .map_or(location.clone(), |path| path.display().to_string());
                json!({ "source": "folder", "location": folder })
            };
            let answer = client
                .send(Method::POST, "/api/v1/plugins", Some(body))
                .await?;
            let installed: InstalledPluginDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "Installed {} {} ({}). Turn it on with `ahab plugins enable {}`.{}",
                    installed.plugin.name,
                    installed.plugin.version,
                    installed.plugin.id,
                    installed.plugin.id,
                    warn(&installed)
                )
            });
            Ok(())
        }
        PluginsCommand::Enable { plugin, machines } => {
            let ids = machine_ids(client, &machines).await?;
            let body = if ids.is_empty() {
                json!({})
            } else {
                json!({ "machines": ids })
            };
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/plugins/{plugin}/enable"),
                    Some(body),
                )
                .await?;
            let enabled: PluginDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!("Enabled {}.", enabled.name)
            });
            Ok(())
        }
        PluginsCommand::Disable { plugin } => {
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/plugins/{plugin}/disable"),
                    Some(json!({})),
                )
                .await?;
            let disabled: PluginDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!("Disabled {}.", disabled.name)
            });
            Ok(())
        }
        PluginsCommand::Dev { folder, no_build } => dev(client, &folder, no_build).await,
        PluginsCommand::Update { plugin } => {
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/plugins/{plugin}/update"),
                    Some(json!({})),
                )
                .await?;
            let installed: InstalledPluginDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "{} is at {} ({}).{}",
                    installed.plugin.name,
                    installed.plugin.version,
                    &installed.plugin.digest[..12.min(installed.plugin.digest.len())],
                    warn(&installed)
                )
            });
            Ok(())
        }
        PluginsCommand::Logs { plugin, machine } => {
            let machine = logs_machine(client, machine).await?;
            let answer = client
                .get(&format!("/api/v1/plugins/{plugin}/logs?machine={machine}"))
                .await?;
            output::print(client.json(), &answer, || {
                let lines: Vec<&str> = answer["lines"]
                    .as_array()
                    .map(|lines| lines.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if lines.is_empty() {
                    format!("{plugin} has logged nothing there yet.")
                } else {
                    lines.join("\n")
                }
            });
            Ok(())
        }
        PluginsCommand::Remove { plugin, yes } => {
            if !yes {
                let question = format!("Remove the plugin {plugin} and its files?");
                if !prompt::confirm(client.json(), &question, false, "--yes")? {
                    return Err(output::failure(
                        client.json(),
                        Exit::Other,
                        "Not removed",
                        &format!("{plugin} stays."),
                    ));
                }
            }
            client
                .send(Method::DELETE, &format!("/api/v1/plugins/{plugin}"), None)
                .await?;
            output::print(client.json(), &json!({ "removed": plugin }), || {
                format!("Removed {plugin}.")
            });
            Ok(())
        }
    }
}

/// `ahab plugins dev`: install or update the folder, enable it, and update
/// it after every build until interrupted.
async fn dev(client: &Client, folder: &str, no_build: bool) -> Result<(), Exit> {
    let root = std::fs::canonicalize(folder).map_err(|_| {
        output::failure(
            client.json(),
            Exit::Usage,
            "No such folder",
            &format!("There is no folder {folder}."),
        )
    })?;
    let manifest: serde_json::Value = std::fs::read_to_string(root.join("ahab-plugin.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| {
            output::failure(
                client.json(),
                Exit::Usage,
                "Not a plugin",
                &format!("{} has no readable ahab-plugin.json.", root.display()),
            )
        })?;
    let id = manifest["id"].as_str().unwrap_or_default().to_owned();
    let location = root.display().to_string();

    // Built once first, so there is something to install.
    let mut build = None;
    if !no_build {
        let status = tokio::process::Command::new("npx")
            .args(["--no-install", "ahab-plugin", "build"])
            .current_dir(&root)
            .status()
            .await;
        if !status.is_ok_and(|status| status.success()) {
            return Err(output::failure(
                client.json(),
                Exit::Other,
                "The build failed",
                "Fix it, or build the plugin yourself and use --no-build.",
            ));
        }
        build = tokio::process::Command::new("npx")
            .args(["--no-install", "ahab-plugin", "build", "--watch"])
            .current_dir(&root)
            .kill_on_drop(true)
            .spawn()
            .ok();
    }

    let existing = client.quiet_get(&format!("/api/v1/plugins/{id}")).await;
    match existing {
        Some(found) if found["location"] == location.as_str() => {
            client
                .send(
                    Method::POST,
                    &format!("/api/v1/plugins/{id}/update"),
                    Some(json!({})),
                )
                .await?;
        }
        Some(found) => {
            return Err(output::failure(
                client.json(),
                Exit::Other,
                "Installed from elsewhere",
                &format!(
                    "{id} is installed from {}; remove it first (ahab plugins remove {id}).",
                    found["location"].as_str().unwrap_or_default()
                ),
            ));
        }
        None => {
            client
                .send(
                    Method::POST,
                    "/api/v1/plugins",
                    Some(json!({ "source": "dev", "location": location })),
                )
                .await?;
        }
    }
    client
        .send(
            Method::POST,
            &format!("/api/v1/plugins/{id}/enable"),
            Some(json!({})),
        )
        .await?;
    eprintln!("Developing {id} from {location}. Ctrl+C stops watching; it stays installed.");

    // Every change to its build, a quarter second after the last.
    let (tell, mut changes) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| !matches!(event.kind, notify::EventKind::Access(_))) {
            let _ = tell.send(());
        }
    })
    .map_err(|error| {
        output::failure(
            client.json(),
            Exit::Other,
            "Cannot watch",
            &error.to_string(),
        )
    })?;
    let mut watched = vec![root.join("ahab-plugin.json")];
    if let Some(entries) = manifest["entries"].as_object() {
        for path in entries.values().filter_map(serde_json::Value::as_str) {
            if let Some(first) = std::path::Path::new(path).components().next() {
                watched.push(root.join(first.as_os_str()));
            }
        }
    }
    watched.dedup();
    for path in &watched {
        let _ =
            std::fs::create_dir_all(path.extension().map_or(path.as_path(), |_| root.as_path()));
        let _ = notify::Watcher::watch(&mut watcher, path, notify::RecursiveMode::Recursive);
    }
    let mut last = existing_digest(client, &id).await;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            changed = changes.recv() => {
                if changed.is_none() {
                    break;
                }
                // Wait until the build is done writing.
                loop {
                    tokio::select! {
                        () = tokio::time::sleep(std::time::Duration::from_millis(250)) => break,
                        more = changes.recv() => if more.is_none() { break },
                    }
                }
                let answer = client
                    .try_send(Method::POST, &format!("/api/v1/plugins/{id}/update"))
                    .await;
                match answer {
                    Ok(answer) => {
                        let digest = answer["plugin"]["digest"].as_str().unwrap_or_default().to_owned();
                        if digest != last {
                            eprintln!("Updated {id}: build {}.", &digest[..12.min(digest.len())]);
                            last = digest;
                        }
                    }
                    Err(reason) => eprintln!("Could not update {id}: {reason}. Waiting for the next change."),
                }
            }
        }
    }
    drop(watcher);
    if let Some(mut build) = build {
        let _ = build.kill().await;
    }
    eprintln!("Stopped watching {id}; it stays installed from {location}.");
    Ok(())
}

async fn existing_digest(client: &Client, id: &str) -> String {
    client
        .quiet_get(&format!("/api/v1/plugins/{id}"))
        .await
        .and_then(|found| found["digest"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::is_git;

    #[test]
    fn tells_git_urls_from_folders() {
        assert!(is_git("https://github.com/you/ahab-docker"));
        assert!(is_git("git@github.com:you/ahab-docker.git"));
        assert!(!is_git("./ahab-docker"));
        assert!(!is_git("/home/you/ahab-docker"));
    }
}
