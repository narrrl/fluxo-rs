//! Client-side module command dispatch.
//!
//! Resolves CLI aliases (e.g. `mic` → audio source), delegates to
//! special-case handlers (BT menu), and falls through to the standard
//! IPC → daemon → Waybar JSON path for everything else.

/// Resolve client-side module aliases that prepend implicit arguments.
///
/// `vol` maps to the audio sink, `mic` maps to the audio source — both
/// dispatch to the `"vol"` module on the daemon with a `"sink"` or
/// `"source"` prefix argument.
fn resolve_alias(module: &str, args: &[String]) -> (String, Vec<String>) {
    match module {
        "vol" => {
            let mut a = vec!["sink".to_string()];
            a.extend(args.iter().cloned());
            ("vol".to_string(), a)
        }
        "mic" => {
            let mut a = vec!["source".to_string()];
            a.extend(args.iter().cloned());
            ("vol".to_string(), a)
        }
        _ => (module.to_string(), args.to_vec()),
    }
}

/// Entry point for all `fluxo <module> [args...]` invocations.
///
/// Handles the BT menu special case client-side, resolves aliases, and
/// sends the request to the daemon via IPC.
pub fn run_module_command(module: &str, args: &[String]) {
    // Bluetooth menu runs client-side because it spawns the user's menu
    // command (rofi/dmenu/wofi) — the daemon must not open GUI windows.
    #[cfg(feature = "mod-bt")]
    if module == "bt" && args.first().map(|s| s.as_str()) == Some("menu") {
        crate::bt_menu::run_bt_menu();
        return;
    }

    let (actual_module, actual_args) = resolve_alias(module, args);
    let args_ref: Vec<&str> = actual_args.iter().map(|s| s.as_str()).collect();
    crate::output::print_waybar_response(crate::ipc::request_data(&actual_module, &args_ref));
}
