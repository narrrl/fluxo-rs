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
#[cfg(feature = "mod-bt")]
mod bt_menu;
mod client;
mod config;
mod daemon;
mod error;
mod health;
mod help;
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
#[command(disable_help_subcommand = true)]
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
    /// Show detailed help for all modules or a specific module
    Help {
        /// Optional module name to show detailed help for
        module: Option<String>,
    },
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
            Commands::Help { module } => {
                help::print_help(module.as_deref());
            }
        }
        return;
    }

    if let Some(module) = &cli.module {
        client::run_module_command(module, &cli.args);
    } else {
        help::print_help(None);
    }
}
