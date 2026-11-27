//! The native bridge: the few functions the paired server's web
//! app may call, and nothing else. Each is allowed to the server's origin
//! by a capability of its own (added when the server is shown) and checks
//! again that it is called from the server the window shows: a page of
//! another origin, or a frame, gets nothing. Starting, stopping, joining
//! and leaving are not here: only the app's built-in screens do that.
//!
//! Plugin web entries are modules imported into the web app's
//! page, not frames, so they run with its origin: the list is kept to what
//! is harmless for any page of the server to call.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::ipc::CapabilityBuilder;
use tauri::{Manager, Runtime, Webview};
use url::Url;

use crate::computer::ThisComputer;
use crate::servers::Server;
use crate::state::{AppState, Failure};
use crate::windows::{self, Shell, Target};

/// The bridge's commands, as the capability allows them.
pub const COMMANDS: &[&str] = &[
    "bridge_info",
    "bridge_notify",
    "bridge_badge",
    "bridge_open_external",
    "bridge_servers",
    "bridge_switch_server",
    "bridge_add_server",
    "bridge_new_window",
    "bridge_this_computer",
    "bridge_background",
];

/// The capability that lets `origin` call the bridge.
pub fn capability(origin: &str) -> CapabilityBuilder {
    let mut builder = CapabilityBuilder::new(format!("server-{origin}"))
        .remote(format!("{origin}/*"))
        .window("*");
    for permission in permissions() {
        builder = builder.permission(permission);
    }
    builder
}

/// The permissions a server's origin gets: the bridge's commands, nothing
/// of Tauri's core or its plugins.
pub fn permissions() -> Vec<String> {
    COMMANDS
        .iter()
        .map(|command| format!("allow-{}", command.replace('_', "-")))
        .collect()
}

/// Whether a page at `page` may use the bridge in a window showing
/// `server`: only the server's own origin.
pub fn allowed(page: &Url, server: Option<&Server>) -> bool {
    let Some(server) = server else {
        return false;
    };
    matches!(page.scheme(), "http" | "https") && page.origin().ascii_serialization() == server.url
}

fn caller<R: Runtime>(webview: &Webview<R>) -> Result<Server, Failure> {
    let shell = webview.state::<Arc<Shell>>();
    let server = shell.server_of(webview.label());
    let page = webview
        .url()
        .map_err(|error| Failure::new("not_allowed", error.to_string()))?;
    if allowed(&page, server.as_ref()) {
        server.ok_or_else(|| Failure::new("not_allowed", "No server here."))
    } else {
        Err(Failure::new(
            "not_allowed",
            "Only the server this window shows can use the app.",
        ))
    }
}

/// What `app.info()` answers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub app: bool,
    pub version: &'static str,
    pub platform: &'static str,
    /// The server this window shows, as the app knows it.
    pub server: Server,
    /// The other functions the bridge has, for the web app to check.
    pub functions: Vec<&'static str>,
}

#[tauri::command]
pub fn bridge_info(webview: Webview) -> Result<Info, Failure> {
    let server = caller(&webview)?;
    Ok(Info {
        app: true,
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        server,
        functions: COMMANDS
            .iter()
            .map(|command| command.trim_start_matches("bridge_"))
            .collect(),
    })
}

/// A notification from the web app: what to show and where clicking it
/// leads (a path of its server).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
}

#[tauri::command]
pub fn bridge_notify(webview: Webview, notification: Notification) -> Result<(), Failure> {
    let server = caller(&webview)?;
    crate::comfort::notify(webview.app_handle(), &server, webview.label(), notification);
    Ok(())
}

#[tauri::command]
pub fn bridge_badge(webview: Webview, count: u32) -> Result<(), Failure> {
    caller(&webview)?;
    crate::comfort::badge(webview.app_handle(), webview.label(), count);
    Ok(())
}

#[tauri::command]
pub fn bridge_open_external(webview: Webview, url: String) -> Result<(), Failure> {
    caller(&webview)?;
    let url = Url::parse(&url).map_err(|_| Failure::new("bad_address", "Not an address."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Failure::new(
            "bad_address",
            "Only http and https addresses open.",
        ));
    }
    windows::open_external(webview.app_handle(), &url);
    Ok(())
}

/// A server, as the web app's switcher shows it (no credential).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub url: String,
    pub local: bool,
    pub current: bool,
}

#[tauri::command]
pub fn bridge_servers(webview: Webview) -> Result<Vec<Listed>, Failure> {
    let current = caller(&webview)?;
    let state = webview.state::<Arc<AppState>>();
    Ok(state
        .servers
        .load()?
        .servers
        .into_iter()
        .map(|server| Listed {
            current: server.id == current.id,
            id: server.id,
            name: server.name,
            url: server.url,
            local: server.local,
        })
        .collect())
}

#[tauri::command]
pub fn bridge_switch_server(webview: Webview, id: String) -> Result<(), Failure> {
    caller(&webview)?;
    windows::show_server(webview.app_handle(), webview.label(), &id, None)
}

/// Opens the built-in Add server screen; pairing happens there.
#[tauri::command]
pub fn bridge_add_server(webview: Webview) -> Result<(), Failure> {
    caller(&webview)?;
    windows::show_screen(webview.app_handle(), webview.label(), "/connect")
}

#[tauri::command]
pub fn bridge_new_window(webview: Webview, path: Option<String>) -> Result<(), Failure> {
    let server = caller(&webview)?;
    let path = path.as_deref().and_then(crate::links::safe_path);
    windows::open(
        webview.app_handle(),
        Target::Server {
            id: server.id,
            path: Some(path.unwrap_or_else(|| "/".to_owned())),
        },
        None,
    )
    .map(|_| ())
}

#[tauri::command]
pub async fn bridge_this_computer(webview: Webview) -> Result<ThisComputer, Failure> {
    caller(&webview)?;
    let state = webview.state::<Arc<AppState>>().inner().clone();
    Ok(state.this_computer().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(url: &str) -> Server {
        Server {
            id: "s1".into(),
            name: "atlas".into(),
            url: url.into(),
            local: false,
            client_id: None,
            last_path: None,
        }
    }

    #[test]
    fn only_the_windows_own_server_reaches_the_bridge() {
        let atlas = server("https://atlas.lan");
        let page = |text: &str| Url::parse(text).unwrap();
        assert!(allowed(&page("https://atlas.lan/p/1"), Some(&atlas)));
        assert!(!allowed(&page("https://evil.example/"), Some(&atlas)));
        assert!(!allowed(&page("https://atlas.lan:444/"), Some(&atlas)));
        assert!(!allowed(&page("http://atlas.lan/"), Some(&atlas)));
        // The built-in screens' window shows no server: its pages use the
        // built-in commands instead.
        assert!(!allowed(&page("tauri://localhost/index.html"), None));
        assert!(!allowed(&page("https://atlas.lan/"), None));
        // A sandboxed frame has an opaque origin.
        assert!(!allowed(
            &page("data:text/html,<script></script>"),
            Some(&atlas)
        ));
    }

    #[test]
    fn the_capability_lists_only_the_bridge() {
        let permissions = permissions();
        assert_eq!(permissions.len(), COMMANDS.len());
        assert!(permissions.contains(&"allow-bridge-info".to_owned()));
        for permission in &permissions {
            assert!(permission.starts_with("allow-bridge-"), "{permission}");
        }
        for builtin in crate::commands::BUILTIN {
            let builtin = format!("allow-{}", builtin.replace('_', "-"));
            assert!(!permissions.contains(&builtin), "{builtin}");
        }
    }
}

/// The phone's background connection: its state, the permission,
/// how many agents work, staying connected, battery settings.
#[tauri::command]
pub fn bridge_background(
    webview: Webview,
    ask: crate::background::Ask,
) -> Result<crate::background::Status, Failure> {
    let server = caller(&webview)?;
    let credential = webview
        .state::<Arc<AppState>>()
        .servers
        .credential(&server.id)
        .ok()
        .flatten();
    crate::background::ask(webview.app_handle(), &server, credential, ask)
}
