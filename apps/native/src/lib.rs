//! Ahab for desktop: a thin shell around the paired server's
//! own web app, which also runs Ahab on this computer through the `ahab`
//! command line it bundles.

pub mod background;
pub mod bridge;
#[cfg(desktop)]
pub mod comfort;
#[cfg(mobile)]
#[path = "comfort_mobile.rs"]
pub mod comfort;
pub mod commands;
pub mod computer;
pub mod links;
pub mod secrets;
pub mod server_api;
pub mod servers;
pub mod settings;
pub mod state;
pub mod windows;

use std::path::PathBuf;
use std::sync::Arc;
#[cfg(desktop)]
use std::sync::atomic::Ordering;

#[cfg(desktop)]
use tauri::RunEvent;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::comfort::Comfort;
use crate::commands::PendingLinks;
use crate::computer::{Computer, Env};
use crate::state::AppState;
use crate::windows::{Shell, Target};

/// The `ahab` this app bundles: beside the app's own binary (the
/// sidecar); `AHAB_APP_BUNDLED` replaces it in development.
fn bundled_ahab() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AHAB_APP_BUNDLED") {
        return Some(PathBuf::from(path));
    }
    let exe = std::env::current_exe().ok()?;
    Some(
        exe.parent()?
            .join(if cfg!(windows) { "ahab.exe" } else { "ahab" }),
    )
}

/// Where the app keeps its server list and settings: the platform's app
/// data folder, or `AHAB_APP_DATA_DIR`.
fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    match std::env::var_os("AHAB_APP_DATA_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => app.path().app_data_dir(),
    }
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir(app)?;
    std::fs::create_dir_all(&dir)?;
    // `AHAB_APP_SECRETS=file` keeps credentials out of the keychain (a
    // sandbox to try the app in).
    let (secrets, note) = if std::env::var("AHAB_APP_SECRETS").as_deref() == Ok("file") {
        (
            Box::new(secrets::EncryptedFile::new(&dir)) as Box<dyn secrets::SecretStore>,
            Some("AHAB_APP_SECRETS=file: credentials are in an encrypted file in the app's data folder.".to_owned()),
        )
    } else {
        secrets::open(&dir)
    };
    let computer = Computer::new(Env::process(), bundled_ahab());
    let state = AppState::new(&dir, secrets, note, computer);
    app.manage(state.clone());
    app.manage(Arc::new(Shell::default()));
    app.manage(PendingLinks::default());
    app.manage(Comfort::default());

    #[cfg(desktop)]
    {
        if let Err(error) = comfort::build_tray(app) {
            tracing::info!(%error, "no tray here");
        }
        let settings = state.files.settings();
        if let Err(error) = comfort::apply_shortcut(app, settings.global_shortcut.as_deref()) {
            tracing::warn!(%error, "the global shortcut is not available");
        }
    }

    // An AppImage has no installed desktop entry: register `ahab://` for
    // it. The packages' desktop entries register it themselves.
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_some()
        && let Err(error) = app.deep_link().register_all()
    {
        tracing::warn!(%error, "cannot register ahab:// links");
    }
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            open_link(&handle, url.as_str());
        }
    });

    restore_windows(app, &state);
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            open_link(app, url.as_str());
        }
    }
    Ok(())
}

/// Opens the windows of the last session, each on its server and page;
/// or one window on the start screen.
pub(crate) fn restore_windows(app: &AppHandle, state: &AppState) {
    let session = state.files.session();
    let known = state
        .servers
        .load()
        .map(|list| list.servers)
        .unwrap_or_default();
    let mut opened = 0;
    for record in session.windows {
        let Some(id) = record
            .server_id
            .filter(|id| known.iter().any(|server| server.id == *id))
        else {
            let _ = state.files.forget_window(&record.label);
            continue;
        };
        let target = Target::Server {
            id,
            path: record.path,
        };
        match windows::open(app, target, Some(record.label)) {
            Ok(_) => opened += 1,
            Err(error) => tracing::warn!(%error, "cannot restore a window"),
        }
    }
    if opened == 0 {
        let _ = windows::open(
            app,
            Target::Screen("/start".to_owned()),
            Some("main".to_owned()),
        );
    }
}

#[cfg(desktop)]
fn stays_in_tray(app: &AppHandle) -> bool {
    app.tray_by_id("ahab").is_some()
        && app
            .state::<Arc<AppState>>()
            .files
            .settings()
            .keeps_in_tray(app.state::<Comfort>().ahab_runs_here.load(Ordering::SeqCst))
}

/// An `ahab://` link: the link screen reads it and asks before anything.
fn open_link(app: &AppHandle, link: &str) {
    app.state::<PendingLinks>()
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(link.to_owned());
    windows::show_in_front(app, "/link");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("AHAB_APP_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let builder = tauri::Builder::default();
    // Desktop: one instance (a second launch or a clicked link goes to this
    // one; first), window state and the global shortcut.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            windows::focus_or_open(app);
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Windows are restored by the app itself, one by one.
                .skip_initial_state("main")
                .build(),
        )
        .plugin(comfort::shortcut_plugin());
    let app = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(background::init())
        .invoke_handler(tauri::generate_handler![
            bridge::bridge_info,
            bridge::bridge_background,
            bridge::bridge_notify,
            bridge::bridge_badge,
            bridge::bridge_open_external,
            bridge::bridge_servers,
            bridge::bridge_switch_server,
            bridge::bridge_add_server,
            bridge::bridge_new_window,
            bridge::bridge_this_computer,
            commands::boot,
            commands::reach,
            commands::pair,
            commands::ask,
            commands::wait,
            commands::check_server,
            commands::open_server,
            commands::open_server_window,
            commands::show_screen,
            commands::remove_server,
            commands::rename_server,
            commands::this_computer,
            commands::run_here,
            commands::agents_here,
            commands::open_in_browser,
            commands::use_existing,
            commands::start_here,
            commands::stop_here,
            commands::join_server,
            commands::join_with_code,
            commands::leave,
            commands::parse_link,
            commands::take_link,
            commands::get_settings,
            commands::set_settings,
            commands::check_for_updates,
            commands::quit,
        ])
        .setup(|app| setup(app.handle()))
        .on_window_event(|window, event| match event {
            #[cfg(desktop)]
            WindowEvent::CloseRequested { .. } => {
                let app = window.app_handle();
                let shell = app.state::<Arc<Shell>>();
                if shell.quitting.load(Ordering::SeqCst) {
                    return;
                }
                let state = app.state::<Arc<AppState>>();
                let other_open = app
                    .webview_windows()
                    .keys()
                    .any(|label| label != window.label());
                if other_open {
                    // Not the last: it is not restored next time.
                    let _ = state.files.forget_window(window.label());
                    return;
                }
                tracing::debug!(
                    label = window.label(),
                    "releasing the last workbench window"
                );
                windows::remember_current(app, window.label());
                if stays_in_tray(app) {
                    comfort::watch_in_tray(app, window.label());
                    #[cfg(target_os = "linux")]
                    windows::release_renderer(app, window.label());
                }
            }
            WindowEvent::Destroyed => {
                window
                    .app_handle()
                    .state::<Arc<Shell>>()
                    .forget(window.label());
            }
            _ => {}
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            eprintln!("Ahab cannot start: {error}");
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        // A phone has no tray to stay in.
        #[cfg(mobile)]
        let _ = (app, event);
        #[cfg(desktop)]
        if let RunEvent::ExitRequested { api, code, .. } = event {
            // The last window closed while the app stays in the tray.
            let keep = code.is_none()
                && !app.state::<Arc<Shell>>().quitting.load(Ordering::SeqCst)
                && stays_in_tray(app);
            if keep {
                api.prevent_exit();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    /// The app's version is Ahab's: it ships the binary.
    #[test]
    fn the_version_is_ahabs() {
        let ahab = include_str!("../../../crates/ahab/Cargo.toml");
        let version = ahab
            .lines()
            .find_map(|line| line.strip_prefix("version = "))
            .map(|value| value.trim_matches('"'));
        assert_eq!(version, Some(env!("CARGO_PKG_VERSION")));
    }
}
