//! Google Maestro (PixelBuds GATT) integration.
//!
//! Each connected device gets its own [`buds_task`] running on a dedicated
//! single-threaded runtime. The task opens an RFCOMM channel, speaks the
//! Maestro protocol to read battery + ANC state, and listens for settings
//! changes. External callers interact via [`MaestroManager::send_command`]
//! and [`MaestroManager::get_status`].

use crate::state::AppReceivers;
use anyhow::{Context, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use maestro::protocol::codec::Codec;
use maestro::pwrpc::client::Client;
use maestro::service::MaestroService;
use maestro::service::settings::{self, SettingValue};

/// Cached per-device snapshot returned to BT plugin consumers.
#[derive(Clone, Default)]
pub struct BudsStatus {
    pub left_battery: Option<u8>,
    pub right_battery: Option<u8>,
    pub case_battery: Option<u8>,
    pub anc_state: String,
    #[allow(dead_code)]
    pub last_update: Option<Instant>,
    pub error: Option<String>,
}

/// Command that can be issued against a connected buds device.
pub enum BudsCommand {
    /// Set the ANC mode: `active`, `aware`, or `off`.
    SetAnc(String),
}

/// Messages sent to the [`MaestroManager`] control thread.
pub enum ManagerCommand {
    /// Ensure a [`buds_task`] is running for `mac`; spawn if absent.
    EnsureTask(String),
    /// Forward a [`BudsCommand`] to the task for `mac`.
    SendCommand(String, BudsCommand),
}

/// Owns all buds-task lifetimes and a shared status cache.
pub struct MaestroManager {
    statuses: Arc<Mutex<HashMap<String, BudsStatus>>>,
    management_tx: mpsc::UnboundedSender<ManagerCommand>,
}

impl MaestroManager {
    /// Spawn the management thread + runtime and return a handle.
    pub fn new(state: AppReceivers) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<ManagerCommand>();
        let statuses = Arc::new(Mutex::new(HashMap::new()));
        let statuses_clone = Arc::clone(&statuses);
        let state_clone = state.clone();

        // Dedicated thread — bluer uses per-thread local tasks.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = tokio::task::LocalSet::new();

            local.block_on(&rt, async move {
                let mut command_txs: HashMap<String, mpsc::Sender<BudsCommand>> = HashMap::new();

                loop {
                    tokio::select! {
                        Some(cmd) = rx.recv() => {
                            match cmd {
                                ManagerCommand::EnsureTask(mac) => {
                                    if !command_txs.contains_key(&mac) {
                                        let (tx, buds_rx) = mpsc::channel::<BudsCommand>(10);
                                        command_txs.insert(mac.clone(), tx);

                                        let mac_clone = mac.clone();
                                        let st_clone = Arc::clone(&statuses_clone);
                                        let state_inner = state_clone.clone();

                                        tokio::task::spawn_local(async move {
                                            if let Err(e) = buds_task(&mac_clone, st_clone, buds_rx, state_inner).await {
                                                error!("Buds task for {} failed: {}", mac_clone, e);
                                            }
                                        });
                                    }
                                }
                                ManagerCommand::SendCommand(mac, buds_cmd) => {
                                    if let Some(tx) = command_txs.get(&mac) {
                                        let _ = tx.try_send(buds_cmd);
                                    }
                                }
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {
                            // Wake tick: future hook for task-lifecycle cleanup.
                        }
                    }
                }
            });
        });

        Self {
            statuses,
            management_tx: tx,
        }
    }

    /// Return the cached [`BudsStatus`] for `mac` (default if absent).
    pub fn get_status(&self, mac: &str) -> BudsStatus {
        let statuses = self.statuses.lock().unwrap();
        statuses.get(mac).cloned().unwrap_or_default()
    }

    /// Request that a buds task be running for `mac`. Idempotent.
    pub fn ensure_task(&self, mac: &str) {
        let _ = self
            .management_tx
            .send(ManagerCommand::EnsureTask(mac.to_string()));
    }

    /// Ensure a task exists and forward `cmd` to it.
    pub fn send_command(&self, mac: &str, cmd: BudsCommand) -> Result<()> {
        self.ensure_task(mac);
        let _ = self
            .management_tx
            .send(ManagerCommand::SendCommand(mac.to_string(), cmd));
        Ok(())
    }
}

/// Per-device async task: opens RFCOMM, runs the Maestro codec, mirrors
/// battery/ANC state into the shared status map, and consumes commands.
async fn buds_task(
    mac: &str,
    statuses: Arc<Mutex<HashMap<String, BudsStatus>>>,
    mut rx: mpsc::Receiver<BudsCommand>,
    state: AppReceivers,
) -> Result<()> {
    info!("Starting native Maestro connection task for {}", mac);

    loop {
        let addr: bluer::Address = match mac.parse() {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to parse MAC address {}: {}", mac, e);
                return Err(e.into());
            }
        };
        let session = bluer::Session::new()
            .await
            .context("Failed to create bluer session")?;
        let adapter = session
            .default_adapter()
            .await
            .context("Failed to get default adapter")?;
        let device = adapter
            .device(addr)
            .context("Failed to get device handle")?;

        if !device.is_connected().await.unwrap_or(false) {
            debug!("Device {} not connected to BT, stopping maestro task", mac);
            break;
        }

        // Maestro historically listens on channel 1 or 2 — probe both.
        let mut stream = None;
        for channel in [1, 2] {
            let socket = match bluer::rfcomm::Socket::new() {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to create RFCOMM socket: {}", e);
                    return Err(e.into());
                }
            };
            let target = bluer::rfcomm::SocketAddr::new(addr, channel);
            debug!(
                "Trying to connect RFCOMM to {} on channel {}...",
                mac, channel
            );
            match socket.connect(target).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => {
                    debug!("Failed to connect to channel {}: {}", channel, e);
                }
            }
        }

        let stream = match stream {
            Some(s) => s,
            None => {
                warn!(
                    "Failed to connect RFCOMM to {} on any common channel. Retrying in 15s...",
                    mac
                );
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            }
        };

        info!("Connected Maestro RFCOMM to {} on channel", mac);

        let codec = Codec::new();
        let stream = codec.wrap(stream);
        let mut client = Client::new(stream);
        let handle = client.handle();

        let channel = match maestro::protocol::utils::resolve_channel(&mut client).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to resolve Maestro channel for {}: {}", mac, e);
                continue;
            }
        };

        tokio::spawn(async move {
            if let Err(e) = client.run().await {
                error!("Maestro client loop error: {}", e);
            }
        });

        let mut service = MaestroService::new(handle, channel);

        // Successful connect — clear health backoff for bt.buds.
        {
            let mut lock = state.health.write().await;
            let health = lock.entry("bt.buds".to_string()).or_default();
            health.consecutive_failures = 0;
            health.backoff_until = None;
        }

        if let Ok(val) = service
            .read_setting_var(settings::SettingId::CurrentAncrState)
            .await
            && let SettingValue::CurrentAncrState(anc_state) = val
        {
            let mut lock = statuses.lock().unwrap();
            let status = lock.entry(mac.to_string()).or_default();
            status.anc_state = anc_state_to_string(&anc_state);
            status.last_update = Some(Instant::now());
        }

        let mut runtime_info_call = match service.subscribe_to_runtime_info() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to subscribe to runtime info for {}: {}", mac, e);
                continue;
            }
        };

        let mut runtime_info = runtime_info_call.stream();

        let mut settings_changes_call = match service.subscribe_to_settings_changes() {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to subscribe to settings changes for {}: {}", mac, e);
                continue;
            }
        };

        let mut settings_changes = settings_changes_call.stream();

        debug!("Subscribed to status and settings for {}", mac);

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(BudsCommand::SetAnc(mode)) => {
                            debug!("Setting ANC mode to {} for {}", mode, mac);
                            let state = mode_to_anc_state(&mode);
                            let val = SettingValue::CurrentAncrState(state);

                            {
                                let mut lock = statuses.lock().unwrap();
                                let status = lock.entry(mac.to_string()).or_default();
                                status.anc_state = mode.clone();
                                status.last_update = Some(Instant::now());
                            }

                            if let Err(e) = service.write_setting(val).await {
                                error!("Failed to write ANC setting for {}: {}", mac, e);
                            }
                        }
                        None => return Ok(()),
                    }
                }
                Some(res) = runtime_info.next() => {
                    match res {
                        Ok(info) => {
                            let mut lock = statuses.lock().unwrap();
                            let status = lock.entry(mac.to_string()).or_default();
                            status.last_update = Some(Instant::now());

                            if let Some(bat) = info.battery_info {
                                status.left_battery = bat.left.map(|b| b.level as u8);
                                status.right_battery = bat.right.map(|b| b.level as u8);
                                status.case_battery = bat.case.map(|b| b.level as u8);
                            }
                        }
                        Err(e) => {
                            warn!("Runtime info stream error for {}: {}", mac, e);
                            break;
                        }
                    }
                }
                Some(res) = settings_changes.next() => {
                    if let Ok(change) = res {
                        use maestro::protocol::types::settings_rsp::ValueOneof as RspOneof;
                        use maestro::protocol::types::setting_value::ValueOneof as ValOneof;

                        if let Some(RspOneof::Value(setting_val)) = change.value_oneof
                            && let Some(ValOneof::CurrentAncrState(anc_state_raw)) = setting_val.value_oneof {
                                let mut lock = statuses.lock().unwrap();
                                let status = lock.entry(mac.to_string()).or_default();

                                let anc_state = match anc_state_raw {
                                    1 => settings::AncState::Off,
                                    2 => settings::AncState::Active,
                                    3 => settings::AncState::Aware,
                                    4 => settings::AncState::Adaptive,
                                    _ => settings::AncState::Unknown(anc_state_raw),
                                };

                                status.anc_state = anc_state_to_string(&anc_state);
                                status.last_update = Some(Instant::now());
                                debug!(mode = %status.anc_state, "Caught physical ANC toggle");
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    if !device.is_connected().await.unwrap_or(false) {
                        break;
                    }
                }
            }
        }

        if !device.is_connected().await.unwrap_or(false) {
            break;
        }
    }

    Ok(())
}

/// String ("active"/"aware"/"off") → Maestro enum; unknown falls back to `Off`.
fn mode_to_anc_state(mode: &str) -> settings::AncState {
    match mode {
        "active" => settings::AncState::Active,
        "aware" => settings::AncState::Aware,
        "off" => settings::AncState::Off,
        _ => settings::AncState::Off,
    }
}

/// Inverse of [`mode_to_anc_state`] for status readout.
pub fn anc_state_to_string(state: &settings::AncState) -> String {
    match state {
        settings::AncState::Active => "active".to_string(),
        settings::AncState::Aware => "aware".to_string(),
        settings::AncState::Off => "off".to_string(),
        _ => "unknown".to_string(),
    }
}

static MAESTRO: OnceLock<MaestroManager> = OnceLock::new();

/// Lazily initialise the process-wide [`MaestroManager`] and return a reference.
pub fn get_maestro(state: &AppReceivers) -> &MaestroManager {
    MAESTRO.get_or_init(|| MaestroManager::new(state.clone()))
}
