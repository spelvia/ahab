//! The app's windows: each shows the built-in screens or one
//! server's web app. Links within the server stay in the window, others
//! open in the system browser, `ahab://` links go to the built-in screens.
//! A window opens onto the splash, with its background set natively, so
//! nothing flashes white.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tauri::webview::{Cookie, NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
#[cfg(desktop)]
use tauri_plugin_window_state::{StateFlags, WindowExt};
use url::Url;

use crate::links::safe_path;
use crate::servers::Server;
use crate::settings::WindowRecord;
use crate::state::{AppState, Failure};

/// The splash's background, as the web app's (`--background`).
pub const DARK: Color = Color(0x25, 0x25, 0x26, 0xff);
pub const LIGHT: Color = Color(0xf4, 0xf2, 0xea, 0xff);

/// The name of the cookie the web app's server reads.
const COOKIE: &str = "ahab_device";

/// What happens to a navigation in a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Navigation {
    /// Within the window's server, or the app's own screens.
    Allow,
    /// Elsewhere on the web: the system browser.
    External,
    /// An `ahab://` link: the built-in screens read it.
    Link,
    Deny,
}

/// Decides a navigation from the window showing `server` (an origin).
pub fn decide(url: &Url, server: Option<&str>, builtin: &[String]) -> Navigation {
    let origin = url.origin().ascii_serialization();
    if builtin.contains(&origin) || url.scheme() == "tauri" {
        return Navigation::Allow;
    }
    if url.as_str() == "about:blank" {
        return Navigation::Allow;
    }
    match url.scheme() {
        "ahab" => Navigation::Link,
        "http" | "https" if server == Some(origin.as_str()) => Navigation::Allow,
        "http" | "https" => Navigation::External,
        _ => Navigation::Deny,
    }
}

/// The app's own origins: `tauri://localhost` (Linux, macOS),
/// `http://tauri.localhost` (Windows), and the dev server in development.
pub fn builtin_origins(app: &AppHandle) -> Vec<String> {
    let mut origins = vec![
        "tauri://localhost".to_owned(),
        "http://tauri.localhost".to_owned(),
    ];
    if tauri::is_dev()
        && let Some(dev) = &app.config().build.dev_url
    {
        origins.push(dev.origin().ascii_serialization());
    }
    origins
}

/// Whether `url` is one of the app's own screens.
pub fn is_builtin(url: &Url, origins: &[String]) -> bool {
    url.scheme() == "tauri" || origins.contains(&url.origin().ascii_serialization())
}

/// Which server each window shows (`None`: the built-in screens).
#[derive(Default)]
pub struct Shell {
    shown: Arc<Mutex<HashMap<String, Option<Server>>>>,
    counter: AtomicU32,
    /// Servers whose origin may use the bridge (a capability each).
    allowed: Mutex<Vec<String>>,
    /// Quitting: windows closing now stay in the session to restore.
    pub quitting: std::sync::atomic::AtomicBool,
}

impl Shell {
    pub fn server_of(&self, label: &str) -> Option<Server> {
        self.shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(label)
            .cloned()
            .flatten()
    }

    fn set(&self, label: &str, server: Option<Server>) {
        self.shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(label.to_owned(), server);
    }

    pub fn forget(&self, label: &str) {
        self.shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(label);
    }

    fn next_label(&self, app: &AppHandle) -> String {
        loop {
            let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
            let label = if n == 1 {
                "main".to_owned()
            } else {
                format!("window-{n}")
            };
            if app.get_webview_window(&label).is_none() {
                return label;
            }
        }
    }

    /// Lets `server`'s origin call the bridge functions, and only those: a
    /// capability for its origin, added once.
    fn allow_bridge(&self, app: &AppHandle, server: &Server) -> Result<(), Failure> {
        let mut allowed = self
            .allowed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if allowed.contains(&server.url) {
            return Ok(());
        }
        app.add_capability(crate::bridge::capability(&server.url))
            .map_err(|error| Failure::new("capability", error.to_string()))?;
        allowed.push(server.url.clone());
        Ok(())
    }
}

/// What a new window shows first.
pub enum Target {
    /// A built-in screen (`#/connect`, `#/settings`…).
    Screen(String),
    /// A server's web app, at a path (its last one when none).
    Server { id: String, path: Option<String> },
}

fn system_theme(window: &WebviewWindow) -> Theme {
    window.theme().unwrap_or(Theme::Dark)
}

/// Opens a window onto the splash, then shows `target`.
pub fn open(app: &AppHandle, target: Target, label: Option<String>) -> Result<String, Failure> {
    #[cfg(desktop)]
    crate::comfort::stop_tray_watch(app);
    let shell = app.state::<Arc<Shell>>();
    let label = label.unwrap_or_else(|| shell.next_label(app));
    let route = match &target {
        Target::Screen(route) => route.clone(),
        Target::Server { id, path } => {
            let mut route = format!("/opening?server={id}");
            if let Some(path) = path {
                route.push_str("&path=");
                route.extend(url::form_urlencoded::byte_serialize(path.as_bytes()));
            }
            route
        }
    };
    let origins = builtin_origins(app);
    let shown = shell.shown.clone();
    let navigating_label = label.clone();
    let navigating_app = app.clone();
    let new_window_app = app.clone();
    let new_window_label = label.clone();
    let loaded_app = app.clone();
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html#{route}").into()),
    )
    .title("Ahab")
    .inner_size(1280.0, 820.0)
    .min_inner_size(560.0, 420.0)
    .visible(false)
    .background_color(DARK)
    .initialization_script(INIT_SCRIPT)
    .on_navigation(move |url| {
        let server = shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&navigating_label)
            .cloned()
            .flatten()
            .map(|server| server.url);
        match decide(url, server.as_deref(), &origins) {
            Navigation::Allow => true,
            Navigation::External => {
                open_external(&navigating_app, url);
                false
            }
            Navigation::Link => {
                let _ = navigating_app.emit("ahab-link", url.as_str());
                false
            }
            Navigation::Deny => false,
        }
    })
    .on_new_window(move |url, _features| {
        let shell = new_window_app.state::<Arc<Shell>>();
        let server = shell.server_of(&new_window_label);
        let origin = url.origin().ascii_serialization();
        match server {
            Some(server) if server.url == origin => {
                let path = safe_path(&path_of(&url));
                let _ = open(
                    &new_window_app,
                    Target::Server {
                        id: server.id,
                        path,
                    },
                    None,
                );
            }
            _ if matches!(url.scheme(), "http" | "https") => open_external(&new_window_app, &url),
            _ => {}
        }
        NewWindowResponse::Deny
    })
    .on_page_load(move |window, payload| {
        if payload.event() != PageLoadEvent::Finished {
            return;
        }
        remember(&loaded_app, &window, payload.url());
    })
    .on_document_title_changed(|window, title| {
        let _ = window.set_title(if title.is_empty() { "Ahab" } else { &title });
    })
    .build()
    .map_err(|error| Failure::new("window", error.to_string()))?;
    let color = if system_theme(&window) == Theme::Light {
        LIGHT
    } else {
        DARK
    };
    let _ = window.set_background_color(Some(color));
    #[cfg(desktop)]
    let _ = window.restore_state(StateFlags::all());
    let _ = window.show();
    let _ = window.set_focus();
    shell.set(&label, None);
    Ok(label)
}

/// Shows `id`'s web app in window `label`: its credential as the cookie
/// the server reads, then its address (`path`, else its last page).
pub fn show_server(
    app: &AppHandle,
    label: &str,
    id: &str,
    path: Option<&str>,
) -> Result<(), Failure> {
    let state = app.state::<Arc<AppState>>();
    let shell = app.state::<Arc<Shell>>();
    let server = state.servers.get(id)?;
    let credential = state
        .servers
        .credential(id)?
        .ok_or_else(|| Failure::new("not_paired", "This server has no credential here."))?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| Failure::new("not_found", "No such window."))?;
    let base =
        Url::parse(&server.url).map_err(|error| Failure::new("bad_address", error.to_string()))?;
    let host = base.host_str().unwrap_or_default().to_owned();
    let cookie = Cookie::build((COOKIE, credential))
        .domain(host)
        .path("/")
        .http_only(true)
        .secure(base.scheme() == "https")
        .same_site(tauri::webview::cookie::SameSite::Lax)
        .build();
    window
        .set_cookie(cookie)
        .map_err(|error| Failure::new("window", error.to_string()))?;
    shell.allow_bridge(app, &server)?;
    let path = path
        .and_then(safe_path)
        .or_else(|| server.last_path.clone())
        .unwrap_or_else(|| "/".to_owned());
    let url = base
        .join(&path)
        .map_err(|error| Failure::new("bad_address", error.to_string()))?;
    shell.set(label, Some(server.clone()));
    let _ = state.files.record_window(WindowRecord {
        label: label.to_owned(),
        server_id: Some(server.id.clone()),
        path: Some(path),
    });
    window
        .navigate(url)
        .map_err(|error| Failure::new("window", error.to_string()))
}

/// Shows a built-in screen in window `label`.
pub fn show_screen(app: &AppHandle, label: &str, route: &str) -> Result<(), Failure> {
    let shell = app.state::<Arc<Shell>>();
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| Failure::new("not_found", "No such window."))?;
    shell.set(label, None);
    let base = builtin_origins(app)
        .into_iter()
        .rev()
        .find(|origin| tauri::is_dev() || origin.starts_with("tauri"))
        .unwrap_or_else(|| "tauri://localhost".to_owned());
    let url = Url::parse(&format!("{base}/index.html#{route}"))
        .map_err(|error| Failure::new("bad_address", error.to_string()))?;
    window
        .navigate(url)
        .map_err(|error| Failure::new("window", error.to_string()))
}

/// Captures client-side navigation before the native window is destroyed.
#[cfg(desktop)]
pub fn remember_current(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label)
        && let Ok(url) = window.url()
    {
        remember(app, &window, &url);
    }
}

/// `WebKitGTK` may cache a destroyed window's renderer for several minutes.
#[cfg(target_os = "linux")]
pub fn release_renderer(app: &AppHandle, label: &str) {
    use webkit2gtk::WebViewExt as _;

    if let Some(window) = app.get_webview_window(label)
        && let Err(error) = window.with_webview(|view| view.inner().terminate_web_process())
    {
        tracing::debug!(%error, %label, "cannot release the workbench renderer");
    }
}

/// Remembers the server page a window shows, to restore it.
fn remember(app: &AppHandle, window: &WebviewWindow, url: &Url) {
    let shell = app.state::<Arc<Shell>>();
    let Some(server) = shell.server_of(window.label()) else {
        return;
    };
    if url.origin().ascii_serialization() != server.url {
        return;
    }
    let path = path_of(url);
    // The pairing screen is no place to come back to.
    if path.starts_with("/pair") {
        return;
    }
    let state = app.state::<Arc<AppState>>();
    let _ = state.servers.visited(&server.id, &path);
    let _ = state.files.record_window(WindowRecord {
        label: window.label().to_owned(),
        server_id: Some(server.id),
        path: Some(path),
    });
}

fn path_of(url: &Url) -> String {
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

/// Opens an http(s) address in the system browser; nothing else.
pub fn open_external(app: &AppHandle, url: &Url) {
    use tauri_plugin_opener::OpenerExt;
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }
    if let Err(error) = app.opener().open_url(url.as_str(), None::<&str>) {
        tracing::warn!(%error, %url, "cannot open the browser");
    }
}

/// Brings a window forward, or opens one.
pub fn focus_or_open(app: &AppHandle) {
    let windows = app.webview_windows();
    if let Some(window) = windows.values().next() {
        let _ = window.show();
        #[cfg(desktop)]
        let _ = window.unminimize();
        let _ = window.set_focus();
    } else {
        let state = app.state::<Arc<AppState>>();
        crate::restore_windows(app, &state);
    }
}

/// The focused window, else any.
pub fn front_window(app: &AppHandle) -> Option<String> {
    let windows = app.webview_windows();
    windows
        .iter()
        .find(|(_, window)| window.is_focused().unwrap_or(false))
        .or_else(|| windows.iter().next())
        .map(|(label, _)| label.clone())
}

/// Shows a built-in screen in the front window (or a new one).
pub fn show_in_front(app: &AppHandle, route: &str) {
    match front_window(app) {
        Some(label) => {
            let _ = show_screen(app, &label, route);
            focus_or_open(app);
        }
        None => {
            let _ = open(app, Target::Screen(route.to_owned()), None);
        }
    }
}

/// Run in every page before its own scripts: marks the app, so the web
/// app's splash hands over from the app's without starting again.
const INIT_SCRIPT: &str = r#"Object.defineProperty(window, "__AHAB_APP__", { value: Object.freeze({ shell: "tauri" }), configurable: false });
document.documentElement && document.documentElement.classList.add("in-app");"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn keeps_the_server_in_the_window_and_sends_the_rest_out() {
        let builtin = vec![
            "tauri://localhost".to_owned(),
            "http://localhost:5183".to_owned(),
        ];
        let server = Some("https://atlas.lan");
        assert_eq!(
            decide(&url("https://atlas.lan/p/1"), server, &builtin),
            Navigation::Allow
        );
        assert_eq!(
            decide(&url("tauri://localhost/index.html"), server, &builtin),
            Navigation::Allow
        );
        assert_eq!(
            decide(&url("http://localhost:5183/"), None, &builtin),
            Navigation::Allow
        );
        assert_eq!(
            decide(&url("https://github.com/x"), server, &builtin),
            Navigation::External
        );
        // Another port is another origin.
        assert_eq!(
            decide(&url("https://atlas.lan:8443/"), server, &builtin),
            Navigation::External
        );
        assert_eq!(
            decide(&url("http://atlas.lan/"), server, &builtin),
            Navigation::External
        );
        assert_eq!(
            decide(&url("ahab://open?server=x"), server, &builtin),
            Navigation::Link
        );
        assert_eq!(
            decide(&url("file:///etc/passwd"), server, &builtin),
            Navigation::Deny
        );
        assert_eq!(
            decide(&url("javascript:alert(1)"), server, &builtin),
            Navigation::Deny
        );
        // The built-in screens' window has no server: every page is outside.
        assert_eq!(
            decide(&url("https://atlas.lan/"), None, &builtin),
            Navigation::External
        );
    }

    #[test]
    fn knows_its_own_screens() {
        let builtin = vec![
            "tauri://localhost".to_owned(),
            "http://tauri.localhost".to_owned(),
        ];
        assert!(is_builtin(
            &url("tauri://localhost/index.html#/connect"),
            &builtin
        ));
        assert!(is_builtin(&url("http://tauri.localhost/"), &builtin));
        assert!(!is_builtin(&url("https://atlas.lan/"), &builtin));
        assert!(!is_builtin(&url("http://localhost:5183/"), &builtin));
    }
}
