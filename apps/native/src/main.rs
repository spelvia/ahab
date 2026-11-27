// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    appimage_without_host_gio_modules();
    ahab_desktop::run();
}

/// In an `AppImage`, `GIO` must load only the modules bundled with its
/// `GLib`: a host's `GIO_EXTRA_MODULES` (`NixOS` sets one) brings modules
/// built for another one (`dconf`: `undefined symbol:
/// g_assertion_message_cmpint`).
/// The variable cannot be removed safely once threads run, so the app
/// starts again without it, before anything else.
#[cfg(target_os = "linux")]
fn appimage_without_host_gio_modules() {
    use std::os::unix::process::CommandExt;
    if std::env::var_os("APPDIR").is_none() || std::env::var_os("GIO_EXTRA_MODULES").is_none() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let error = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env_remove("GIO_EXTRA_MODULES")
        .exec();
    eprintln!("Ahab cannot restart without the host's GIO modules: {error}");
}
