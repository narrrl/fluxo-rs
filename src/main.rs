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

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Clone, ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Parser)]
#[command(name = "fluxo")]
#[command(about = "A high-performance daemon/client for Waybar custom modules", long_about = None)]
#[command(disable_help_subcommand = true, disable_help_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Print help information
    #[arg(short, long, global = true)]
    help: bool,

    /// Set the log level (trace, debug, info, warn, error)
    #[arg(long, global = true, value_enum)]
    loglevel: Option<LogLevel>,

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
    let cli = Cli::parse();

    // Explicit --loglevel takes priority, then RUST_LOG env var, then a
    // sensible default: INFO for the daemon, WARN for client commands.
    let default_level = if let Some(level) = &cli.loglevel {
        tracing::Level::from(level.clone())
    } else if matches!(&cli.command, Some(Commands::Daemon { .. })) {
        tracing::Level::INFO
    } else {
        tracing::Level::WARN
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).pretty())
        .with(EnvFilter::from_default_env().add_directive(default_level.into()))
        .init();

    if cli.help {
        help::print_help(None);
        return;
    }

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
