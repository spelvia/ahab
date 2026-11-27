//! Desktop comfort on a phone: there is no tray, no global
//! shortcut and no dock badge. While the app is open, the server's web app
//! shows its own toasts; background notifications come later.

use std::sync::atomic::AtomicBool;

use tauri::AppHandle;

use crate::bridge::Notification;
use crate::servers::Server;
use crate::state::Failure;

/// What closing windows goes by; on a phone Ahab never runs here.
#[derive(Default)]
pub struct Comfort {
    pub ahab_runs_here: AtomicBool,
}

pub fn refresh_tray(_app: &AppHandle) {}

pub fn notify(_app: &AppHandle, _server: &Server, _label: &str, _notification: Notification) {}

pub fn badge(_app: &AppHandle, _label: &str, _count: u32) {}

pub fn apply_shortcut(_app: &AppHandle, shortcut: Option<&str>) -> Result<(), Failure> {
    match shortcut {
        None => Ok(()),
        Some(_) => Err(Failure::new(
            "not_on_this_device",
            "A phone has no global shortcut.",
        )),
    }
}
