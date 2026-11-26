//! `ahab sessions …`: a project's sessions from the command line, in the
//! shape of `ahab projects`: list and show them, read a transcript, start
//! one, write to it, stop it, rename, archive, restore and remove it.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Value, json};
use time::macros::format_description;

use crate::cli::client::{Client, urlencode};
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::cli::prompt;
use crate::modules::sessions::session::api::{
    SessionDto, SessionKindDto, SessionListDto, SessionStateDto,
};
use crate::modules::sessions::terminal::api::TerminalStateDto;
use crate::shared::time::display;

/// `ahab sessions` alone lists every project's sessions; `ahab sessions
/// <project>` one project's.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,
    /// The project's name or ID; every project by default.
    pub project: Option<String>,
    /// List archived sessions instead.
    #[arg(long)]
    pub archived: bool,
}

#[derive(Debug, Subcommand)]
pub enum SessionsCommand {
    /// Sessions, most recent first: one project's, or every project's.
    #[command(visible_alias = "ls")]
    List {
        /// The project's name or ID; every project by default.
        project: Option<String>,
        /// List archived sessions instead.
        #[arg(long)]
        archived: bool,
    },
    /// A session: its project, agent and state.
    Show {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
    },
    /// What was said in a session: your messages and the agent's answers.
    #[command(visible_alias = "log")]
    Transcript {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
    },
    /// Start a session in a project, with its first message.
    Start {
        /// The project's name or ID.
        project: String,
        /// The first message.
        message: String,
        /// `claude-code`, `codex` or `pi`.
        #[arg(long, short = 'a', default_value = "claude-code")]
        agent: String,
        /// The model; the agent's default otherwise.
        #[arg(long)]
        model: Option<String>,
        /// The reasoning effort.
        #[arg(long)]
        effort: Option<String>,
        /// What the agent may do without asking: Claude Code `plan`, `ask`,
        /// `acceptEdits` or `bypass`; Codex `readOnly`, `ask`, `workspace` or
        /// `fullAccess`.
        #[arg(long)]
        mode: Option<String>,
        /// The title; the first message, shortened, otherwise.
        #[arg(long)]
        title: Option<String>,
    },
    /// Add a chat or a terminal to a session, beside what it holds.
    Add {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
        /// The new chat's first message (not for `--terminal`).
        message: Option<String>,
        /// Add a terminal instead of a chat.
        #[arg(long, conflicts_with_all = ["agent", "model", "effort", "mode"])]
        terminal: bool,
        /// The terminal's command; the login shell otherwise.
        #[arg(long, requires = "terminal")]
        command: Option<String>,
        /// `claude-code`, `codex` or `pi`.
        #[arg(long, short = 'a')]
        agent: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        /// The terminal's name in the session.
        #[arg(long)]
        title: Option<String>,
    },
    /// Write to a session; an ended one continues.
    Send {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
        message: String,
    },
    /// Stop the agent's current turn.
    Stop {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
    },
    /// Give a session another title.
    Rename {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
        title: String,
    },
    /// Archive a session: it stops, and leaves the list; everything is kept.
    Archive {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
    },
    /// Bring an archived session back.
    Restore {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
    },
    /// Delete a session and its history.
    #[command(visible_alias = "rm")]
    Remove {
        /// The session's ID, or its end as `ahab sessions` shows it.
        session: String,
        /// Do not ask first.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

pub async fn run(client: &Client, args: SessionsArgs) -> Result<(), Exit> {
    let command = args.command.unwrap_or(SessionsCommand::List {
        project: args.project,
        archived: args.archived,
    });
    match command {
        SessionsCommand::List { project, archived } => {
            let projects = match project {
                Some(project) => vec![project],
                None => project_names(client).await?,
            };
            let mut items = Vec::new();
            for project in &projects {
                let answer = client
                    .get(&format!("/api/v1/projects/{}/sessions", urlencode(project)))
                    .await?;
                let list: SessionListDto = client.parse(answer)?;
                items.extend(
                    list.items
                        .into_iter()
                        .filter(|session| session.archived_at.is_some() == archived)
                        .map(|session| (project.clone(), session)),
                );
            }
            items.sort_by_key(|(_, session)| std::cmp::Reverse(session.updated_at));
            let value = json!({ "items": items.iter().map(|(_, s)| s).collect::<Vec<_>>() });
            output::print(client.json(), &value, || table(&items, archived));
            Ok(())
        }
        SessionsCommand::Show { session } => {
            let (answer, session) = find(client, &session).await?;
            output::print(client.json(), &answer, || details(&session));
            Ok(())
        }
        SessionsCommand::Transcript { session } => {
            let (_, session) = find(client, &session).await?;
            let mut said = Vec::new();
            for chat in session.chats.iter().rev() {
                let mut after = 0;
                loop {
                    let answer = client
                        .get(&format!("/api/v1/chats/{}/events?after={after}", chat.id))
                        .await?;
                    let items = answer["items"].as_array().cloned().unwrap_or_default();
                    for item in &items {
                        after = item["seq"].as_i64().unwrap_or(after);
                        if let Some(line) = spoken(&item["event"], &chat.agent) {
                            said.push(line);
                        }
                    }
                    if !answer["more"].as_bool().unwrap_or(false) || items.is_empty() {
                        break;
                    }
                }
            }
            let value = json!({ "items": said.iter().map(|(who, text)| json!({ "from": who, "text": text })).collect::<Vec<_>>() });
            output::print(client.json(), &value, || {
                if said.is_empty() {
                    return "Nothing was said yet.".to_owned();
                }
                said.iter()
                    .map(|(who, text)| format!("{who}:\n{}\n", text.trim_end()))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(())
        }
        SessionsCommand::Start {
            project,
            message,
            agent,
            model,
            effort,
            mode,
            title,
        } => {
            let project = project_id(client, &project).await?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/projects/{project}/sessions"),
                    Some(json!({
                        "title": title,
                        "chat": {
                            "agent": agent,
                            "message": message,
                            "model": model,
                            "effort": effort,
                            "mode": mode,
                        },
                    })),
                )
                .await?;
            let session: SessionDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!(
                    "Started {} ({}). Follow it with `ahab sessions transcript {}`.",
                    session.title,
                    short(&session),
                    short(&session)
                )
            });
            Ok(())
        }
        SessionsCommand::Add {
            session,
            message,
            terminal,
            command,
            agent,
            model,
            effort,
            mode,
            title,
        } => {
            let (_, found) = find(client, &session).await?;
            let answer = if terminal {
                client
                    .send(
                        Method::POST,
                        &format!("/api/v1/sessions/{}/terminals", found.id),
                        Some(json!({ "command": command, "title": title })),
                    )
                    .await?
            } else {
                let Some(message) = message else {
                    return Err(output::failure(
                        client.json(),
                        Exit::Usage,
                        "No message",
                        "Write the new chat's first message, or add `--terminal`.",
                    ));
                };
                client
                    .send(
                        Method::POST,
                        &format!("/api/v1/sessions/{}/chats", found.id),
                        Some(json!({
                            "agent": agent.unwrap_or_else(|| "claude-code".to_owned()),
                            "message": message,
                            "model": model,
                            "effort": effort,
                            "mode": mode,
                        })),
                    )
                    .await?
            };
            output::print(client.json(), &answer, || {
                format!(
                    "Added {} to {}.",
                    if terminal { "a terminal" } else { "a chat" },
                    found.title
                )
            });
            Ok(())
        }
        SessionsCommand::Send { session, message } => {
            let (_, found) = find(client, &session).await?;
            let chat = chat_of(client, &found)?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/chats/{chat}/messages"),
                    Some(json!({ "text": message })),
                )
                .await?;
            output::print(client.json(), &answer, || {
                format!("Sent to {}.", found.title)
            });
            Ok(())
        }
        SessionsCommand::Stop { session } => {
            let (_, found) = find(client, &session).await?;
            let chat = chat_of(client, &found)?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/chats/{chat}/interrupt"),
                    None,
                )
                .await?;
            output::print(client.json(), &answer, || {
                format!("Stopped the agent in {}.", found.title)
            });
            Ok(())
        }
        SessionsCommand::Rename { session, title } => {
            let (_, found) = find(client, &session).await?;
            let answer = client
                .send(
                    Method::PATCH,
                    &format!("/api/v1/sessions/{}", found.id),
                    Some(json!({ "title": title })),
                )
                .await?;
            let renamed: SessionDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || {
                format!("Renamed to {}.", renamed.title)
            });
            Ok(())
        }
        SessionsCommand::Archive { session } => {
            let (_, found) = find(client, &session).await?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/sessions/{}/archive", found.id),
                    None,
                )
                .await?;
            output::print(client.json(), &answer, || {
                format!("Archived {}.", found.title)
            });
            Ok(())
        }
        SessionsCommand::Restore { session } => {
            let (_, found) = find(client, &session).await?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/sessions/{}/unarchive", found.id),
                    None,
                )
                .await?;
            output::print(client.json(), &answer, || {
                format!("Restored {}.", found.title)
            });
            Ok(())
        }
        SessionsCommand::Remove { session, yes } => {
            let (_, found) = find(client, &session).await?;
            if !yes {
                let question = format!("Delete {} and its history?", found.title);
                if !prompt::confirm(client.json(), &question, false, "--yes")? {
                    return Err(output::failure(
                        client.json(),
                        Exit::Other,
                        "Not deleted",
                        &format!("{} stays.", found.title),
                    ));
                }
            }
            // Only archived sessions are deleted: archiving stops it first.
            if found.archived_at.is_none() {
                client
                    .send(
                        Method::POST,
                        &format!("/api/v1/sessions/{}/archive", found.id),
                        None,
                    )
                    .await?;
            }
            client
                .send(
                    Method::DELETE,
                    &format!("/api/v1/sessions/{}", found.id),
                    None,
                )
                .await?;
            output::print(client.json(), &json!({ "removed": found.id }), || {
                format!("Deleted {}.", found.title)
            });
            Ok(())
        }
    }
}

/// The active projects' names.
async fn project_names(client: &Client) -> Result<Vec<String>, Exit> {
    let answer = client.get("/api/v1/projects").await?;
    Ok(answer["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|project| project["name"].as_str().map(str::to_owned))
        .collect())
}

/// A session by its ID, or the start or end of an ID (as `ahab sessions`
/// shows it) unique among every project's sessions.
async fn find(client: &Client, reference: &str) -> Result<(Value, SessionDto), Exit> {
    if uuid::Uuid::parse_str(reference).is_ok() {
        let answer = client.get(&format!("/api/v1/sessions/{reference}")).await?;
        let session = client.parse(answer.clone())?;
        return Ok((answer, session));
    }
    let wanted = reference.to_lowercase();
    let mut found = Vec::new();
    for project in project_names(client).await? {
        let answer = client
            .get(&format!(
                "/api/v1/projects/{}/sessions",
                urlencode(&project)
            ))
            .await?;
        let list: SessionListDto = client.parse(answer)?;
        found.extend(list.items.into_iter().filter(|session| {
            let id = session.id.to_string();
            id.starts_with(&wanted) || id.ends_with(&wanted)
        }));
    }
    match found.len() {
        1 => {
            let session = found.remove(0);
            let answer = serde_json::to_value(&session).unwrap_or(Value::Null);
            Ok((answer, session))
        }
        0 => Err(output::failure(
            client.json(),
            Exit::Other,
            "No such session",
            &format!(
                "No session's ID starts or ends with {reference}; `ahab sessions` lists them."
            ),
        )),
        n => Err(output::failure(
            client.json(),
            Exit::Usage,
            "Several sessions match",
            &format!("{n} sessions' IDs match {reference}; give more of it."),
        )),
    }
}

/// The ID of the project a new session starts in.
async fn project_id(client: &Client, project: &str) -> Result<String, Exit> {
    let answer = client
        .get(&format!("/api/v1/projects/{}", urlencode(project)))
        .await?;
    Ok(answer["id"].as_str().unwrap_or_default().to_owned())
}

/// The session's chat that messages go to: its most recent.
fn chat_of(client: &Client, session: &SessionDto) -> Result<uuid::Uuid, Exit> {
    session.chats.first().map(|chat| chat.id).ok_or_else(|| {
        output::failure(
            client.json(),
            Exit::Other,
            "No conversation",
            &format!("{} has no chat to write to.", session.title),
        )
    })
}

/// A finished message in a chat's events: who said it, and what.
fn spoken(event: &Value, agent: &str) -> Option<(String, String)> {
    if event["type"] != "itemEnd" {
        return None;
    }
    let item = &event["item"];
    match item["type"].as_str()? {
        "userMessage" => {
            let text: String = item["content"]
                .as_array()?
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            Some(("You".to_owned(), text))
        }
        "agentMessage" => Some((
            crate::modules::machines::agent_label(agent).to_owned(),
            item["text"].as_str()?.to_owned(),
        )),
        _ => None,
    }
}

/// The last part of a session's ID: enough to name it here. (IDs are
/// UUID v7: their start is the time, shared by sessions started together;
/// their end is random.)
fn short(session: &SessionDto) -> String {
    let id = session.id.to_string();
    id[id.len() - 8..].to_owned()
}

fn state(session: &SessionDto) -> String {
    if let Some(terminal) = &session.terminal {
        return match (terminal.state, terminal.exit_code) {
            (TerminalStateDto::Running, _) => "running".to_owned(),
            (TerminalStateDto::Exited, Some(code)) => format!("exited {code}"),
            (TerminalStateDto::Exited, None) => "exited".to_owned(),
        };
    }
    match session.state {
        SessionStateDto::NeedsYou => "needs you",
        SessionStateDto::Working => "working",
        SessionStateDto::Idle => "idle",
        SessionStateDto::Ended => "ended",
    }
    .to_owned()
}

/// What runs in a session: its agent, or "Terminal".
fn runs(session: &SessionDto) -> String {
    if session.kind == SessionKindDto::Terminal {
        return "Terminal".to_owned();
    }
    session
        .chats
        .first()
        .map(|chat| crate::modules::machines::agent_label(&chat.agent).to_owned())
        .unwrap_or_default()
}

fn table(items: &[(String, SessionDto)], archived: bool) -> String {
    if items.is_empty() {
        return if archived {
            "No archived sessions.".to_owned()
        } else {
            "No sessions yet. Start one with `ahab sessions start <project> \"…\"`.".to_owned()
        };
    }
    let mut rows = vec![
        ["ID", "State", "Runs", "Project", "Title", "Updated"]
            .map(str::to_owned)
            .to_vec(),
    ];
    for (project, session) in items {
        rows.push(vec![
            short(session),
            state(session),
            runs(session),
            project.clone(),
            session.title.clone(),
            display(
                session.updated_at,
                format_description!("[year]-[month]-[day] [hour]:[minute]"),
            ),
        ]);
    }
    output::table(&rows)
}

fn details(session: &SessionDto) -> String {
    let mut lines = vec![
        format!("{}  ({})", session.title, session.id),
        format!("State      {}", state(session)),
    ];
    if session.archived_at.is_some() {
        lines.push("Archived   yes".to_owned());
    }
    // Oldest first, as the session's tabs show them.
    for chat in session.chats.iter().rev() {
        let mut line = format!(
            "Chat       {} · {}",
            crate::modules::machines::agent_label(&chat.agent),
            chat.title
        );
        if let Some(model) = &chat.model {
            line.push_str(&format!(" · {model}"));
        }
        if let Some(mode) = &chat.mode {
            line.push_str(&format!(" · {mode}"));
        }
        lines.push(line);
    }
    for terminal in &session.terminals {
        let name = terminal
            .label
            .as_deref()
            .or(terminal.title.as_deref())
            .unwrap_or("Terminal");
        lines.push(format!(
            "Terminal   {name} · {} · {}",
            terminal.command.as_deref().unwrap_or("the login shell"),
            match terminal.state {
                TerminalStateDto::Running => "running",
                TerminalStateDto::Exited => "exited",
            }
        ));
    }
    lines.push(format!(
        "Started    {}",
        display(
            session.created_at,
            format_description!("[year]-[month]-[day] [hour]:[minute]")
        )
    ));
    lines.push(format!(
        "Updated    {}",
        display(
            session.updated_at,
            format_description!("[year]-[month]-[day] [hour]:[minute]")
        )
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::spoken;

    #[test]
    fn a_transcript_holds_finished_messages_only() {
        let user = json!({ "type": "itemEnd", "item": { "type": "userMessage", "content": [{ "type": "text", "text": "Hello" }] } });
        let agent =
            json!({ "type": "itemEnd", "item": { "type": "agentMessage", "text": "Hi there" } });
        let delta =
            json!({ "type": "itemDelta", "item": { "type": "agentMessage", "text": "Hi" } });
        assert_eq!(
            spoken(&user, "codex"),
            Some(("You".to_owned(), "Hello".to_owned()))
        );
        assert_eq!(
            spoken(&agent, "claude-code"),
            Some(("Claude Code".to_owned(), "Hi there".to_owned()))
        );
        assert_eq!(spoken(&delta, "codex"), None);
    }
}
