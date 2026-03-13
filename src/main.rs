mod config;
mod daemon;
mod ipc;
mod modules;
mod output;
mod state;
mod utils;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "fluxo")]
#[command(about = "A high-performance daemon/client for Waybar custom modules", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
    /// Network speed module
    #[command(alias = "network")]
    Net,
    /// CPU usage and temp module
    Cpu,
    /// Memory usage module
    #[command(alias = "memory")]
    Mem,
    /// Disk usage module (path defaults to /)
    Disk {
        #[arg(default_value = "/")]
        path: String,
    },
    /// Storage pool aggregate module (e.g., btrfs)
    #[command(alias = "btrfs")]
    Pool {
        #[arg(default_value = "btrfs")]
        kind: String,
    },
    /// Audio volume (sink) control
    Vol {
        /// Cycle to the next available output device
        #[arg(short, long)]
        cycle: bool,
    },
    /// Microphone (source) control
    Mic {
        /// Cycle to the next available input device
        #[arg(short, long)]
        cycle: bool,
    },
    /// GPU usage, VRAM, and temp module
    Gpu,
    /// System load average and uptime
    Sys,
    /// Bluetooth audio device status
    #[command(alias = "bluetooth")]
    Bt {
        #[arg(default_value = "show")]
        action: String,
    },
    /// Pixel Buds Pro ANC and Battery
    Buds {
        #[arg(default_value = "show")]
        action: String,
    },
    /// System power and battery status
    Power,
    /// Hyprland gamemode status
    Game,
}

fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).pretty())
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Daemon { config } => {
            info!("Starting Fluxo daemon...");
            if let Err(e) = daemon::run_daemon(config.clone()) {
                error!("Daemon failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Reload => {
            match ipc::request_data("reload", &[]) {
                Ok(_) => info!("Reload signal sent to daemon"),
                Err(e) => {
                    error!("Failed to send reload signal: {}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Net => handle_ipc_response(ipc::request_data("net", &[])),
        Commands::Cpu => handle_ipc_response(ipc::request_data("cpu", &[])),
        Commands::Mem => handle_ipc_response(ipc::request_data("mem", &[])),
        Commands::Disk { path } => handle_ipc_response(ipc::request_data("disk", &[path.clone()])),
        Commands::Pool { kind } => handle_ipc_response(ipc::request_data("pool", &[kind.clone()])),
        Commands::Vol { cycle } => {
            let action = if *cycle { "cycle" } else { "show" };
            handle_ipc_response(ipc::request_data("vol", &[action.to_string()]));
        }
        Commands::Mic { cycle } => {
            let action = if *cycle { "cycle" } else { "show" };
            handle_ipc_response(ipc::request_data("mic", &[action.to_string()]));
        }
        Commands::Gpu => handle_ipc_response(ipc::request_data("gpu", &[])),
        Commands::Sys => handle_ipc_response(ipc::request_data("sys", &[])),
        Commands::Bt { action } => {
            if action == "menu" {
                // Client-side execution of the menu
                let config = config::load_config(None);
                
                let devices_out = std::process::Command::new("bluetoothctl")
                    .args(["devices"])
                    .output()
                    .expect("Failed to run bluetoothctl");
                let stdout = String::from_utf8_lossy(&devices_out.stdout);
                
                let mut items = Vec::new();
                for line in stdout.lines() {
                    if line.starts_with("Device ") {
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() == 3 {
                            items.push(format!("{} ({})", parts[2], parts[1]));
                        }
                    }
                }

                if !items.is_empty() {
                    if let Ok(selected) = utils::show_menu("Connect BT:", &items, &config.general.menu_command) {
                        if let Some(mac_start) = selected.rfind('(') {
                            if let Some(mac_end) = selected.rfind(')') {
                                let mac = &selected[mac_start + 1..mac_end];
                                let _ = std::process::Command::new("bluetoothctl")
                                    .args(["connect", mac])
                                    .status();
                            }
                        }
                    }
                } else {
                    info!("No paired Bluetooth devices found.");
                }
                return;
            }
            handle_ipc_response(ipc::request_data("bt", &[action.clone()]));
        }
        Commands::Buds { action } => handle_ipc_response(ipc::request_data("buds", &[action.clone()])),
        Commands::Power => handle_ipc_response(ipc::request_data("power", &[])),
        Commands::Game => handle_ipc_response(ipc::request_data("game", &[])),
    }
}

fn handle_ipc_response(response: anyhow::Result<String>) {
    match response {
        Ok(json_str) => {
            match serde_json::from_str::<serde_json::Value>(&json_str) {
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
            }
        }
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
