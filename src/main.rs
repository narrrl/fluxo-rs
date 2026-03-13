mod config;
mod daemon;
mod ipc;
mod modules;
mod output;
mod state;

use clap::{Parser, Subcommand};
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
    Daemon,
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
        Commands::Daemon => {
            info!("Starting Fluxo daemon...");
            if let Err(e) = daemon::run_daemon() {
                error!("Daemon failed: {}", e);
                process::exit(1);
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
        Commands::Bt { action } => handle_ipc_response(ipc::request_data("bt", &[action.clone()])),
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
                        // 1. Replace regular spaces with Figure Spaces (\u2007) for perfect numeric alignment
                        // 2. Wrap the text in Zero-Width Spaces (\u200B) to prevent Waybar from trimming
                        let fixed_text = format!("\u{200B}{}\u{200B}", text.replace(' ', "\u{2007}"));
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
