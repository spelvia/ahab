//! `ahab notifications …`: the inbox, and marking notifications read, through the API.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::json;
use time::macros::format_description;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::output;
use crate::modules::notifications::notification::api::{
    CountDto, NotificationDto, NotificationListDto,
};
use crate::shared::time::display;

/// `ahab notifications` alone lists the inbox.
#[derive(Debug, Args)]
pub struct NotificationsArgs {
    #[command(subcommand)]
    pub command: Option<NotificationsCommand>,
    /// Only unread ones.
    #[arg(long)]
    pub unread: bool,
}

#[derive(Debug, Subcommand)]
pub enum NotificationsCommand {
    /// Mark a notification read, or every one with --all.
    Read {
        /// The notification's ID, or a unique start of it.
        id: Option<String>,
        /// Every unread notification.
        #[arg(long, conflicts_with = "id")]
        all: bool,
    },
}

pub async fn run(client: &Client, args: NotificationsArgs) -> Result<(), Exit> {
    match args.command {
        None => {
            let path = if args.unread {
                "/api/v1/notifications?unread=true"
            } else {
                "/api/v1/notifications"
            };
            let answer = client.get(path).await?;
            let inbox: NotificationListDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || table(&inbox));
            Ok(())
        }
        Some(NotificationsCommand::Read { all: true, .. }) => {
            let answer = client
                .send(
                    Method::POST,
                    "/api/v1/notifications/read-all",
                    Some(json!({})),
                )
                .await?;
            let marked: CountDto = client.parse(answer.clone())?;
            output::print(client.json(), &answer, || match marked.count {
                0 => "Nothing was unread.".to_owned(),
                1 => "Marked 1 notification read.".to_owned(),
                count => format!("Marked {count} notifications read."),
            });
            Ok(())
        }
        Some(NotificationsCommand::Read { id: Some(id), .. }) => {
            let found = find(client, &id).await?;
            let answer = client
                .send(
                    Method::POST,
                    &format!("/api/v1/notifications/{}/read", found.id),
                    Some(json!({})),
                )
                .await?;
            output::print(client.json(), &answer, || format!("Read: {}.", found.title));
            Ok(())
        }
        Some(NotificationsCommand::Read {
            id: None,
            all: false,
        }) => Err(output::failure(
            client.json(),
            Exit::Usage,
            "Which notification",
            "Name a notification, or use --all.",
        )),
    }
}

/// The notification `id` names: its ID, or a unique start of it.
async fn find(client: &Client, id: &str) -> Result<NotificationDto, Exit> {
    let inbox: NotificationListDto =
        client.parse(client.get("/api/v1/notifications?limit=200").await?)?;
    let found: Vec<NotificationDto> = inbox
        .items
        .into_iter()
        .filter(|item| item.id.to_string().starts_with(id))
        .collect();
    match found.len() {
        1 => found.into_iter().next().ok_or(Exit::Other),
        0 => Err(output::failure(
            client.json(),
            Exit::Usage,
            "Not found",
            &format!("No notification starts with {id}."),
        )),
        _ => Err(output::failure(
            client.json(),
            Exit::Usage,
            "Which notification",
            &format!("More than one notification starts with {id}; give more of it."),
        )),
    }
}

fn table(inbox: &NotificationListDto) -> String {
    if inbox.items.is_empty() {
        return "No notifications.".to_owned();
    }
    let mut rows = vec![vec![
        String::new(),
        "WHEN".to_owned(),
        "TITLE".to_owned(),
        "DETAIL".to_owned(),
        "ID".to_owned(),
    ]];
    for item in &inbox.items {
        rows.push(vec![
            if item.read_at.is_none() {
                "●".to_owned()
            } else {
                String::new()
            },
            display(
                item.created_at,
                format_description!("[year]-[month]-[day] [hour]:[minute]"),
            ),
            item.title.clone(),
            item.body.clone().unwrap_or_default(),
            item.id.to_string(),
        ]);
    }
    format!("{}\n{} unread", output::table(&rows), inbox.unread)
}
