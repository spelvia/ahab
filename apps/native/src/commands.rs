//! The commands of the app's built-in screens (bundled with the app): the
//! only code that pairs, starts, stops, joins or leaves. Each
//! checks that it is called from the app's own pages; the capability that
//! allows them covers only the app's own origin.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};

use crate::computer::ThisComputer;
use crate::links::{self, Link};
use crate::servers::Server;
use crate::settings::Settings;
use crate::state::{AppState, Asked, Failure, Reached, ServerOverview, Waited};
use crate::windows::{self, Target};

/// The built-in commands, which the remote capability never lists.
pub const BUILTIN: &[&str] = &[
    "boot",
    "reach",
    "pair",
    "ask",
    "wait",
    "check_server",
    "open_server",
    "open_server_window",
    "show_screen",
    "remove_server",
    "rename_server",
    "this_computer",
    "run_here",
    "use_existing",
    "start_here",
    "stop_here",
    "join_server",
    "join_with_code",
    "leave",
    "parse_link",
    "take_link",
    "get_settings",
    "set_settings",
    "check_for_updates",
    "quit",
];

/// `ahab://` links that arrived before a screen could take them.
#[derive(Default)]
pub struct PendingLinks(pub Mutex<Vec<String>>);

fn builtin(webview: &Webview) -> Result<(), Failure> {
    let page = webview
        .url()
        .map_err(|error| Failure::new("not_allowed", error.to_string()))?;
    if windows::is_builtin(&page, &windows::builtin_origins(webview.app_handle())) {
        Ok(())
    } else {
        Err(Failure::new(
            "not_allowed",
            "Only the app's own screens can do this.",
        ))
    }
}

fn state(webview: &Webview) -> Arc<AppState> {
    webview.state::<Arc<AppState>>().inner().clone()
}

/// What the start screen needs to decide where to go.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Boot {
    pub servers: ServerOverview,
    pub computer: ThisComputer,
    pub settings: Settings,
    pub version: &'static str,
}

#[tauri::command]
pub async fn boot(webview: Webview) -> Result<Boot, Failure> {
    builtin(&webview)?;
    let state = state(&webview);
    Ok(Boot {
        servers: state.overview()?,
        computer: state.this_computer().await,
        settings: state.files.settings(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
pub async fn reach(webview: Webview, address: String) -> Result<Reached, Failure> {
    builtin(&webview)?;
    state(&webview).reach(&address).await
}

#[tauri::command]
pub async fn pair(
    webview: Webview,
    url: String,
    secret: Option<String>,
    code: Option<String>,
) -> Result<Server, Failure> {
    builtin(&webview)?;
    let server = state(&webview)
        .pair(&url, secret.as_deref(), code.as_deref())
        .await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(server)
}

#[tauri::command]
pub async fn ask(webview: Webview, url: String) -> Result<Asked, Failure> {
    builtin(&webview)?;
    state(&webview).ask(&url).await
}

#[tauri::command]
pub async fn wait(webview: Webview, id: String) -> Result<Waited, Failure> {
    builtin(&webview)?;
    let waited = state(&webview).wait(&id).await?;
    if matches!(waited, Waited::Paired { .. }) {
        crate::comfort::refresh_tray(webview.app_handle());
    }
    Ok(waited)
}

/// Whether a server answers and still knows this app, before its web
/// app is shown.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub reachable: bool,
    pub paired: bool,
    pub message: Option<String>,
}

#[tauri::command]
pub async fn check_server(webview: Webview, id: String) -> Result<Checked, Failure> {
    builtin(&webview)?;
    Ok(match state(&webview).still_paired(&id).await {
        Ok(paired) => Checked {
            reachable: true,
            paired,
            message: None,
        },
        Err(failure) if failure.code == "not_found" => return Err(failure),
        Err(failure) => Checked {
            reachable: false,
            paired: false,
            message: Some(failure.message),
        },
    })
}

/// Shows a server's web app in this window.
#[tauri::command]
pub fn open_server(webview: Webview, id: String, path: Option<String>) -> Result<(), Failure> {
    builtin(&webview)?;
    windows::show_server(webview.app_handle(), webview.label(), &id, path.as_deref())
}

#[tauri::command]
pub fn open_server_window(webview: Webview, id: String) -> Result<(), Failure> {
    builtin(&webview)?;
    windows::open(
        webview.app_handle(),
        Target::Server { id, path: None },
        None,
    )
    .map(|_| ())
}

/// Opens a built-in screen in a window of its own (App settings from the
/// servers screen).
#[tauri::command]
pub fn show_screen(webview: Webview, route: String) -> Result<(), Failure> {
    builtin(&webview)?;
    windows::show_screen(webview.app_handle(), webview.label(), &route)
}

#[tauri::command]
pub async fn remove_server(webview: Webview, id: String) -> Result<Server, Failure> {
    builtin(&webview)?;
    let removed = state(&webview).remove(&id).await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(removed)
}

#[tauri::command]
pub fn rename_server(webview: Webview, id: String, name: String) -> Result<(), Failure> {
    builtin(&webview)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Failure::new("validation_failed", "A name is needed."));
    }
    state(&webview).servers.rename(&id, name)?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(())
}

#[tauri::command]
pub async fn this_computer(webview: Webview) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    Ok(state(&webview).this_computer().await)
}

#[tauri::command]
pub async fn run_here(webview: Webview) -> Result<Server, Failure> {
    builtin(&webview)?;
    let server = state(&webview).run_here().await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(server)
}

/// Opens an http(s) address in the system browser: the server's approval
/// page while pairing ("Open approval in my browser").
#[tauri::command]
pub fn open_in_browser(webview: Webview, url: String) -> Result<(), Failure> {
    builtin(&webview)?;
    let url = url::Url::parse(&url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .ok_or_else(|| Failure::new("bad_address", "This is not a web address."))?;
    windows::open_external(webview.app_handle(), &url);
    Ok(())
}

/// What the first launch's checklist shows once Ahab runs here: this
/// computer's machine and the agents it found.
#[tauri::command]
pub async fn agents_here(
    webview: Webview,
    id: String,
) -> Result<crate::server_api::LocalAgents, Failure> {
    builtin(&webview)?;
    state(&webview).local_agents(&id).await
}

#[tauri::command]
pub async fn use_existing(webview: Webview) -> Result<Server, Failure> {
    builtin(&webview)?;
    let server = state(&webview).use_existing().await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(server)
}

#[tauri::command]
pub async fn start_here(webview: Webview) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    let computer = state(&webview).start_here().await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(computer)
}

#[tauri::command]
pub async fn stop_here(webview: Webview) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    let computer = state(&webview).stop_here().await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(computer)
}

#[tauri::command]
pub async fn join_server(webview: Webview, id: String) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    let computer = state(&webview).join_server(&id).await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(computer)
}

#[tauri::command]
pub async fn join_with_code(
    webview: Webview,
    server: String,
    code: String,
    fingerprint: Option<String>,
) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    let computer = state(&webview)
        .join_with_code(&server, &code, fingerprint.as_deref())
        .await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(computer)
}

#[tauri::command]
pub async fn leave(webview: Webview) -> Result<ThisComputer, Failure> {
    builtin(&webview)?;
    let computer = state(&webview).leave().await?;
    crate::comfort::refresh_tray(webview.app_handle());
    Ok(computer)
}

/// Reads a link the person pasted or opened; nothing happens until they
/// confirm on the screen it leads to.
#[tauri::command]
pub fn parse_link(webview: Webview, link: String) -> Result<Link, Failure> {
    builtin(&webview)?;
    links::parse(&link).map_err(|error| Failure::new("bad_link", error.to_string()))
}

#[tauri::command]
pub fn take_link(webview: Webview) -> Result<Option<String>, Failure> {
    builtin(&webview)?;
    let pending = webview.state::<PendingLinks>();
    let mut links = pending
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(if links.is_empty() {
        None
    } else {
        Some(links.remove(0))
    })
}

#[tauri::command]
pub fn get_settings(webview: Webview) -> Result<Settings, Failure> {
    builtin(&webview)?;
    Ok(state(&webview).files.settings())
}

#[tauri::command]
pub fn set_settings(webview: Webview, settings: Settings) -> Result<Settings, Failure> {
    builtin(&webview)?;
    let app = webview.app_handle();
    crate::comfort::apply_shortcut(app, settings.global_shortcut.as_deref())?;
    state(&webview).files.save_settings(&settings)?;
    Ok(settings)
}

/// Where updates stand: nothing to check until releases are
/// published.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Updates {
    pub enabled: bool,
    pub available: bool,
    pub note: &'static str,
}

#[tauri::command]
pub fn check_for_updates(webview: Webview) -> Result<Updates, Failure> {
    builtin(&webview)?;
    Ok(Updates {
        enabled: state(&webview).files.settings().check_for_updates,
        available: false,
        note: "Ahab's releases are not published anywhere yet, so there is nothing to check. \
               To update, install a newer package.",
    })
}

#[tauri::command]
pub fn quit(webview: Webview) -> Result<(), Failure> {
    builtin(&webview)?;
    quit_app(webview.app_handle());
    Ok(())
}

/// Quits the app; Ahab's service, if it runs here, keeps running.
pub fn quit_app(app: &AppHandle) {
    let shell = app.state::<Arc<windows::Shell>>();
    shell
        .quitting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}
