use crate::config::Config;
use crate::error::FluxoError;
use crate::ipc::socket_path;
use crate::modules::WaybarModule;
use crate::modules::audio::AudioDaemon;
use crate::modules::bt::BtDaemon;
use crate::modules::hardware::HardwareDaemon;
use crate::modules::network::NetworkDaemon;
use crate::state::{AppState, SharedState};
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, error, info, warn};

struct SocketGuard {
    path: String,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        debug!("Cleaning up socket file: {}", self.path);
        let _ = fs::remove_file(&self.path);
    }
}

pub async fn run_daemon(config_path: Option<PathBuf>) -> Result<()> {
    let sock_path = socket_path();

    if fs::metadata(&sock_path).is_ok() {
        debug!("Removing stale socket file: {}", sock_path);
        fs::remove_file(&sock_path)?;
    }

    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    let listener = UnixListener::bind(&sock_path)?;
    let _guard = SocketGuard {
        path: sock_path.clone(),
    };

    // Signal handling for graceful shutdown in async context
    let running = Arc::new(tokio::sync::Notify::new());
    let r_clone = Arc::clone(&running);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Received shutdown signal, exiting...");
        r_clone.notify_waiters();
    });

    let config_path_clone = config_path.clone();
    let config = Arc::new(RwLock::new(crate::config::load_config(config_path)));

    // 1. Network Task
    let poll_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("Starting Network polling task");
        let mut daemon = NetworkDaemon::new();
        loop {
            daemon.poll(Arc::clone(&poll_state)).await;
            sleep(Duration::from_secs(1)).await;
        }
    });

    // 2. Fast Hardware Task (CPU, Mem, Load)
    let poll_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("Starting Fast Hardware polling task");
        let mut daemon = HardwareDaemon::new();
        loop {
            daemon.poll_fast(Arc::clone(&poll_state)).await;
            sleep(Duration::from_secs(1)).await;
        }
    });

    // 3. Slow Hardware Task (GPU, Disks)
    let poll_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("Starting Slow Hardware polling task");
        let mut daemon = HardwareDaemon::new();
        loop {
            daemon.poll_slow(Arc::clone(&poll_state)).await;
            sleep(Duration::from_secs(1)).await;
        }
    });

    // 4. Bluetooth Task
    let poll_state = Arc::clone(&state);
    let poll_config = Arc::clone(&config);
    tokio::spawn(async move {
        info!("Starting Bluetooth polling task");
        let mut daemon = BtDaemon::new();
        loop {
            let config = poll_config.read().await;
            daemon.poll(Arc::clone(&poll_state), &config).await;
            sleep(Duration::from_secs(1)).await;
        }
    });

    // 5. Audio Thread (Event driven - pulse usually needs its own thread)
    let audio_daemon = AudioDaemon::new();
    audio_daemon.start(Arc::clone(&state));

    info!("Fluxo daemon successfully bound to socket: {}", sock_path);

    loop {
        tokio::select! {
            _ = running.notified() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((mut stream, _)) => {
                        let state_clone = Arc::clone(&state);
                        let config_clone = Arc::clone(&config);
                        let cp_clone = config_path_clone.clone();
                        tokio::spawn(async move {
                            let (reader, mut writer) = stream.split();
                            let mut reader = BufReader::new(reader);
                            let mut request = String::new();
                            if let Err(e) = reader.read_line(&mut request).await {
                                error!("Failed to read from IPC stream: {}", e);
                                return;
                            }

                            let request = request.trim();
                            if request.is_empty() {
                                return;
                            }

                            let parts: Vec<&str> = request.split_whitespace().collect();
                            if let Some(module_name) = parts.first() {
                                if *module_name == "reload" {
                                    info!("Reloading configuration...");
                                    let new_config = crate::config::load_config(cp_clone);
                                    let mut config_lock = config_clone.write().await;
                                    *config_lock = new_config;
                                    let _ = writer.write_all(b"{\"text\":\"ok\"}").await;
                                    info!("Configuration reloaded successfully.");
                                    return;
                                }

                                debug!(module = module_name, args = ?&parts[1..], "Handling IPC request");
                                let response =
                                    handle_request(module_name, &parts[1..], &state_clone, &config_clone).await;
                                if let Err(e) = writer.write_all(response.as_bytes()).await {
                                    if e.kind() == std::io::ErrorKind::BrokenPipe
                                        || e.kind() == std::io::ErrorKind::ConnectionReset
                                    {
                                        debug!(
                                            "IPC client disconnected before response could be sent: {}",
                                            e
                                        );
                                    } else {
                                        error!("Failed to write IPC response: {}", e);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => error!("Failed to accept incoming connection: {}", e),
                }
            }
        }
    }

    info!("Daemon shutting down gracefully.");
    Ok(())
}

async fn handle_request(
    module_name: &str,
    args: &[&str],
    state: &SharedState,
    config_lock: &Arc<RwLock<Config>>,
) -> String {
    // 1. Check Circuit Breaker status
    let is_in_backoff = {
        let lock = state.read().await;
        if let Some(health) = lock.health.get(module_name) {
            if let Some(until) = health.backoff_until {
                Instant::now() < until
            } else {
                false
            }
        } else {
            false
        }
    };

    if is_in_backoff {
        return format!(
            "{{\"text\":\"\u{200B}Cooling down ({})\u{200B}\",\"class\":\"error\"}}",
            module_name
        );
    }

    let config = config_lock.read().await;

    let result = match module_name {
        "net" | "network" => {
            crate::modules::network::NetworkModule
                .run(&config, state, args)
                .await
        }
        "cpu" => {
            crate::modules::cpu::CpuModule
                .run(&config, state, args)
                .await
        }
        "mem" | "memory" => {
            crate::modules::memory::MemoryModule
                .run(&config, state, args)
                .await
        }
        "disk" => {
            crate::modules::disk::DiskModule
                .run(&config, state, args)
                .await
        }
        "pool" | "btrfs" => {
            crate::modules::btrfs::BtrfsModule
                .run(&config, state, args)
                .await
        }
        "vol" => {
            crate::modules::audio::AudioModule
                .run(&config, state, &["sink", args.first().unwrap_or(&"show")])
                .await
        }
        "mic" => {
            crate::modules::audio::AudioModule
                .run(&config, state, &["source", args.first().unwrap_or(&"show")])
                .await
        }
        "gpu" => {
            crate::modules::gpu::GpuModule
                .run(&config, state, args)
                .await
        }
        "sys" => {
            crate::modules::sys::SysModule
                .run(&config, state, args)
                .await
        }
        "bt" | "bluetooth" => crate::modules::bt::BtModule.run(&config, state, args).await,
        "power" => {
            crate::modules::power::PowerModule
                .run(&config, state, args)
                .await
        }
        "game" => {
            crate::modules::game::GameModule
                .run(&config, state, args)
                .await
        }
        _ => {
            warn!("Received request for unknown module: '{}'", module_name);
            Err(FluxoError::Ipc(format!("Unknown module: {}", module_name)))
        }
    };

    // 2. Update Health based on result
    {
        let mut lock = state.write().await;
        let health = lock.health.entry(module_name.to_string()).or_default();
        match &result {
            Ok(_) => {
                health.consecutive_failures = 0;
                health.backoff_until = None;
            }
            Err(e) => {
                health.consecutive_failures += 1;
                health.last_failure = Some(Instant::now());
                if health.consecutive_failures >= 3 {
                    // Backoff for 30 seconds after 3 failures
                    health.backoff_until = Some(Instant::now() + Duration::from_secs(30));
                    warn!(module = module_name, error = %e, "Module entered backoff state due to repeated failures");
                }
            }
        }
    }

    match result {
        Ok(output) => serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string()),
        Err(e) => {
            let error_msg = e.to_string();
            error!(module = module_name, error = %error_msg, "Module execution failed");
            let err_out = crate::output::WaybarOutput {
                text: "\u{200B}Error\u{200B}".to_string(),
                tooltip: Some(error_msg),
                class: Some("error".to_string()),
                percentage: None,
            };
            serde_json::to_string(&err_out).unwrap_or_else(|_| "{}".to_string())
        }
    }
}
