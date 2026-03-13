use crate::config::Config;
use crate::ipc::SOCKET_PATH;
use crate::modules::network::NetworkDaemon;
use crate::modules::hardware::HardwareDaemon;
use crate::modules::WaybarModule;
use crate::state::{AppState, SharedState};
use anyhow::Result;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tracing::{info, warn, error, debug};
use std::path::PathBuf;

pub fn run_daemon(config_path: Option<PathBuf>) -> Result<()> {
    if fs::metadata(SOCKET_PATH).is_ok() {
        debug!("Removing stale socket file: {}", SOCKET_PATH);
        fs::remove_file(SOCKET_PATH)?;
    }

    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    let listener = UnixListener::bind(SOCKET_PATH)?;
    
    // We store the original config_path to allow proper reloading later
    let config_path_clone = config_path.clone();
    let config = Arc::new(RwLock::new(crate::config::load_config(config_path)));

    let poll_state = Arc::clone(&state);
    thread::spawn(move || {
        info!("Starting background polling thread");
        let mut network_daemon = NetworkDaemon::new();
        let mut hardware_daemon = HardwareDaemon::new();
        loop {
            network_daemon.poll(Arc::clone(&poll_state));
            hardware_daemon.poll(Arc::clone(&poll_state));
            thread::sleep(Duration::from_secs(1));
        }
    });

    info!("Fluxo daemon successfully bound to socket: {}", SOCKET_PATH);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let state_clone = Arc::clone(&state);
                let config_clone = Arc::clone(&config);
                let cp_clone = config_path_clone.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    if let Err(e) = reader.read_line(&mut request) {
                        error!("Failed to read from IPC stream: {}", e);
                        return;
                    }
                    
                    let request = request.trim();
                    if request.is_empty() { return; }

                    let parts: Vec<&str> = request.split_whitespace().collect();
                    if let Some(module_name) = parts.first() {
                        if *module_name == "reload" {
                            info!("Reloading configuration...");
                            let new_config = crate::config::load_config(cp_clone);
                            if let Ok(mut config_lock) = config_clone.write() {
                                *config_lock = new_config;
                                let _ = stream.write_all(b"{\"text\":\"ok\"}");
                                info!("Configuration reloaded successfully.");
                            } else {
                                error!("Failed to acquire write lock for configuration reload.");
                            }
                            return;
                        }

                        debug!(module = module_name, args = ?&parts[1..], "Handling IPC request");
                        let response = handle_request(*module_name, &parts[1..], &state_clone, &config_clone);
                        if let Err(e) = stream.write_all(response.as_bytes()) {
                            error!("Failed to write IPC response: {}", e);
                        }
                    }
                });
            }
            Err(e) => error!("Failed to accept incoming connection: {}", e),
        }
    }

    Ok(())
}

fn handle_request(module_name: &str, args: &[&str], state: &SharedState, config_lock: &Arc<RwLock<Config>>) -> String {
    let config = if let Ok(c) = config_lock.read() {
        c
    } else {
        error!("Failed to acquire read lock for configuration.");
        return "{\"text\":\"error: config lock failed\"}".to_string();
    };

    let result = match module_name {
        "net" | "network" => crate::modules::network::NetworkModule.run(&config, state, args),
        "cpu" => crate::modules::cpu::CpuModule.run(&config, state, args),
        "mem" | "memory" => crate::modules::memory::MemoryModule.run(&config, state, args),
        "disk" => crate::modules::disk::DiskModule.run(&config, state, args),
        "pool" | "btrfs" => crate::modules::btrfs::BtrfsModule.run(&config, state, args),
        "vol" => crate::modules::audio::AudioModule.run(&config, state, &["sink", args.get(0).unwrap_or(&"show")]),
        "mic" => crate::modules::audio::AudioModule.run(&config, state, &["source", args.get(0).unwrap_or(&"show")]),
        "gpu" => crate::modules::gpu::GpuModule.run(&config, state, args),
        "sys" => crate::modules::sys::SysModule.run(&config, state, args),
        "bt" | "bluetooth" => crate::modules::bt::BtModule.run(&config, state, args),
        "buds" => crate::modules::buds::BudsModule.run(&config, state, args),
        "power" => crate::modules::power::PowerModule.run(&config, state, args),
        "game" => crate::modules::game::GameModule.run(&config, state, args),
        _ => {
            warn!("Received request for unknown module: '{}'", module_name);
            Err(anyhow::anyhow!("Unknown module: {}", module_name))
        },
    };

    match result {
        Ok(output) => serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string()),
        Err(e) => {
            error!(module = module_name, error = %e, "Module execution failed");
            let err_out = crate::output::WaybarOutput {
                text: format!("\u{200B}Error\u{200B}"),
                tooltip: Some(e.to_string()),
                class: Some("error".to_string()),
                percentage: None,
            };
            serde_json::to_string(&err_out).unwrap_or_else(|_| "{}".to_string())
        }
    }
}
