//! Plugins and notifications: what a plugin sends, what plugins'
//! destinations and `notifications.send` hooks get, and what a hook may
//! change.

use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::notifications::notification::model::{Level, NewNotification, Notification};
use crate::modules::notifications::push::model::target_path;
use crate::shared::error::FieldError;

/// The hook plugins declare to change or drop notifications.
pub const HOOK: &str = "notifications.send";

/// The most of a title and a body a plugin may send.
const MOST_TITLE: usize = 200;
const MOST_BODY: usize = 2000;

/// What a plugin asks to send (`ahab.notifications.send`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSend {
    /// A kind its manifest declares, without the plugin's ID.
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    /// The project it is about (its ID).
    #[serde(default)]
    pub project: Option<Uuid>,
    /// `{"type": "route", "path"}`, `{"type": "url", "url"}` or
    /// `{"type": "window", "window", "props"}`.
    #[serde(default)]
    pub target: Option<Value>,
    #[serde(default)]
    pub group_key: Option<String>,
}

/// Whether `target` is one a plugin may give: a page of Ahab, a web page,
/// or one of its windows.
fn target_problem(target: &Value) -> Option<&'static str> {
    let text = |key: &str| target.get(key).and_then(Value::as_str);
    match text("type") {
        Some("route") => match text("path") {
            Some(path) if path.starts_with('/') && !path.starts_with("//") => None,
            _ => Some("a route's path starts with one /"),
        },
        Some("url") => match text("url") {
            Some(url) if url.starts_with("https://") || url.starts_with("http://") => None,
            _ => Some("a url is http or https"),
        },
        Some("window") => text("window")
            .is_none()
            .then_some("a window target names its window"),
        _ => Some("type is route, url or window"),
    }
}

impl PluginSend {
    /// The notification `plugin` sends, if `declared` (its manifest's kinds)
    /// has this kind and the rest is fine.
    pub fn check(
        self,
        plugin: &str,
        declared: &[String],
    ) -> Result<NewNotification, Vec<FieldError>> {
        let mut problems = Vec::new();
        if !declared.contains(&self.kind) {
            problems.push(FieldError::new(
                "kind",
                format!(
                    "{} is not declared in the plugin's manifest (notifications)",
                    self.kind
                ),
            ));
        }
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > MOST_TITLE {
            problems.push(FieldError::new(
                "title",
                format!("1 to {MOST_TITLE} characters"),
            ));
        }
        if self
            .body
            .as_ref()
            .is_some_and(|body| body.chars().count() > MOST_BODY)
        {
            problems.push(FieldError::new(
                "body",
                format!("at most {MOST_BODY} characters"),
            ));
        }
        let level = match self.level.as_deref() {
            None => Level::Info,
            Some(level @ ("info" | "success" | "warning" | "error")) => Level::parse(level),
            Some(_) => {
                problems.push(FieldError::new("level", "info, success, warning or error"));
                Level::Info
            }
        };
        let target = self.target.unwrap_or_else(|| json!({ "type": "none" }));
        if target.get("type").and_then(Value::as_str) != Some("none")
            && let Some(problem) = target_problem(&target)
        {
            problems.push(FieldError::new("target", problem));
        }
        if self
            .group_key
            .as_ref()
            .is_some_and(|key| key.is_empty() || key.len() > 200)
        {
            problems.push(FieldError::new("groupKey", "1 to 200 characters"));
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(NewNotification {
            kind: format!("{plugin}.{}", self.kind),
            source: plugin.to_owned(),
            level,
            title: title.to_owned(),
            body: self.body.filter(|body| !body.trim().is_empty()),
            project_id: self.project,
            target,
            // Its own groups: a plugin cannot replace Ahab's or another's.
            group_key: self.group_key.map(|key| format!("plugin:{plugin}:{key}")),
        })
    }
}

/// A notification about to be kept, as a `notifications.send` hook sees it.
pub fn hooked(new: &NewNotification) -> Value {
    json!({
        "kind": new.kind,
        "source": new.source,
        "level": new.level.as_str(),
        "title": new.title,
        "body": new.body,
        "projectId": new.project_id,
        "target": new.target,
        "groupKey": new.group_key,
    })
}

/// `new` as a hook changed it: its title, body, level and target; never
/// its kind or source, so Settings still decide.
pub fn changed(mut new: NewNotification, data: &Value) -> NewNotification {
    if let Some(title) = data["title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    {
        new.title = title.chars().take(MOST_TITLE).collect();
    }
    match &data["body"] {
        Value::String(body) => new.body = Some(body.chars().take(MOST_BODY).collect()),
        Value::Null if data.get("body").is_some() => new.body = None,
        _ => {}
    }
    if let Some(level @ ("info" | "success" | "warning" | "error")) = data["level"].as_str() {
        new.level = Level::parse(level);
    }
    if let Some(target) = data
        .get("target")
        .filter(|target| target_problem(target).is_none())
    {
        new.target = target.clone();
    }
    new
}

/// The page of this server a notification opens; plugins' routes too.
pub fn path_of(target: &Value) -> Option<String> {
    if target.get("type").and_then(Value::as_str) == Some("route") {
        return target
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| path.starts_with('/') && !path.starts_with("//"))
            .map(str::to_owned);
    }
    target_path(target)
}

/// A kept notification, as destinations and `notification.created`
/// listeners get it: with an absolute `url` to open when the server's
/// address is known (`public_url`).
pub fn outgoing(notification: &Notification, public_url: Option<&str>) -> Value {
    let url = match notification.target.get("type").and_then(Value::as_str) {
        Some("url") => notification.target["url"].as_str().map(str::to_owned),
        _ => public_url.map(|base| {
            let path = path_of(&notification.target).unwrap_or_else(|| "/".to_owned());
            format!("{}{path}", base.trim_end_matches('/'))
        }),
    };
    json!({
        "id": notification.id,
        "kind": notification.kind,
        "source": notification.source,
        "level": notification.level.as_str(),
        "title": notification.title,
        "body": notification.body,
        "projectId": notification.project_id,
        "target": notification.target,
        "groupKey": notification.group_key,
        "createdAt": notification.created_at.format(&time::format_description::well_known::Rfc3339).ok(),
        "url": url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(value: Value) -> PluginSend {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_plugin_sends_only_what_it_declared() {
        let declared = vec!["pipeline.failed".to_owned()];
        let new = send(json!({
            "kind": "pipeline.failed", "title": "  Pipeline failed  ", "level": "error",
            "target": { "type": "url", "url": "https://gitlab.example/p/1" }, "groupKey": "main",
        }))
        .check("gitlab", &declared)
        .unwrap();
        assert_eq!(new.kind, "gitlab.pipeline.failed");
        assert_eq!(new.source, "gitlab");
        assert_eq!(new.title, "Pipeline failed");
        assert_eq!(new.level, Level::Error);
        assert_eq!(new.group_key.as_deref(), Some("plugin:gitlab:main"));

        let wrong = send(json!({ "kind": "deploy.done", "title": "", "level": "loud",
            "target": { "type": "route", "path": "//evil.example" } }))
        .check("gitlab", &declared)
        .unwrap_err();
        let fields: Vec<&str> = wrong.iter().map(|problem| problem.field.as_ref()).collect();
        assert_eq!(fields, ["kind", "title", "level", "target"]);
    }

    #[test]
    fn a_hook_changes_the_text_but_not_the_kind() {
        let new = send(json!({ "kind": "a", "title": "Hi" }))
            .check("p", &["a".to_owned()])
            .unwrap();
        let after = changed(
            new,
            &json!({ "kind": "ahab.other", "source": "x", "title": "Hello", "body": "More", "level": "warning" }),
        );
        assert_eq!(after.kind, "p.a");
        assert_eq!(after.source, "p");
        assert_eq!(
            (after.title.as_str(), after.body.as_deref()),
            ("Hello", Some("More"))
        );
        assert_eq!(after.level, Level::Warning);
    }

    #[test]
    fn what_a_destination_opens() {
        assert_eq!(
            path_of(&json!({ "type": "route", "path": "/settings/plugins" })).as_deref(),
            Some("/settings/plugins")
        );
        assert_eq!(
            path_of(&json!({ "type": "window", "window": "gitlab.pipeline" })),
            None
        );
    }
}
