//! Desktop comfort: the tray, native notifications, the
//! badge and the global shortcut.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::bridge::Notification;
use crate::computer::Role;
use crate::server_api::ServerApi;
use crate::servers::Server;
use crate::state::{AppState, Failure};
use crate::windows::{self, Target};

const TRAY: &str = "ahab";

/// What the tray and closing windows go by.
#[derive(Default)]
pub struct Comfort {
    /// Ahab runs on this computer (as a server or a machine).
    pub ahab_runs_here: AtomicBool,
    /// Each window's badge count.
    badges: Mutex<HashMap<String, u32>>,
    tray_watcher: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn icon(dot: bool) -> Option<Image<'static>> {
    let base = Image::from_bytes(include_bytes!("../icons/32x32.png")).ok()?;
    if !dot {
        return Some(base.to_owned());
    }
    Some(with_dot(&base))
}

/// The icon with a dot in its top right corner: an agent needs you.
pub fn with_dot(base: &Image<'_>) -> Image<'static> {
    let (width, height) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    let radius = f64::from(width.min(height)) * 0.22;
    let (cx, cy) = (f64::from(width) - radius - 0.5, radius + 0.5);
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
            if dx.hypot(dy) <= radius {
                let at = ((y * width + x) * 4) as usize;
                // The web app's sea-glass accent.
                rgba[at..at + 4].copy_from_slice(&[0x8f, 0xb3, 0xa3, 0xff]);
            }
        }
    }
    Image::new_owned(rgba, width, height)
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = Menu::with_items(
        app,
        &[&MenuItem::with_id(
            app,
            "open",
            "Open Ahab",
            true,
            None::<&str>,
        )?],
    )?;
    let mut builder = TrayIconBuilder::with_id(TRAY)
        .tooltip("Ahab")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::focus_or_open(tray.app_handle());
            }
        });
    if let Some(icon) = icon(false) {
        builder = builder.icon(icon);
    }
    let tray = builder.build(app)?;
    refresh_tray(app);
    Ok(tray)
}

/// Rebuilds the tray's menu: the local service's state, the servers, and
/// Start or Stop.
pub fn refresh_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>().inner().clone();
        let computer = state.this_computer().await;
        let runs = computer.role != Role::None && computer.running;
        app.state::<Comfort>()
            .ahab_runs_here
            .store(runs, Ordering::SeqCst);
        let servers = state
            .servers
            .load()
            .map(|list| list.servers)
            .unwrap_or_default();
        let status = match (computer.role, computer.running) {
            (Role::Server, true) => "Ahab runs on this computer".to_owned(),
            (Role::Machine, true) => format!(
                "This computer is a machine of {}",
                computer.joined.as_deref().unwrap_or("a server")
            ),
            (Role::Server | Role::Machine, false) => "Ahab is stopped on this computer".to_owned(),
            (Role::None, _) => "Ahab does not run on this computer".to_owned(),
        };
        let menu = tray_menu(&app, &status, &servers, computer.role, computer.running);
        if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), menu) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}

fn tray_menu(
    app: &AppHandle,
    status: &str,
    servers: &[Server],
    role: Role,
    running: bool,
) -> tauri::Result<Menu<Wry>> {
    let status = MenuItem::with_id(app, "status", status, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Ahab", true, None::<&str>)?;
    let server_items = servers
        .iter()
        .map(|server| {
            MenuItem::with_id(
                app,
                format!("server:{}", server.id),
                &server.name,
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let add = MenuItem::with_id(app, "add", "Add a server…", true, None::<&str>)?;
    let mut entries: Vec<&dyn IsMenuItem<Wry>> = server_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<Wry>)
        .collect();
    entries.push(&add);
    let servers_menu = Submenu::with_items(app, "Servers", true, &entries)?;
    let start = MenuItem::with_id(
        app,
        "start",
        "Start Ahab on this computer",
        true,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        "stop",
        "Stop Ahab on this computer…",
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "App settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&status, &separator, &open, &servers_menu];
    match (role, running) {
        (Role::None, _) => {}
        (_, true) => items.push(&stop),
        (_, false) => items.push(&start),
    }
    items.extend([&separator2 as &dyn IsMenuItem<Wry>, &settings, &quit]);
    Menu::with_items(app, &items)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => windows::focus_or_open(app),
        "quit" => crate::commands::quit_app(app),
        "add" => windows::show_in_front(app, "/connect"),
        "settings" => windows::show_in_front(app, "/settings"),
        // Stopping asks first, on the This computer screen.
        "stop" => windows::show_in_front(app, "/settings?confirm=stop"),
        "start" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>().inner().clone();
                if let Err(error) = state.start_here().await {
                    tracing::warn!(%error, "cannot start Ahab from the tray");
                    windows::show_in_front(&app, "/settings");
                }
                refresh_tray(&app);
            });
        }
        other => {
            if let Some(server) = other.strip_prefix("server:") {
                let label = windows::front_window(app);
                let shown = match label {
                    Some(label) => windows::show_server(app, &label, server, None),
                    None => windows::open(
                        app,
                        Target::Server {
                            id: server.to_owned(),
                            path: None,
                        },
                        None,
                    )
                    .map(|_| ()),
                };
                if let Err(error) = shown {
                    tracing::warn!(%error, "cannot open the server");
                }
                windows::focus_or_open(app);
            }
        }
    }
}

/// The last window's notification stream stays connected without a `WebView`.
pub fn watch_in_tray(app: &AppHandle, label: &str) {
    stop_tray_watch(app);
    let state = app.state::<Arc<AppState>>();
    let Some(server) = app.state::<Arc<windows::Shell>>().server_of(label) else {
        return;
    };
    let Ok(Some(credential)) = state.servers.credential(&server.id) else {
        return;
    };
    let app = app.clone();
    let label = label.to_owned();
    let watcher = tauri::async_runtime::spawn({
        let app = app.clone();
        async move {
            let api = ServerApi::new(&server.url);
            let mut delay = Duration::from_secs(1);
            loop {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    Duration::from_secs(10),
                    api.notification_stream(&credential),
                )
                .await
                {
                    tracing::debug!(%label, "tray notification stream connected");
                    if let Some(count) = api.unread_notifications(&credential).await {
                        badge(&app, &label, count);
                    }
                    let mut pending = Vec::new();
                    while let Ok(Ok(Some(bytes))) =
                        tokio::time::timeout(Duration::from_secs(45), stream.chunk()).await
                    {
                        delay = Duration::from_secs(1);
                        let Some(events) = notification_frames(&mut pending, &bytes) else {
                            break;
                        };
                        for event in events {
                            match event["type"].as_str() {
                                Some("created") => {
                                    let notice = &event["notification"];
                                    if let (Some(title), Some(kind)) =
                                        (notice["title"].as_str(), notice["kind"].as_str())
                                        && api.notification_allowed(&credential, kind).await
                                    {
                                        notify(
                                            &app,
                                            &server,
                                            &label,
                                            Notification {
                                                title: title.to_owned(),
                                                body: notice["body"].as_str().map(str::to_owned),
                                                target: notification_target(&notice["target"]),
                                            },
                                        );
                                    }
                                    if let Some(count) = api.unread_notifications(&credential).await
                                    {
                                        badge(&app, &label, count);
                                    }
                                }
                                Some("changed") => {
                                    let count = if let Some(count) = event["unread"].as_u64() {
                                        Some(u32::try_from(count).unwrap_or(u32::MAX))
                                    } else {
                                        api.unread_notifications(&credential).await
                                    };
                                    if let Some(count) = count {
                                        badge(&app, &label, count);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        }
    });
    let comfort = app.state::<Comfort>();
    *comfort
        .tray_watcher
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(watcher);
}

pub fn stop_tray_watch(app: &AppHandle) {
    if let Some(watcher) = app
        .state::<Comfort>()
        .tray_watcher
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    {
        watcher.abort();
        tracing::debug!("tray notification stream stopped");
    }
}

fn notification_frames(pending: &mut Vec<u8>, chunk: &[u8]) -> Option<Vec<Value>> {
    // The server sends UTF-8 JSON on one data line, with LF separators.
    if pending.len().saturating_add(chunk.len()) > 512 * 1024 {
        return None;
    }
    pending.extend_from_slice(chunk);
    let mut events = Vec::new();
    let mut start = 0;
    for end in pending
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .map(|(index, _)| index)
    {
        if let Some(data) = pending[start..end].strip_prefix(b"data:")
            && let Ok(event) = serde_json::from_slice(data)
        {
            events.push(event);
        }
        start = end + 1;
    }
    pending.drain(..start);
    Some(events)
}

fn notification_target(target: &Value) -> Option<String> {
    let id = |name| target[name].as_str()?.parse::<uuid::Uuid>().ok();
    match target["type"].as_str()? {
        "route" => crate::links::safe_path(target["path"].as_str()?),
        "machine" => Some(format!("/settings/machines/{}", id("machineId")?)),
        "session" => {
            let (project, session) = (id("projectId")?, id("sessionId")?);
            let mut path = format!("/p/{project}/s/{session}");
            if let Some(chat) = id("chatId") {
                use std::fmt::Write as _;
                let _ = write!(path, "?chat={chat}");
            }
            Some(path)
        }
        _ => None,
    }
}

/// A native notification; clicking it opens its target in the window
/// that sent it.
pub fn notify(app: &AppHandle, server: &Server, label: &str, notification: Notification) {
    let state = app.state::<Arc<AppState>>();
    if !state.files.settings().notifications {
        return;
    }
    let app = app.clone();
    let server_id = server.id.clone();
    let server_name = server.name.clone();
    let label = label.to_owned();
    std::thread::spawn(move || {
        let mut native = notify_rust::Notification::new();
        native
            .appname("Ahab")
            .summary(&notification.title)
            .body(notification.body.as_deref().unwrap_or(&server_name))
            .action("default", "Open");
        match native.show() {
            #[cfg(all(unix, not(target_os = "macos")))]
            Ok(handle) => handle.wait_for_action(|action| {
                if action != "default" {
                    return;
                }
                let target = notification.target.clone();
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let path = target.as_deref().and_then(crate::links::safe_path);
                    if let Some(window) = app2.get_webview_window(&label) {
                        if let Some(path) = path {
                            let _ = windows::show_server(&app2, &label, &server_id, Some(&path));
                        }
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    } else {
                        let _ = windows::open(
                            &app2,
                            Target::Server {
                                id: server_id,
                                path,
                            },
                            Some(label),
                        );
                    }
                });
            }),
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            Ok(_) => {}
            Err(error) => tracing::info!(%error, "no notification server"),
        }
    });
}

/// The dot: on the tray icon, and the window's badge where the desktop
/// shows one.
pub fn badge(app: &AppHandle, label: &str, count: u32) {
    let comfort = app.state::<Comfort>();
    let total: u32 = {
        let mut badges = comfort
            .badges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        badges.insert(label.to_owned(), count);
        badges.values().sum()
    };
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_badge_count((count > 0).then_some(i64::from(count)));
    }
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.set_icon(icon(total > 0));
    }
}

/// Registers the global shortcut that brings Ahab forward (none: off).
pub fn apply_shortcut(app: &AppHandle, shortcut: Option<&str>) -> Result<(), Failure> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let Some(shortcut) = shortcut
        .map(str::trim)
        .filter(|shortcut| !shortcut.is_empty())
    else {
        return Ok(());
    };
    shortcuts.register(shortcut).map_err(|error| {
        Failure::new(
            "bad_shortcut",
            format!("{shortcut} cannot be used: {error}"),
        )
    })
}

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                windows::focus_or_open(app);
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_reads_split_notification_events_and_safe_targets() {
        let mut pending = Vec::new();
        assert!(
            notification_frames(&mut pending, b": heartbeat\n\ndata: {\"type\":")
                .unwrap()
                .is_empty()
        );
        let events = notification_frames(&mut pending, b"\"changed\",\"unread\":2}\n\n").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["unread"], 2);
        assert!(pending.is_empty());
        assert!(notification_frames(&mut pending, &vec![b'x'; 512 * 1024 + 1]).is_none());
        assert_eq!(
            notification_target(&serde_json::json!({"type": "route", "path": "/notifications"})),
            Some("/notifications".to_owned()),
        );
        assert!(
            notification_target(&serde_json::json!({"type": "route", "path": "//other"})).is_none()
        );
    }

    #[test]
    fn a_session_notification_opens_its_chat_in_the_project() {
        let project = "0196a7f0-0000-7000-8000-000000000001";
        let session = "0196a7f0-0000-7000-8000-000000000002";
        let chat = "0196a7f0-0000-7000-8000-000000000003";
        assert_eq!(
            notification_target(&serde_json::json!({
                "type": "session",
                "projectId": project,
                "sessionId": session,
                "chatId": chat,
                "projectName": "ahab",
                "sessionTitle": "Fix it",
            })),
            Some(format!("/p/{project}/s/{session}?chat={chat}")),
        );
        assert_eq!(
            notification_target(
                &serde_json::json!({"type": "session", "projectId": project, "sessionId": session})
            ),
            Some(format!("/p/{project}/s/{session}")),
        );
        assert!(
            notification_target(&serde_json::json!({"type": "session", "projectId": project}))
                .is_none()
        );
    }

    #[test]
    fn the_dot_goes_in_the_top_right_corner() {
        let base = Image::new_owned(vec![0; 32 * 32 * 4], 32, 32);
        let dotted = with_dot(&base);
        let pixel = |x: u32, y: u32| {
            let at = ((y * 32 + x) * 4) as usize;
            dotted.rgba()[at..at + 4].to_vec()
        };
        assert_eq!(pixel(26, 5), vec![0x8f, 0xb3, 0xa3, 0xff]);
        assert_eq!(pixel(3, 28), vec![0, 0, 0, 0]);
    }
}
