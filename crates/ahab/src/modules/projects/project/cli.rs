//! `ahab projects …` and `ahab open`, through the API, in the shape of
//! `ahab machines`: the project is a positional argument (name or ID).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::json;

use crate::cli::client::{Client, urlencode};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::modules::projects::project::api::{
    AvailabilityDto, OpenedFolderDto, ProjectDto, ProjectListDto, ProjectModeDto, ProjectStateDto,
};

/// `ahab projects` alone lists them.
#[derive(Debug, Args)]
pub struct ProjectsArgs {
    #[command(subcommand)]
    pub command: Option<ProjectsCommand>,
}

#[derive(Debug, Subcommand)]
pub enum ProjectsCommand {
    /// The projects, by group.
    #[command(visible_alias = "ls")]
    List {
        /// List archived projects instead.
        #[arg(long)]
        archived: bool,
    },
    /// A project: its folder, machine, mode and group.
    Show {
        /// The project's name or ID.
        project: String,
    },
    /// Create a project for a folder on a machine.
    Create {
        /// The folder's absolute path on that machine.
        path: String,
        /// The machine's name or ID; this server's own by default.
        #[arg(long)]
        machine: Option<String>,
        /// The folder's name by default.
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// The heading it is listed under.
        #[arg(long)]
        group: Option<String>,
        /// Agents and tools may only read it.
        #[arg(long)]
        read_only: bool,
    },
    /// Change a project.
    Edit {
        /// The project's name or ID.
        project: String,
        #[arg(long)]
        name: Option<String>,
        /// A blank description removes it.
        #[arg(long)]
        description: Option<String>,
        /// A blank group removes it.
        #[arg(long)]
        group: Option<String>,
        #[arg(long, conflicts_with = "read_write")]
        read_only: bool,
        #[arg(long)]
        read_write: bool,
    },
    /// Hide a project; everything is kept.
    Archive { project: String },
    /// Bring an archived project back.
    Restore { project: String },
    /// Delete a project and what Ahab keeps for it. Files are never touched.
    #[command(visible_alias = "rm")]
    Remove {
        project: String,
        /// Do not ask first.
        #[arg(long, short)]
        yes: bool,
    },
}

/// `ahab open [path]`.
#[derive(Debug, Args)]
pub struct OpenArgs {
    /// The folder; the current one by default.
    #[arg(conflicts_with = "no_folder")]
    pub path: Option<PathBuf>,
    /// Open Ahab itself, without a folder: on this computer, the way to
    /// open a browser that is not paired yet.
    #[arg(long, conflicts_with = "project")]
    pub no_folder: bool,
    /// When the folder is several projects: which one (its name or ID).
    #[arg(long)]
    pub project: Option<String>,
    /// Print the address instead of opening a browser.
    #[arg(long)]
    pub no_browser: bool,
}

fn path_of(project: &str) -> String {
    format!("/api/v1/projects/{}", urlencode(project))
}

pub async fn run(client: &Client, args: ProjectsArgs) -> Result<(), Exit> {
    match args
        .command
        .unwrap_or(ProjectsCommand::List { archived: false })
    {
        ProjectsCommand::List { archived } => {
            let path = if archived {
                "/api/v1/projects?state=archived"
            } else {
                "/api/v1/projects"
            };
            let answer = client.get(path).await?;
            let list: ProjectListDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || table(&list.items, archived));
            Ok(())
        }
        ProjectsCommand::Show { project } => {
            let answer = client.get(&path_of(&project)).await?;
            let project: ProjectDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || details(&project));
            Ok(())
        }
        ProjectsCommand::Create {
            path,
            machine,
            name,
            description,
            group,
            read_only,
        } => {
            let machine = match machine {
                Some(machine) => machine,
                None => local_machine(client).await?,
            };
            let answer = client
                .send(
                    Method::POST,
                    "/api/v1/projects",
                    Some(json!({
                        "name": name,
                        "description": description,
                        "machine": machine,
                        "path": path,
                        "mode": if read_only { "readOnly" } else { "readWrite" },
                        "group": group,
                    })),
                )
                .await?;
            let project: ProjectDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "Created {} for {} on {} ({}).",
                    project.name, project.path, project.machine_name, project.id
                )
            });
            Ok(())
        }
        ProjectsCommand::Edit {
            project,
            name,
            description,
            group,
            read_only,
            read_write,
        } => {
            let mode = match (read_only, read_write) {
                (true, _) => Some("readOnly"),
                (_, true) => Some("readWrite"),
                _ => None,
            };
            if name.is_none() && description.is_none() && group.is_none() && mode.is_none() {
                return Err(output::failure(
                    client.json(),
                    Exit::Usage,
                    "Nothing to change",
                    "Say what to change, such as --name app, --group work or --read-only.",
                ));
            }
            let answer = client
                .send(
                    Method::PATCH,
                    &path_of(&project),
                    Some(json!({
                        "name": name,
                        "description": description,
                        "group": group,
                        "mode": mode,
                    })),
                )
                .await?;
            let edited: ProjectDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || format!("Saved {}.", edited.name));
            Ok(())
        }
        ProjectsCommand::Archive { project } => set_state(client, &project, "archived").await,
        ProjectsCommand::Restore { project } => set_state(client, &project, "active").await,
        ProjectsCommand::Remove { project, yes } => {
            let answer = client.get(&path_of(&project)).await?;
            let found: ProjectDto = client.parse(answer)?;
            if !yes {
                let question = format!(
                    "Delete {} ({} on {})? Files are not touched.",
                    found.name, found.path, found.machine_name
                );
                if !prompt::confirm(client.json(), &question, false, "--yes")? {
                    return Err(output::failure(
                        client.json(),
                        Exit::Other,
                        "Not deleted",
                        &format!("{} stays.", found.name),
                    ));
                }
            }
            client
                .send(Method::DELETE, &path_of(&found.id.to_string()), None)
                .await?;
            output::print(client.json(), &json!({ "removed": found.id }), || {
                format!("Deleted {}.", found.name)
            });
            Ok(())
        }
    }
}

async fn set_state(client: &Client, project: &str, state: &str) -> Result<(), Exit> {
    let answer = client
        .send(
            Method::PATCH,
            &path_of(project),
            Some(json!({ "state": state })),
        )
        .await?;
    let changed: ProjectDto = client.parse(answer.clone())?;
    output::print(client.json(), &answer, || match changed.state {
        ProjectStateDto::Archived => format!("Archived {}.", changed.name),
        ProjectStateDto::Active => format!("Restored {}.", changed.name),
    });
    Ok(())
}

/// `ahab open`: `fingerprint` is this computer's machine key fingerprint,
/// when it has one.
pub async fn open(
    client: &Client,
    args: OpenArgs,
    fingerprint: Option<String>,
) -> Result<(), Exit> {
    if args.no_folder {
        return open_ahab(client, args.no_browser).await;
    }
    let folder = match args.path {
        Some(path) => path,
        None => std::env::current_dir().map_err(|error| {
            output::failure(client.json(), Exit::Other, "No folder", &error.to_string())
        })?,
    };
    let folder = std::fs::canonicalize(&folder).map_err(|error| {
        output::failure(
            client.json(),
            Exit::Usage,
            "No such folder",
            &format!("{}: {error}", folder.display()),
        )
    })?;
    let answer = client
        .send(
            Method::POST,
            "/api/v1/projects/open",
            Some(json!({
                "machineFingerprint": fingerprint,
                "path": folder.display().to_string(),
                "project": args.project,
            })),
        )
        .await?;
    let opened: OpenedFolderDto = client.parse(answer.clone())?;
    let path = format!("/p/{}", opened.project.id);
    let url = format!("{}{path}", client.base());
    // The browser opens paired: through a one-time pairing link.
    let launched = !client.json()
        && !args.no_browser
        && webbrowser::open(&client.browser_link(&path).await).is_ok();
    output::print(client.json(), &answer, || {
        let what = if opened.created {
            format!("Created project {} for this folder.", opened.project.name)
        } else {
            format!("This folder is {}.", opened.project.name)
        };
        if launched {
            format!("{what}\nOpened {url}")
        } else {
            format!("{what}\n{url}")
        }
    });
    Ok(())
}

/// `ahab open --no-folder`: Ahab's start page in a browser, paired.
async fn open_ahab(client: &Client, no_browser: bool) -> Result<(), Exit> {
    let url = client.base().to_owned();
    let launched =
        !client.json() && !no_browser && webbrowser::open(&client.browser_link("/").await).is_ok();
    output::print(client.json(), &json!({ "url": url }), || {
        if launched {
            format!("Opened {url}")
        } else {
            url.clone()
        }
    });
    Ok(())
}

fn table(projects: &[ProjectDto], archived: bool) -> String {
    if projects.is_empty() {
        return if archived {
            "No archived projects.".to_owned()
        } else {
            "No projects yet. Run `ahab open` in a folder, or `ahab projects create <path>`."
                .to_owned()
        };
    }
    let mut lines = vec![format!(
        "{:<24} {:<16} {:<16} {}",
        "NAME", "GROUP", "MACHINE", "PATH"
    )];
    for project in grouped(projects) {
        lines.push(format!(
            "{:<24} {:<16} {:<16} {}",
            project.name,
            project.group.as_deref().unwrap_or("-"),
            project.machine_name,
            project.path
        ));
    }
    lines.join("\n")
}

/// Projects by group, in the order groups first appear, the ungrouped ones
/// last; within a group in the order given.
fn grouped(projects: &[ProjectDto]) -> Vec<&ProjectDto> {
    let mut groups: Vec<Option<&str>> = Vec::new();
    for project in projects {
        let group = project.group.as_deref();
        if group.is_some() && !groups.contains(&group) {
            groups.push(group);
        }
    }
    groups.push(None);
    groups
        .into_iter()
        .flat_map(|group| projects.iter().filter(move |p| p.group.as_deref() == group))
        .collect()
}

/// The server's own machine, by ID.
async fn local_machine(client: &Client) -> Result<String, Exit> {
    let answer = client.get("/api/v1/machines").await?;
    answer["items"]
        .as_array()
        .and_then(|items| items.iter().find(|m| m["isLocal"] == true))
        .and_then(|machine| machine["id"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            output::failure(
                client.json(),
                Exit::Usage,
                "Which machine?",
                "This server has no machine of its own: say which with --machine.",
            )
        })
}

/// The project's state in a word.
pub fn availability(project: &ProjectDto) -> &'static str {
    match project.availability {
        AvailabilityDto::Available => "available",
        AvailabilityDto::MachineOffline => "machine offline",
        AvailabilityDto::Missing => "folder missing",
    }
}

fn details(project: &ProjectDto) -> String {
    let mut lines = vec![
        format!("{:<12} {}", "Name", project.name),
        format!("{:<12} {}", "ID", project.id),
        format!(
            "{:<12} {}",
            "State",
            match project.state {
                ProjectStateDto::Active => "active",
                ProjectStateDto::Archived => "archived",
            }
        ),
    ];
    if let Some(description) = &project.description {
        lines.push(format!("{:<12} {description}", "Description"));
    }
    if let Some(group) = &project.group {
        lines.push(format!("{:<12} {group}", "Group"));
    }
    lines.push(format!("{:<12} {}", "Machine", project.machine_name));
    lines.push(format!("{:<12} {}", "Folder", project.path));
    lines.push(format!(
        "{:<12} {}",
        "Mode",
        match project.mode {
            ProjectModeDto::ReadWrite => "read-write",
            ProjectModeDto::ReadOnly => "read-only",
        }
    ));
    lines.push(format!("{:<12} {}", "Availability", availability(project)));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::cli::client::urlencode;

    #[test]
    fn encodes_names_for_paths() {
        assert_eq!(urlencode("my app"), "my%20app");
        assert_eq!(urlencode("ahab"), "ahab");
    }
}
