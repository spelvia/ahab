//! Tauri's build step: the app's commands become permissions, so the
//! built-in screens' capability can allow them and a server's cannot.

fn main() {
    let commands: Vec<&str> = BRIDGE.iter().chain(BUILTIN).copied().collect();
    let commands: &'static [&str] = Box::leak(commands.into_boxed_slice());
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .unwrap_or_else(|error| panic!("tauri-build: {error:#}"));
}

const BRIDGE: &[&str] = &[
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

const BUILTIN: &[&str] = &[
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
    "agents_here",
    "open_in_browser",
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
