use crate::config::Config;
use crate::error::FluxoError;
use crate::ipc::socket_path;
use crate::modules::WaybarModule;
use crate::modules::audio::AudioDaemon;
use crate::modules::backlight::BacklightDaemon;
use crate::modules::bt::BtDaemon;
use crate::modules::dnd::DndDaemon;
use crate::modules::hardware::HardwareDaemon;
use crate::modules::keyboard::KeyboardDaemon;
use crate::modules::mpris::MprisDaemon;
use crate::modules::network::NetworkDaemon;
use crate::signaler::WaybarSignaler;
use crate::state::AppReceivers;
use anyhow::Result;
use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::{Duration, Instant, sleep};
use tokio_util::sync::CancellationToken;
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

fn get_config_path(custom_path: Option<PathBuf>) -> PathBuf {
    custom_path.unwrap_or_else(|| {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
                PathBuf::from(home).join(".config")
            });
        config_dir.join("fluxo/config.toml")
    })
}

pub async fn run_daemon(config_path: Option<PathBuf>) -> Result<()> {
    let sock_path = socket_path();

    if fs::metadata(&sock_path).is_ok() {
        debug!("Removing stale socket file: {}", sock_path);
        fs::remove_file(&sock_path)?;
    }

    let (net_tx, net_rx) = watch::channel(Default::default());
    let (cpu_tx, cpu_rx) = watch::channel(Default::default());
    let (mem_tx, mem_rx) = watch::channel(Default::default());
    let (sys_tx, sys_rx) = watch::channel(Default::default());
    let (gpu_tx, gpu_rx) = watch::channel(Default::default());
    let (disks_tx, disks_rx) = watch::channel(Default::default());
    let (bt_tx, bt_rx) = watch::channel(Default::default());
    let (audio_tx, audio_rx) = watch::channel(Default::default());
    let (mpris_tx, mpris_rx) = watch::channel(Default::default());
    let (backlight_tx, backlight_rx) = watch::channel(Default::default());
    let (keyboard_tx, keyboard_rx) = watch::channel(Default::default());
    let (dnd_tx, dnd_rx) = watch::channel(Default::default());
    let health = Arc::new(RwLock::new(HashMap::new()));
    let (bt_force_tx, mut bt_force_rx) = mpsc::channel(1);
    let (audio_cmd_tx, audio_cmd_rx) = mpsc::channel(8);

    let receivers = AppReceivers {
        network: net_rx,
        cpu: cpu_rx,
        memory: mem_rx,
        sys: sys_rx,
        gpu: gpu_rx,
        disks: disks_rx,
        bluetooth: bt_rx,
        audio: audio_rx,
        mpris: mpris_rx,
        backlight: backlight_rx,
        keyboard: keyboard_rx,
        dnd: dnd_rx,
        health: Arc::clone(&health),
        bt_force_poll: bt_force_tx,
        audio_cmd_tx,
    };

    let listener = UnixListener::bind(&sock_path)?;
    let _guard = SocketGuard {
        path: sock_path.clone(),
    };

    // Signal handling for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Received shutdown signal, exiting...");
        token_clone.cancel();
    });

    let resolved_config_path = get_config_path(config_path.clone());
    let config = Arc::new(RwLock::new(crate::config::load_config(config_path.clone())));

    // 0. Config Watcher (Hot Reload)
    let watcher_config = Arc::clone(&config);
    let watcher_path = resolved_config_path.clone();
    tokio::spawn(async move {
        let (ev_tx, mut ev_rx) = mpsc::channel(1);
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res
                    && (event.kind.is_modify() || event.kind.is_create())
                {
                    let _ = ev_tx.blocking_send(());
                }
            },
            NotifyConfig::default(),
        )
        .unwrap();

        if let Some(parent) = watcher_path.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }

        info!("Config watcher started on {:?}", watcher_path);

        loop {
            tokio::select! {
                _ = ev_rx.recv() => {
                    // Debounce reloads
                    sleep(Duration::from_millis(100)).await;
                    while ev_rx.try_recv().is_ok() {}
                    reload_config(&watcher_config, Some(watcher_path.clone())).await;
                }
            }
        }
    });

    // 0.1 SIGHUP Handler
    let hup_config = Arc::clone(&config);
    let hup_path = resolved_config_path.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let mut stream = signal(SignalKind::hangup()).unwrap();
        loop {
            stream.recv().await;
            info!("Received SIGHUP, reloading config...");
            reload_config(&hup_config, Some(hup_path.clone())).await;
        }
    });

    // 1. Network Task
    let token = cancel_token.clone();
    let net_health = Arc::clone(&health);
    tokio::spawn(async move {
        info!("Starting Network polling task");
        let mut daemon = NetworkDaemon::new();
        loop {
            if !is_in_backoff("net", &net_health).await {
                let res = daemon.poll(&net_tx).await;
                handle_poll_result("net", res, &net_health).await;
            }
            tokio::select! {
                _ = token.cancelled() => break,
                _ = sleep(Duration::from_secs(1)) => {}
            }
        }
        info!("Network task shut down.");
    });

    // 2. Fast Hardware Task (CPU, Mem, Load)
    let token = cancel_token.clone();
    let hw_health = Arc::clone(&health);
    tokio::spawn(async move {
        info!("Starting Fast Hardware polling task");
        let mut daemon = HardwareDaemon::new();
        loop {
            if !is_in_backoff("cpu", &hw_health).await {
                daemon.poll_fast(&cpu_tx, &mem_tx, &sys_tx).await;
            }
            tokio::select! {
                _ = token.cancelled() => break,
                _ = sleep(Duration::from_secs(1)) => {}
            }
        }
        info!("Fast Hardware task shut down.");
    });

    // 3. Slow Hardware Task (GPU, Disks)
    let token = cancel_token.clone();
    let slow_health = Arc::clone(&health);
    tokio::spawn(async move {
        info!("Starting Slow Hardware polling task");
        let mut daemon = HardwareDaemon::new();
        loop {
            if !is_in_backoff("gpu", &slow_health).await {
                daemon.poll_slow(&gpu_tx, &disks_tx).await;
            }
            tokio::select! {
                _ = token.cancelled() => break,
                _ = sleep(Duration::from_secs(5)) => {}
            }
        }
        info!("Slow Hardware task shut down.");
    });

    // 4. Bluetooth Task
    let token = cancel_token.clone();
    let bt_health = Arc::clone(&health);
    let poll_config = Arc::clone(&config);
    let poll_receivers = receivers.clone();
    tokio::spawn(async move {
        info!("Starting Bluetooth polling task");
        let mut daemon = BtDaemon::new();
        loop {
            if !is_in_backoff("bt", &bt_health).await {
                let config = poll_config.read().await;
                daemon.poll(&bt_tx, &poll_receivers, &config).await;
            }
            tokio::select! {
                _ = token.cancelled() => break,
                _ = bt_force_rx.recv() => {},
                _ = sleep(Duration::from_secs(2)) => {}
            }
        }
        info!("Bluetooth task shut down.");
    });

    // 5. Audio Thread (Event driven)
    let audio_daemon = AudioDaemon::new();
    audio_daemon.start(&audio_tx, audio_cmd_rx);

    // 5.1 Backlight Thread (Event driven)
    let backlight_daemon = BacklightDaemon::new();
    backlight_daemon.start(backlight_tx);

    // 5.2 Keyboard Thread (Event driven)
    let keyboard_daemon = KeyboardDaemon::new();
    keyboard_daemon.start(keyboard_tx);

    // 5.3 DND Thread (Event driven)
    let dnd_daemon = DndDaemon::new();
    dnd_daemon.start(dnd_tx);

    // 5.4 MPRIS Thread
    let mpris_daemon = MprisDaemon::new();
    mpris_daemon.start(mpris_tx);

    // 6. Waybar Signaler Task
    let signaler = WaybarSignaler::new();
    let sig_config = Arc::clone(&config);
    let sig_receivers = receivers.clone();
    tokio::spawn(async move {
        info!("Starting Waybar Signaler task");
        signaler.run(sig_config, sig_receivers).await;
    });

    info!("Fluxo daemon successfully bound to socket: {}", sock_path);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((mut stream, _)) => {
                        let state_clone = receivers.clone();
                        let config_clone = Arc::clone(&config);
                        let cp_clone = config_path.clone();
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
                                    reload_config(&config_clone, cp_clone).await;
                                    let _ = writer.write_all(b"{\"text\":\"ok\"}").await;
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

async fn handle_poll_result(
    module_name: &str,
    result: crate::error::Result<()>,
    health_lock: &Arc<RwLock<HashMap<String, crate::state::ModuleHealth>>>,
) {
    let mut lock = health_lock.write().await;
    let health = lock.entry(module_name.to_string()).or_default();

    match result {
        Ok(_) => {
            if health.consecutive_failures > 0 {
                info!(
                    module = module_name,
                    "Module recovered after {} failures", health.consecutive_failures
                );
            }
            health.consecutive_failures = 0;
            health.backoff_until = None;
        }
        Err(e) => {
            health.consecutive_failures += 1;
            health.last_failure = Some(Instant::now());

            if !e.is_transient() {
                // Fatal errors trigger immediate long backoff
                health.backoff_until = Some(Instant::now() + Duration::from_secs(60));
                error!(module = module_name, error = %e, "Fatal module error, entering long cooldown");
            } else if health.consecutive_failures >= 3 {
                // Exponential backoff for transient errors: 30s, 60s, 120s...
                let backoff_secs = 30 * (2u64.pow(health.consecutive_failures.saturating_sub(3)));
                let backoff_secs = backoff_secs.min(3600); // Cap at 1 hour
                health.backoff_until = Some(Instant::now() + Duration::from_secs(backoff_secs));
                warn!(module = module_name, error = %e, backoff = backoff_secs, "Repeated transient failures, entering backoff");
            } else {
                debug!(module = module_name, error = %e, "Transient module failure (attempt {})", health.consecutive_failures);
            }
        }
    }
}

async fn is_in_backoff(
    module_name: &str,
    health_lock: &Arc<RwLock<HashMap<String, crate::state::ModuleHealth>>>,
) -> bool {
    let lock = health_lock.read().await;
    if let Some(health) = lock.get(module_name)
        && let Some(until) = health.backoff_until
    {
        return Instant::now() < until;
    }
    false
}

pub async fn reload_config(config_lock: &Arc<RwLock<Config>>, path: Option<PathBuf>) {
    info!("Reloading configuration...");
    let new_config = crate::config::load_config(path);
    let mut lock = config_lock.write().await;
    *lock = new_config;
    info!("Configuration reloaded successfully.");
}

pub async fn evaluate_module_for_signaler(
    module_name: &str,
    state: &AppReceivers,
    config: &Config,
) -> Option<String> {
    let result = match module_name {
        "net" | "network" => {
            crate::modules::network::NetworkModule
                .run(config, state, &[])
                .await
        }
        "cpu" => crate::modules::cpu::CpuModule.run(config, state, &[]).await,
        "mem" | "memory" => {
            crate::modules::memory::MemoryModule
                .run(config, state, &[])
                .await
        }
        "disk" => {
            crate::modules::disk::DiskModule
                .run(config, state, &["/"])
                .await
        }
        "pool" | "btrfs" => {
            crate::modules::btrfs::BtrfsModule
                .run(config, state, &["btrfs"])
                .await
        }
        "vol" | "audio" => {
            crate::modules::audio::AudioModule
                .run(config, state, &["sink", "show"])
                .await
        }
        "mic" => {
            crate::modules::audio::AudioModule
                .run(config, state, &["source", "show"])
                .await
        }
        "gpu" => crate::modules::gpu::GpuModule.run(config, state, &[]).await,
        "sys" => crate::modules::sys::SysModule.run(config, state, &[]).await,
        "bt" | "bluetooth" => {
            crate::modules::bt::BtModule
                .run(config, state, &["show"])
                .await
        }
        "power" => {
            crate::modules::power::PowerModule
                .run(config, state, &[])
                .await
        }
        "game" => {
            crate::modules::game::GameModule
                .run(config, state, &[])
                .await
        }
        "backlight" => {
            crate::modules::backlight::BacklightModule
                .run(config, state, &[])
                .await
        }
        "kbd" | "keyboard" => {
            crate::modules::keyboard::KeyboardModule
                .run(config, state, &[])
                .await
        }
        "dnd" => crate::modules::dnd::DndModule.run(config, state, &[]).await,
        "mpris" => {
            crate::modules::mpris::MprisModule
                .run(config, state, &[])
                .await
        }
        _ => return None,
    };
    result.ok().and_then(|out| serde_json::to_string(&out).ok())
}

async fn handle_request(
    module_name: &str,
    args: &[&str],
    state: &AppReceivers,
    config_lock: &Arc<RwLock<Config>>,
) -> String {
    // 1. Check Circuit Breaker status
    let (is_in_backoff, cached_output) = {
        let lock = state.health.read().await;
        if let Some(health) = lock.get(module_name) {
            let in_backoff = if let Some(until) = health.backoff_until {
                Instant::now() < until
            } else {
                false
            };
            (in_backoff, health.last_successful_output.clone())
        } else {
            (false, None)
        }
    };

    if is_in_backoff {
        if let Some(mut cached) = cached_output {
            // Add a "warning" class to indicate stale data
            let class = cached.class.unwrap_or_default();
            cached.class = Some(format!("{} warning", class).trim().to_string());
            return serde_json::to_string(&cached).unwrap_or_else(|_| "{}".to_string());
        }
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
        "vol" | "audio" => {
            crate::modules::audio::AudioModule
                .run(&config, state, args)
                .await
        }
        "mic" => {
            crate::modules::audio::AudioModule
                .run(&config, state, args)
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
        "backlight" => {
            crate::modules::backlight::BacklightModule
                .run(&config, state, args)
                .await
        }
        "kbd" | "keyboard" => {
            crate::modules::keyboard::KeyboardModule
                .run(&config, state, args)
                .await
        }
        "dnd" => {
            crate::modules::dnd::DndModule
                .run(&config, state, args)
                .await
        }
        "mpris" => {
            crate::modules::mpris::MprisModule
                .run(&config, state, args)
                .await
        }
        _ => {
            warn!("Received request for unknown module: '{}'", module_name);
            Err(FluxoError::Ipc(format!("Unknown module: {}", module_name)))
        }
    };

    // 2. Update Health and Cache based on result
    {
        let mut lock = state.health.write().await;
        let health = lock.entry(module_name.to_string()).or_default();
        match &result {
            Ok(output) => {
                health.consecutive_failures = 0;
                health.backoff_until = None;
                health.last_successful_output = Some(output.clone());
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
            // If we have a cached output, return it as fallback even on immediate error
            if let Some(mut cached) = cached_output {
                let class = cached.class.unwrap_or_default();
                cached.class = Some(format!("{} warning", class).trim().to_string());
                return serde_json::to_string(&cached).unwrap_or_else(|_| "{}".to_string());
            }

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
