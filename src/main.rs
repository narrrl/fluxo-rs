//! `fluxo` — high-performance daemon/client for Waybar custom modules.
//!
//! The binary has two faces:
//! * `fluxo daemon` — starts a long-lived process that polls system state
//!   (network, cpu, audio, bluetooth, …) on background tasks and exposes the
//!   results over a Unix socket. It also sends `SIGRTMIN+N` signals to Waybar
//!   when module output changes, so the bar refreshes instantly.
//! * `fluxo <module> [args]` — a tiny client that asks the daemon to evaluate
//!   a single module and prints the Waybar-compatible JSON to stdout.
//!
//! Modules are feature-gated at compile time (`mod-audio`, `mod-bt`, `mod-dbus`,
//! `mod-hardware`, `mod-network`) and registered centrally via the
//! [`for_each_watched_module!`] macro in [`mod@macros`].

#[macro_use]
mod macros;
mod config;
mod daemon;
mod error;
mod health;
mod ipc;
mod modules;
mod output;
mod registry;
mod signaler;
mod state;
mod utils;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser)]
#[command(name = "fluxo")]
#[command(about = "A high-performance daemon/client for Waybar custom modules", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Module name to query or interact with
    module: Option<String>,

    /// Arguments to pass to the module
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the background polling daemon
    Daemon {
        /// Optional custom path to config.toml
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Reload the daemon configuration
    Reload,
}

fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).pretty())
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    if let Some(command) = &cli.command {
        match command {
            Commands::Daemon { config } => {
                info!("Starting Fluxo daemon...");
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                if let Err(e) = rt.block_on(daemon::run_daemon(config.clone())) {
                    error!("Daemon failed: {}", e);
                    process::exit(1);
                }
            }
            Commands::Reload => match ipc::request_data("reload", &[]) {
                Ok(_) => info!("Reload signal sent to daemon"),
                Err(e) => {
                    error!("Failed to send reload signal: {}", e);
                    process::exit(1);
                }
            },
        }
        return;
    }

    if let Some(module) = &cli.module {
        // Bluetooth menu is handled client-side: it needs access to the user's
        // menu command (rofi/dmenu/wofi) which the daemon has no business spawning.
        #[cfg(feature = "mod-bt")]
        if module == "bt" && cli.args.first().map(|s| s.as_str()) == Some("menu") {
            let config = config::load_config(None);
            let mut items = Vec::new();

            // Ask the daemon for the device list; tuples are (alias, mac).
            let mut connected: Vec<(String, String)> = Vec::new();
            let mut paired: Vec<(String, String)> = Vec::new();

            if let Ok(json_str) = ipc::request_data("bt", &["menu_data"])
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

            for (alias, mac) in &connected {
                if let Ok(json_str) = ipc::request_data("bt", &["get_modes", mac])
                    && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str)
                    && let Some(modes_str) = val.get("text").and_then(|t| t.as_str())
                    && !modes_str.is_empty()
                {
                    for mode in modes_str.lines() {
                        items.push(format!("{}: Mode: {} [{}]", alias, mode, mac));
                    }
                }
                items.push(format!("Disconnect {} [{}]", alias, mac));
            }

            if !paired.is_empty() {
                items.push("--- Connect Device ---".to_string());
                for (alias, mac) in &paired {
                    items.push(format!("{} ({})", alias, mac));
                }
            }

            if !items.is_empty() {
                if let Ok(selected) =
                    utils::show_menu("BT Menu: ", &items, &config.general.menu_command)
                {
                    if selected.contains(": Mode: ") {
                        // Parse "<alias>: Mode: <mode> [<MAC>]".
                        if let Some(bracket_start) = selected.rfind('[')
                            && let Some(bracket_end) = selected.rfind(']')
                        {
                            let mac = &selected[bracket_start + 1..bracket_end];
                            if let Some(mode_start) = selected.find(": Mode: ") {
                                let mode =
                                    &selected[mode_start + ": Mode: ".len()..bracket_start - 1];
                                handle_ipc_response(ipc::request_data(
                                    "bt",
                                    &["set_mode", mode, mac],
                                ));
                            }
                        }
                    } else if selected.starts_with("Disconnect ") {
                        // Parse "Disconnect <alias> [<MAC>]".
                        if let Some(bracket_start) = selected.rfind('[')
                            && let Some(bracket_end) = selected.rfind(']')
                        {
                            let mac = &selected[bracket_start + 1..bracket_end];
                            handle_ipc_response(ipc::request_data("bt", &["disconnect", mac]));
                        }
                    } else if selected == "--- Connect Device ---" {
                        // section header
                    } else if let Some(mac_start) = selected.rfind('(')
                        && let Some(mac_end) = selected.rfind(')')
                    {
                        let mac = &selected[mac_start + 1..mac_end];
                        handle_ipc_response(ipc::request_data("bt", &["connect", mac]));
                    }
                }
            } else {
                info!("No Bluetooth options found.");
            }
            return;
        }

        // `vol` and `mic` both dispatch to the audio module; we just prepend
        // the "sink" / "source" argument so the server picks the right device.
        let (actual_module, actual_args) = if module == "vol" {
            let mut new_args = vec!["sink".to_string()];
            new_args.extend(cli.args.clone());
            ("vol".to_string(), new_args)
        } else if module == "mic" {
            let mut new_args = vec!["source".to_string()];
            new_args.extend(cli.args.clone());
            ("vol".to_string(), new_args)
        } else {
            (module.clone(), cli.args.clone())
        };

        let args_ref: Vec<&str> = actual_args.iter().map(|s| s.as_str()).collect();
        handle_ipc_response(ipc::request_data(&actual_module, &args_ref));
    } else {
        println!("Please specify a module or command. See --help.");
        process::exit(1);
    }
}

/// Post-process the daemon's response for direct output to Waybar.
///
/// Normal spaces are replaced with figure-spaces (U+2007) so Waybar's
/// proportional font does not jitter between updates, and the text is wrapped
/// in zero-width spaces (U+200B) as a cosmetic padding trick. Markup strings
/// (containing `<`) pass through untouched. On IPC failure an `error` output
/// is emitted and the client exits non-zero.
fn handle_ipc_response(response: anyhow::Result<String>) {
    match response {
        Ok(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(mut val) => {
                if let Some(text) = val.get_mut("text").and_then(|t| t.as_str()) {
                    let processed_text = if text.contains('<') {
                        text.to_string()
                    } else {
                        text.replace(' ', "\u{2007}")
                    };

                    let fixed_text = format!("\u{200B}{}\u{200B}", processed_text);
                    val["text"] = serde_json::Value::String(fixed_text);
                }
                println!("{}", serde_json::to_string(&val).unwrap());
            }
            Err(_) => println!("{}", json_str),
        },
        Err(e) => {
            let err_out = output::WaybarOutput {
                text: format!("\u{200B}Daemon offline ({})\u{200B}", e),
                tooltip: Some(e.to_string()),
                class: Some("error".to_string()),
                percentage: None,
            };
            println!("{}", serde_json::to_string(&err_out).unwrap());
            process::exit(1);
        }
    }
}
