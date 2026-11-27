//! Notifications while the app is in the background.
//! On Android, a Kotlin plugin (`gen/android/…/background`) holds the
//! server's stream in a foreground service while agents work, and checks
//! every 15 minutes for what it missed. On desktop, the tray holds its own
//! notification connection after the last window closes.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Runtime;
use tauri::plugin::TauriPlugin;

use crate::servers::Server;
use crate::state::Failure;

/// What the web app asks of the background connection.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Ask {
    /// Its state (and this server, kept for it).
    Status,
    /// Android 13's notification permission.
    Allow,
    /// How many agents work now: it connects while some do.
    Working { working: u64 },
    /// Stay connected even when nothing runs.
    Stay { stay: bool },
    /// The settings where Android stops apps in the background.
    Battery,
}

/// What it answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `granted`, `denied` or `prompt`.
    pub permission: String,
    pub stay_connected: bool,
    pub running: bool,
}

#[cfg(target_os = "android")]
mod android {
    use serde_json::json;
    use tauri::plugin::{Builder, PluginHandle};
    use tauri::{AppHandle, Manager, Runtime};

    use super::{Ask, Status, TauriPlugin, Value};
    use crate::servers::Server;
    use crate::state::Failure;

    struct Handle<R: Runtime>(PluginHandle<R>);

    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("background")
            .setup(|app, api| {
                let handle =
                    api.register_android_plugin("dev.ahab.app.background", "BackgroundPlugin")?;
                app.manage(Handle(handle));
                Ok(())
            })
            .build()
    }

    fn run<R: Runtime>(app: &AppHandle<R>, command: &str, args: Value) -> Result<Status, Failure> {
        let handle = app
            .try_state::<Handle<R>>()
            .ok_or_else(|| Failure::new("not_on_this_device", "No background connection here."))?;
        handle
            .0
            .run_mobile_plugin::<Status>(command, args)
            .map_err(|error| Failure::new("background_failed", error.to_string()))
    }

    pub fn ask<R: Runtime>(
        app: &AppHandle<R>,
        server: &Server,
        credential: Option<String>,
        ask: Ask,
    ) -> Result<Status, Failure> {
        // The server the web app shows is the one to follow.
        if let Some(token) = credential {
            run(
                app,
                "configure",
                json!({ "server": server.url, "token": token, "name": server.name }),
            )?;
        }
        match ask {
            Ask::Status => run(app, "status", json!({})),
            Ask::Allow => run(app, "allow", json!({})),
            Ask::Working { working } => run(app, "working", json!({ "working": working })),
            Ask::Stay { stay } => run(app, "setStayConnected", json!({ "stay": stay })),
            Ask::Battery => run(app, "batterySettings", json!({})),
        }
    }
}

/// The plugin that brings the Kotlin side in; nothing outside Android.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    #[cfg(target_os = "android")]
    {
        android::init()
    }
    #[cfg(not(target_os = "android"))]
    {
        tauri::plugin::Builder::new("background").build()
    }
}

/// Answers `ask` for the web app of `server`, whose credential this app
/// keeps.
pub fn ask<R: Runtime>(
    app: &tauri::AppHandle<R>,
    server: &Server,
    credential: Option<String>,
    ask: Ask,
) -> Result<Status, Failure> {
    #[cfg(target_os = "android")]
    {
        android::ask(app, server, credential, ask)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, server, credential, ask, Value::Null);
        Err(Failure::new(
            "not_on_this_device",
            "Only the phone app keeps a background connection.",
        ))
    }
}
