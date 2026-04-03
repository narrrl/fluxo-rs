mod config;
mod daemon;
mod error;
mod ipc;
mod modules;
mod output;
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
        // Special case for client-side Bluetooth menu which requires UI
        if module == "bt" && cli.args.first().map(|s| s.as_str()) == Some("menu") {
            let config = config::load_config(None);
            let mut items = Vec::new();

            if let Ok(json_str) = ipc::request_data("bt", &["get_modes"])
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str)
                && let Some(modes_str) = val.get("text").and_then(|t| t.as_str())
                && !modes_str.is_empty()
            {
                for mode in modes_str.lines() {
                    items.push(format!("Mode: {}", mode));
                }
            }

            if !items.is_empty() {
                items.push("Disconnect".to_string());
            }

            items.push("--- Connect Device ---".to_string());

            if let Ok(json_str) = ipc::request_data("bt", &["menu_data"])
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str)
                && let Some(devices_str) = val.get("text").and_then(|t| t.as_str())
            {
                for line in devices_str.lines() {
                    if !line.is_empty() {
                        items.push(line.to_string());
                    }
                }
            }

            if !items.is_empty() {
                if let Ok(selected) =
                    utils::show_menu("BT Menu: ", &items, &config.general.menu_command)
                {
                    if let Some(mode) = selected.strip_prefix("Mode: ") {
                        handle_ipc_response(ipc::request_data("bt", &["set_mode", mode]));
                    } else if selected == "Disconnect" {
                        handle_ipc_response(ipc::request_data("bt", &["disconnect"]));
                    } else if selected == "--- Connect Device ---" {
                        // Do nothing
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

        // Generic module dispatch
        // Translate module-specific shorthand targets
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
