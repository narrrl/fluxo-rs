//! Daemon entry point: orchestrates polling tasks, signal handling, config
//! hot-reloading, and the IPC server.
//!
//! Layout of [`run_daemon`]:
//!
//! 1. **Channels** — `watch::channel()` pairs for every module that pushes
//!    state from a background task.
//! 2. **Polling / event tasks** — one per module; each writes into its sender,
//!    the signaler and request handlers read from the matching receiver.
//! 3. **Config watchers** — filesystem notifier + `SIGHUP` handler refresh the
//!    [`Config`] in place so modules see updates immediately.
//! 4. **Signaler** — watches all state receivers and pokes Waybar.
//! 5. **IPC loop** — `UnixListener` accepting client requests; each connection
//!    dispatches to [`crate::registry::dispatch`] and returns JSON.

use crate::config::Config;
use crate::ipc::socket_path;
#[cfg(feature = "mod-audio")]
use crate::modules::audio::AudioDaemon;
#[cfg(feature = "mod-dbus")]
use crate::modules::backlight::BacklightDaemon;
#[cfg(feature = "mod-bt")]
use crate::modules::bt::BtDaemon;
#[cfg(feature = "mod-dbus")]
use crate::modules::dnd::DndDaemon;
#[cfg(feature = "mod-hardware")]
use crate::modules::hardware::HardwareDaemon;
#[cfg(feature = "mod-dbus")]
use crate::modules::keyboard::KeyboardDaemon;
#[cfg(feature = "mod-dbus")]
use crate::modules::mpris::MprisDaemon;
#[cfg(feature = "mod-network")]
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
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

/// Spawn a health-tracked polling loop.
///
/// Each iteration: skip if in backoff, else await `$poll_expr` and feed the
/// `Result` to [`crate::health::handle_poll_result`]. The loop breaks when
/// `$token` is cancelled.
macro_rules! spawn_poll_loop {
    ($name:expr, $interval:expr, $health:expr, $token:expr, $poll_expr:expr) => {
        tokio::spawn(async move {
            info!(concat!("Starting ", $name, " polling task"));
            loop {
                if !crate::health::is_poll_in_backoff($name, &$health).await {
                    let res: crate::error::Result<()> = $poll_expr.await;
                    crate::health::handle_poll_result($name, res, &$health).await;
                }
                tokio::select! {
                    _ = $token.cancelled() => break,
                    _ = sleep($interval) => {}
                }
            }
            info!(concat!($name, " task shut down."));
        })
    };
}

/// Spawn a health-tracked polling loop with an extra trigger channel.
///
/// Identical to [`spawn_poll_loop`] but `$trigger` can wake the loop early —
/// used by the Bluetooth daemon when a client forces an immediate refresh.
macro_rules! spawn_poll_loop_triggered {
    ($name:expr, $interval:expr, $health:expr, $token:expr, $trigger:expr, $poll_expr:expr) => {
        tokio::spawn(async move {
            info!(concat!("Starting ", $name, " polling task"));
            loop {
                if !crate::health::is_poll_in_backoff($name, &$health).await {
                    let res: crate::error::Result<()> = $poll_expr.await;
                    crate::health::handle_poll_result($name, res, &$health).await;
                }
                tokio::select! {
                    _ = $token.cancelled() => break,
                    _ = $trigger.recv() => {},
                    _ = sleep($interval) => {}
                }
            }
            info!(concat!($name, " task shut down."));
        })
    };
}

/// Spawn a polling loop with no health tracking.
///
/// Used for internal daemons (hardware fast/slow) whose poll functions are
/// infallible and whose failures don't drive client-visible backoff.
macro_rules! spawn_poll_loop_simple {
    ($name:expr, $interval:expr, $token:expr, $poll_expr:expr) => {
        tokio::spawn(async move {
            info!(concat!("Starting ", $name, " polling task"));
            loop {
                $poll_expr.await;
                tokio::select! {
                    _ = $token.cancelled() => break,
                    _ = sleep($interval) => {}
                }
            }
            info!(concat!($name, " task shut down."));
        })
    };
}

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
    custom_path.unwrap_or_else(crate::config::default_config_path)
}

/// Run the daemon to completion.
///
/// Sets up the socket, spawns all enabled module tasks, hooks up config
/// hot-reloading, and finally enters the IPC accept loop. Returns only on
/// a fatal error or `Ctrl+C`.
pub async fn run_daemon(config_path: Option<PathBuf>) -> Result<()> {
    let sock_path = socket_path();

    if fs::metadata(&sock_path).is_ok() {
        debug!("Removing stale socket file: {}", sock_path);
        fs::remove_file(&sock_path)?;
    }

    #[cfg(feature = "mod-network")]
    let (net_tx, net_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-hardware")]
    let (cpu_tx, cpu_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-hardware")]
    let (mem_tx, mem_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-hardware")]
    let (sys_tx, sys_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-hardware")]
    let (gpu_tx, gpu_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-hardware")]
    let (disks_tx, disks_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-bt")]
    let (bt_tx, bt_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-audio")]
    let (audio_tx, audio_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-dbus")]
    let (mpris_tx, mpris_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-dbus")]
    let (backlight_tx, backlight_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-dbus")]
    let (keyboard_tx, keyboard_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-dbus")]
    let (dnd_tx, dnd_rx) = watch::channel(Default::default());
    #[cfg(feature = "mod-dbus")]
    let mpris_scroll = Arc::new(RwLock::new(crate::state::MprisScrollState::default()));
    #[cfg(feature = "mod-dbus")]
    let (mpris_scroll_tick_tx, mpris_scroll_tick_rx) = watch::channel(0u64);
    let health = Arc::new(RwLock::new(HashMap::new()));
    #[cfg(feature = "mod-bt")]
    let (bt_force_tx, mut bt_force_rx) = mpsc::channel(1);
    #[cfg(feature = "mod-bt")]
    let bt_cycle = Arc::new(RwLock::new(0usize));
    #[cfg(feature = "mod-audio")]
    let (audio_cmd_tx, audio_cmd_rx) = mpsc::channel(8);

    let receivers = AppReceivers {
        #[cfg(feature = "mod-network")]
        network: net_rx,
        #[cfg(feature = "mod-hardware")]
        cpu: cpu_rx,
        #[cfg(feature = "mod-hardware")]
        memory: mem_rx,
        #[cfg(feature = "mod-hardware")]
        sys: sys_rx,
        #[cfg(feature = "mod-hardware")]
        gpu: gpu_rx,
        #[cfg(feature = "mod-hardware")]
        disks: disks_rx,
        #[cfg(feature = "mod-bt")]
        bluetooth: bt_rx,
        #[cfg(feature = "mod-bt")]
        bt_cycle,
        #[cfg(feature = "mod-audio")]
        audio: audio_rx,
        #[cfg(feature = "mod-dbus")]
        mpris: mpris_rx,
        #[cfg(feature = "mod-dbus")]
        backlight: backlight_rx,
        #[cfg(feature = "mod-dbus")]
        keyboard: keyboard_rx,
        #[cfg(feature = "mod-dbus")]
        dnd: dnd_rx,
        #[cfg(feature = "mod-dbus")]
        mpris_scroll: Arc::clone(&mpris_scroll),
        #[cfg(feature = "mod-dbus")]
        mpris_scroll_tick: mpris_scroll_tick_rx,
        health: Arc::clone(&health),
        #[cfg(feature = "mod-bt")]
        bt_force_poll: bt_force_tx,
        #[cfg(feature = "mod-audio")]
        audio_cmd_tx,
    };

    let listener = UnixListener::bind(&sock_path)?;
    let _guard = SocketGuard {
        path: sock_path.clone(),
    };

    // Ctrl+C triggers a graceful shutdown by cancelling this token; every
    // spawned polling task checks it in its `select!`.
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Received shutdown signal, exiting...");
        token_clone.cancel();
    });

    let resolved_config_path = get_config_path(config_path.clone());
    let config = Arc::new(RwLock::new(crate::config::load_config(config_path.clone())));
    spawn_config_watchers(&config, &resolved_config_path);

    #[cfg(feature = "mod-network")]
    if config.read().await.network.enabled {
        let mut daemon = NetworkDaemon::new();
        let token = cancel_token.clone();
        let h = Arc::clone(&health);
        spawn_poll_loop!(
            "net",
            Duration::from_secs(1),
            h,
            token,
            daemon.poll(&net_tx)
        );
    }

    // Fast-cycle hardware (cpu/mem/load) polled at 1 Hz.
    #[cfg(feature = "mod-hardware")]
    {
        let cfg = config.read().await;
        let fast_enabled = cfg.cpu.enabled || cfg.memory.enabled || cfg.sys.enabled;
        drop(cfg);
        if fast_enabled {
            let mut daemon = HardwareDaemon::new();
            let token = cancel_token.clone();
            spawn_poll_loop_simple!(
                "fast_hw",
                Duration::from_secs(1),
                token,
                daemon.poll_fast(&cpu_tx, &mem_tx, &sys_tx)
            );
        }
    }

    // Slow-cycle hardware (gpu/disks) polled every 5 s — expensive to sample.
    #[cfg(feature = "mod-hardware")]
    {
        let cfg = config.read().await;
        let slow_enabled = cfg.gpu.enabled || cfg.disk.enabled;
        drop(cfg);
        if slow_enabled {
            let mut daemon = HardwareDaemon::new();
            let token = cancel_token.clone();
            spawn_poll_loop_simple!(
                "slow_hw",
                Duration::from_secs(5),
                token,
                daemon.poll_slow(&gpu_tx, &disks_tx)
            );
        }
    }

    #[cfg(feature = "mod-bt")]
    if config.read().await.bt.enabled {
        let mut daemon = BtDaemon::new();
        let token = cancel_token.clone();
        let h = Arc::clone(&health);
        let poll_config = Arc::clone(&config);
        let poll_receivers = receivers.clone();
        spawn_poll_loop_triggered!("bt", Duration::from_secs(2), h, token, bt_force_rx, async {
            let config = poll_config.read().await;
            daemon.poll(&bt_tx, &poll_receivers, &config).await;
            Ok(())
        });
    }

    // Event-driven subsystems — these spawn their own threads internally and
    // push into their watch sender as events arrive (no polling loop).
    #[cfg(feature = "mod-audio")]
    if config.read().await.audio.enabled {
        let audio_daemon = AudioDaemon::new();
        audio_daemon.start(&audio_tx, audio_cmd_rx);
    }

    #[cfg(feature = "mod-dbus")]
    if config.read().await.backlight.enabled {
        let backlight_daemon = BacklightDaemon::new();
        backlight_daemon.start(backlight_tx);
    }

    #[cfg(feature = "mod-dbus")]
    if config.read().await.keyboard.enabled {
        let keyboard_daemon = KeyboardDaemon::new();
        keyboard_daemon.start(keyboard_tx);
    }

    #[cfg(feature = "mod-dbus")]
    if config.read().await.dnd.enabled {
        let dnd_daemon = DndDaemon::new();
        dnd_daemon.start(dnd_tx);
    }

    #[cfg(feature = "mod-dbus")]
    if config.read().await.mpris.enabled {
        let mpris_daemon = MprisDaemon::new();
        mpris_daemon.start(mpris_tx);

        // Ticks the scroll offset forward for the marquee animation.
        let scroll_config = Arc::clone(&config);
        let scroll_rx = receivers.mpris.clone();
        let scroll_state = Arc::clone(&mpris_scroll);
        tokio::spawn(async move {
            crate::modules::mpris::mpris_scroll_ticker(
                scroll_config,
                scroll_rx,
                scroll_state,
                mpris_scroll_tick_tx,
            )
            .await;
        });
    }

    let signaler = WaybarSignaler::new();
    let sig_config = Arc::clone(&config);
    let sig_receivers = receivers.clone();
    tokio::spawn(async move {
        info!("Starting Waybar Signaler task");
        signaler.run(sig_config, sig_receivers).await;
    });

    info!("Fluxo daemon successfully bound to socket: {}", sock_path);
    run_ipc_loop(listener, receivers, config, config_path, cancel_token).await
}

/// Spawn background tasks that hot-reload the daemon's [`Config`].
///
/// Installs a `notify`-based filesystem watcher on the config file's parent
/// directory, plus a `SIGHUP` handler — either triggers a reload of the
/// shared `Arc<RwLock<Config>>`.
fn spawn_config_watchers(config: &Arc<RwLock<Config>>, resolved_path: &std::path::Path) {
    // `notify` recursively tracks the parent dir so atomic-write editors
    // (which rename a new file into place) still get picked up.
    let watcher_config = Arc::clone(config);
    let watcher_path = resolved_path.to_path_buf();
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
                    // Coalesce rapid editor writes into one reload.
                    sleep(Duration::from_millis(100)).await;
                    while ev_rx.try_recv().is_ok() {}
                    reload_config(&watcher_config, Some(watcher_path.clone())).await;
                }
            }
        }
    });

    let hup_config = Arc::clone(config);
    let hup_path = resolved_path.to_path_buf();
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let mut stream = signal(SignalKind::hangup()).unwrap();
        loop {
            stream.recv().await;
            info!("Received SIGHUP, reloading config...");
            reload_config(&hup_config, Some(hup_path.clone())).await;
        }
    });
}

/// Accept loop for the client Unix socket.
///
/// Each client request spawns a short-lived task that reads one line, looks
/// up the module via [`crate::registry::dispatch`], and writes the JSON
/// response back. Broken-pipe errors are logged at `debug` — they just mean
/// the client timed out before we responded.
async fn run_ipc_loop(
    listener: UnixListener,
    receivers: AppReceivers,
    config: Arc<RwLock<Config>>,
    config_path: Option<PathBuf>,
    cancel_token: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => break,
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

/// Re-read the configuration file and swap it into the shared lock.
pub async fn reload_config(config_lock: &Arc<RwLock<Config>>, path: Option<PathBuf>) {
    info!("Reloading configuration...");
    let new_config = crate::config::load_config(path);
    let mut lock = config_lock.write().await;
    *lock = new_config;
    info!("Configuration reloaded successfully.");
}

/// Evaluate a module with its signaler-default args and return the JSON body.
///
/// Used by the [`crate::signaler`] to decide whether the module's output has
/// actually changed before sending Waybar a signal.
pub async fn evaluate_module_for_signaler(
    module_name: &str,
    state: &AppReceivers,
    config: &Config,
) -> Option<String> {
    let args = crate::registry::signaler_default_args(module_name);
    crate::registry::dispatch(module_name, config, state, args)
        .await
        .ok()
        .and_then(|out| serde_json::to_string(&out).ok())
}

async fn handle_request(
    module_name: &str,
    args: &[&str],
    state: &AppReceivers,
    config_lock: &Arc<RwLock<Config>>,
) -> String {
    let (is_in_backoff, cached_output) = crate::health::check_backoff(module_name, state).await;

    if is_in_backoff {
        return crate::health::backoff_response(module_name, cached_output);
    }

    let config = config_lock.read().await;
    let result = crate::registry::dispatch(module_name, &config, state, args).await;

    crate::health::update_health(module_name, &result, state).await;

    match result {
        Ok(output) => serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string()),
        Err(crate::error::FluxoError::Disabled(_)) => {
            serde_json::to_string(&crate::output::WaybarOutput::disabled())
                .unwrap_or_else(|_| "{}".to_string())
        }
        Err(e) => crate::health::error_response(module_name, &e, cached_output),
    }
}
