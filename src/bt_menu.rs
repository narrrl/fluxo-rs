//! Bluetooth interactive menu (client-side).
//!
//! Runs entirely in the client process because it needs to spawn the user's
//! menu command (rofi/dmenu/wofi) — the daemon has no business opening GUI
//! windows. Communicates with the daemon via IPC to fetch device lists and
//! dispatch connect/disconnect/mode commands.

/// Format strings used both when *building* menu items and when *parsing*
/// the user's selection back. Keeping them together prevents drift.
mod fmt {
    /// Connected device with a plugin mode: `"<alias>: Mode: <mode> [<mac>]"`.
    pub const MODE_INFIX: &str = ": Mode: ";
    /// Disconnect action: `"Disconnect <alias> [<mac>]"`.
    pub const DISCONNECT_PREFIX: &str = "Disconnect ";
    /// Visual separator before paired-but-not-connected devices.
    pub const CONNECT_HEADER: &str = "--- Connect Device ---";
}

/// Extract a MAC address enclosed in `[…]` at the end of a string.
fn parse_mac_from_brackets(s: &str) -> Option<&str> {
    let start = s.rfind('[')?;
    let end = s.rfind(']')?;
    if end > start + 1 {
        Some(&s[start + 1..end])
    } else {
        None
    }
}

/// Extract a MAC address enclosed in `(…)` at the end of a string.
fn parse_mac_from_parens(s: &str) -> Option<&str> {
    let start = s.rfind('(')?;
    let end = s.rfind(')')?;
    if end > start + 1 {
        Some(&s[start + 1..end])
    } else {
        None
    }
}

/// Parse a mode selection line: `"<alias>: Mode: <mode> [<mac>]"`.
/// Returns `(mode, mac)`.
fn parse_mode_selection(s: &str) -> Option<(&str, &str)> {
    let mac = parse_mac_from_brackets(s)?;
    let mode_start = s.find(fmt::MODE_INFIX)?;
    let mode_begin = mode_start + fmt::MODE_INFIX.len();
    let bracket_start = s.rfind('[')?;
    if bracket_start > mode_begin {
        let mode = s[mode_begin..bracket_start].trim_end();
        Some((mode, mac))
    } else {
        None
    }
}

/// Run the interactive Bluetooth device menu.
///
/// Fetches connected/paired devices from the daemon, presents them in the
/// user's configured menu command, and dispatches the selected action back
/// to the daemon.
pub fn run_bt_menu() {
    let config = crate::config::load_config(None);
    let mut items = Vec::new();

    let mut connected: Vec<(String, String)> = Vec::new();
    let mut paired: Vec<(String, String)> = Vec::new();

    // Fetch the device list from the daemon.
    if let Ok(json_str) = crate::ipc::request_data("bt", &["menu_data"])
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str)
        && let Some(text) = val.get("text").and_then(|t| t.as_str())
    {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("CONNECTED:")
                && let Some((alias, mac)) = rest.split_once('|')
            {
                connected.push((alias.to_string(), mac.to_string()));
            } else if let Some(rest) = line.strip_prefix("PAIRED:")
                && let Some((alias, mac)) = rest.split_once('|')
            {
                paired.push((alias.to_string(), mac.to_string()));
            }
        }
    }

    // Build menu items for connected devices (modes + disconnect).
    for (alias, mac) in &connected {
        if let Ok(json_str) = crate::ipc::request_data("bt", &["get_modes", mac])
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str)
            && let Some(modes_str) = val.get("text").and_then(|t| t.as_str())
            && !modes_str.is_empty()
        {
            for mode in modes_str.lines() {
                items.push(format!("{}{}{} [{}]", alias, fmt::MODE_INFIX, mode, mac));
            }
        }
        items.push(format!("{}{} [{}]", fmt::DISCONNECT_PREFIX, alias, mac));
    }

    // Paired-but-not-connected devices go below a separator.
    if !paired.is_empty() {
        items.push(fmt::CONNECT_HEADER.to_string());
        for (alias, mac) in &paired {
            items.push(format!("{} ({})", alias, mac));
        }
    }

    if items.is_empty() {
        tracing::info!("No Bluetooth options found.");
        return;
    }

    let Ok(selected) = crate::utils::show_menu("BT Menu: ", &items, &config.general.menu_command)
    else {
        return;
    };

    if let Some((mode, mac)) = parse_mode_selection(&selected) {
        crate::output::print_waybar_response(crate::ipc::request_data(
            "bt",
            &["set_mode", mode, mac],
        ));
    } else if selected.starts_with(fmt::DISCONNECT_PREFIX) {
        if let Some(mac) = parse_mac_from_brackets(&selected) {
            crate::output::print_waybar_response(crate::ipc::request_data(
                "bt",
                &["disconnect", mac],
            ));
        }
    } else if selected == fmt::CONNECT_HEADER {
        // Section header — no action.
    } else if let Some(mac) = parse_mac_from_parens(&selected) {
        crate::output::print_waybar_response(crate::ipc::request_data("bt", &["connect", mac]));
    }
}
