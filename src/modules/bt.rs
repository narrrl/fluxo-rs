use crate::config::Config;
use crate::error::{FluxoError, Result as FluxoResult};
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{BtState, SharedState};
use crate::utils::{TokenValue, format_template};
use anyhow::{Context, Result};
use futures::StreamExt;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

// Maestro imports
#[allow(unused_imports)]
use maestro::protocol::codec::Codec;
#[allow(unused_imports)]
use maestro::pwrpc::client::{Client, ClientHandle};
#[allow(unused_imports)]
use maestro::service::MaestroService;
#[allow(unused_imports)]
use maestro::service::settings::{self, Setting, SettingValue};

#[derive(Clone, Default)]
struct BudsStatus {
    left_battery: Option<u8>,
    right_battery: Option<u8>,
    case_battery: Option<u8>,
    anc_state: String,
    #[allow(dead_code)]
    last_update: Option<Instant>,
    error: Option<String>,
}

enum BudsCommand {
    SetAnc(String),
}

enum ManagerCommand {
    EnsureTask(String),
    SendCommand(String, BudsCommand),
}

struct MaestroManager {
    statuses: Arc<Mutex<HashMap<String, BudsStatus>>>,
    management_tx: mpsc::UnboundedSender<ManagerCommand>,
}

impl MaestroManager {
    fn new(state: SharedState) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<ManagerCommand>();
        let statuses = Arc::new(Mutex::new(HashMap::new()));
        let statuses_clone = Arc::clone(&statuses);
        let state_clone = Arc::clone(&state);

        // Start dedicated BT management thread
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
                                        let state_inner = Arc::clone(&state_clone);

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
                            // Cleanup dropped tasks if needed
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

    fn get_status(&self, mac: &str) -> BudsStatus {
        let statuses = self.statuses.lock().unwrap();
        statuses.get(mac).cloned().unwrap_or_default()
    }

    fn ensure_task(&self, mac: &str) {
        let _ = self
            .management_tx
            .send(ManagerCommand::EnsureTask(mac.to_string()));
    }

    fn send_command(&self, mac: &str, cmd: BudsCommand) -> Result<()> {
        self.ensure_task(mac);
        let _ = self
            .management_tx
            .send(ManagerCommand::SendCommand(mac.to_string(), cmd));
        Ok(())
    }
}

async fn buds_task(
    mac: &str,
    statuses: Arc<Mutex<HashMap<String, BudsStatus>>>,
    mut rx: mpsc::Receiver<BudsCommand>,
    state: SharedState,
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

        // Connect to Maestro RFCOMM service
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

        // Initialize Maestro communication stack
        let codec = Codec::new();
        let stream = codec.wrap(stream);
        let mut client = Client::new(stream);
        let handle = client.handle();

        // Resolve Maestro channel
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

        // Update health
        {
            let mut lock = state.write().await;
            let health = lock.health.entry("bt.buds".to_string()).or_default();
            health.consecutive_failures = 0;
            health.backoff_until = None;
        }

        // Query initial ANC state
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

        // Subscribe to real-time status updates (battery, wear)
        let mut runtime_info_call = match service.subscribe_to_runtime_info() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to subscribe to runtime info for {}: {}", mac, e);
                continue;
            }
        };

        let mut runtime_info = runtime_info_call.stream();

        // Subscribe to settings changes (to catch physical toggles on the buds)
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

                        if let Some(RspOneof::Value(setting_val)) = change.value_oneof {
                            if let Some(ValOneof::CurrentAncrState(anc_state_raw)) = setting_val.value_oneof {
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

fn mode_to_anc_state(mode: &str) -> settings::AncState {
    match mode {
        "active" => settings::AncState::Active,
        "aware" => settings::AncState::Aware,
        "off" => settings::AncState::Off,
        _ => settings::AncState::Off,
    }
}

fn anc_state_to_string(state: &settings::AncState) -> String {
    match state {
        settings::AncState::Active => "active".to_string(),
        settings::AncState::Aware => "aware".to_string(),
        settings::AncState::Off => "off".to_string(),
        _ => "unknown".to_string(),
    }
}

static MAESTRO: OnceLock<MaestroManager> = OnceLock::new();

fn get_maestro(state: &SharedState) -> &MaestroManager {
    MAESTRO.get_or_init(|| MaestroManager::new(Arc::clone(state)))
}

pub struct BtDaemon {
    session: Option<bluer::Session>,
}

impl BtDaemon {
    pub fn new() -> Self {
        Self { session: None }
    }

    pub async fn poll(&mut self, state: SharedState, config: &Config) {
        if let Err(e) = self.poll_async(state, config).await {
            error!("BT daemon error: {}", e);
        }
    }

    async fn poll_async(&mut self, state: SharedState, config: &Config) -> Result<()> {
        if self.session.is_none() {
            self.session = Some(bluer::Session::new().await?);
        }
        let session = self.session.as_ref().unwrap();
        let adapter = session.default_adapter().await?;
        let adapter_powered = adapter.is_powered().await.unwrap_or(false);

        let mut bt_state = BtState {
            adapter_powered,
            ..Default::default()
        };

        if adapter_powered {
            let devices = adapter.device_addresses().await?;
            for addr in devices {
                let device = adapter.device(addr)?;
                if device.is_connected().await.unwrap_or(false) {
                    let uuids = device.uuids().await?.unwrap_or_default();
                    let audio_sink_uuid =
                        bluer::Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);
                    if uuids.contains(&audio_sink_uuid) {
                        bt_state.connected = true;
                        bt_state.device_address = addr.to_string();
                        bt_state.device_alias =
                            device.alias().await.unwrap_or_else(|_| addr.to_string());
                        bt_state.battery_percentage =
                            device.battery_percentage().await.unwrap_or(None);

                        for p in PLUGINS.iter() {
                            if p.can_handle(&bt_state.device_alias, &bt_state.device_address) {
                                match p.get_data(config, &state, &bt_state.device_address).await {
                                    Ok(data) => {
                                        bt_state.plugin_data = data
                                            .into_iter()
                                            .map(|(k, v)| {
                                                let val_str = match v {
                                                    TokenValue::String(s) => s,
                                                    TokenValue::Int(i) => i.to_string(),
                                                    TokenValue::Float(f) => format!("{:.1}", f),
                                                };
                                                (k, val_str)
                                            })
                                            .collect();
                                    }
                                    Err(e) => {
                                        warn!("Plugin {} failed for {}: {}", p.name(), addr, e);
                                        bt_state
                                            .plugin_data
                                            .push(("plugin_error".to_string(), e.to_string()));
                                    }
                                }
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }

        let mut lock = state.write().await;
        lock.bluetooth = bt_state;

        Ok(())
    }
}

pub trait BtPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn can_handle(&self, alias: &str, mac: &str) -> bool;
    fn get_data(
        &self,
        config: &Config,
        state: &SharedState,
        mac: &str,
    ) -> BoxFuture<'static, FluxoResult<Vec<(String, TokenValue)>>>;
    fn get_modes(
        &self,
        mac: &str,
        state: &SharedState,
    ) -> BoxFuture<'static, FluxoResult<Vec<String>>>;
    fn set_mode(
        &self,
        mode: &str,
        mac: &str,
        state: &SharedState,
    ) -> BoxFuture<'static, FluxoResult<()>>;
    fn cycle_mode(&self, mac: &str, state: &SharedState) -> BoxFuture<'static, FluxoResult<()>>;
}

pub struct PixelBudsPlugin;

impl BtPlugin for PixelBudsPlugin {
    fn name(&self) -> &str {
        "Pixel Buds Pro"
    }

    fn can_handle(&self, alias: &str, _mac: &str) -> bool {
        alias.contains("Pixel Buds Pro")
    }

    fn get_data(
        &self,
        _config: &Config,
        state: &SharedState,
        mac: &str,
    ) -> BoxFuture<'static, FluxoResult<Vec<(String, TokenValue)>>> {
        let mac = mac.to_string();
        let state = Arc::clone(state);
        Box::pin(async move {
            let maestro = get_maestro(&state);
            maestro.ensure_task(&mac);
            let status = maestro.get_status(&mac);

            if let Some(err) = status.error {
                return Err(FluxoError::Module {
                    module: "bt.buds",
                    message: err,
                });
            }

            let left_display = status
                .left_battery
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "---".to_string());
            let right_display = status
                .right_battery
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "---".to_string());

            let (anc_icon, class) = match status.anc_state.as_str() {
                "active" => ("ANC", "anc-active"),
                "aware" => ("Aware", "anc-aware"),
                "off" => ("Off", "anc-off"),
                _ => ("?", "anc-unknown"),
            };

            Ok(vec![
                ("left".to_string(), TokenValue::String(left_display)),
                ("right".to_string(), TokenValue::String(right_display)),
                ("anc".to_string(), TokenValue::String(anc_icon.to_string())),
                (
                    "plugin_class".to_string(),
                    TokenValue::String(class.to_string()),
                ),
            ])
        })
    }

    fn get_modes(
        &self,
        _mac: &str,
        _state: &SharedState,
    ) -> BoxFuture<'static, FluxoResult<Vec<String>>> {
        Box::pin(async move {
            Ok(vec![
                "active".to_string(),
                "aware".to_string(),
                "off".to_string(),
            ])
        })
    }

    fn set_mode(
        &self,
        mode: &str,
        mac: &str,
        state: &SharedState,
    ) -> BoxFuture<'static, FluxoResult<()>> {
        let mode = mode.to_string();
        let mac = mac.to_string();
        let state = Arc::clone(state);
        Box::pin(async move {
            get_maestro(&state)
                .send_command(&mac, BudsCommand::SetAnc(mode))
                .map_err(|e| FluxoError::Module {
                    module: "bt.buds",
                    message: e.to_string(),
                })
        })
    }

    fn cycle_mode(&self, mac: &str, state: &SharedState) -> BoxFuture<'static, FluxoResult<()>> {
        let mac = mac.to_string();
        let state = Arc::clone(state);
        Box::pin(async move {
            let status = get_maestro(&state).get_status(&mac);
            let next_mode = match status.anc_state.as_str() {
                "active" => "aware",
                "aware" => "off",
                _ => "active",
            };
            get_maestro(&state)
                .send_command(&mac, BudsCommand::SetAnc(next_mode.to_string()))
                .map_err(|e| FluxoError::Module {
                    module: "bt.buds",
                    message: e.to_string(),
                })
        })
    }
}

static PLUGINS: LazyLock<Vec<Box<dyn BtPlugin>>> =
    LazyLock::new(|| vec![Box::new(PixelBudsPlugin)]);

pub struct BtModule;

impl WaybarModule for BtModule {
    fn run(
        &self,
        config: &Config,
        state: &SharedState,
        args: &[&str],
    ) -> impl std::future::Future<Output = FluxoResult<WaybarOutput>> + Send {
        let action = args.first().cloned().unwrap_or("show").to_string();
        let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let state = Arc::clone(state);
        let config = config.clone();

        async move {
            let bt_state = {
                let lock = state.read().await;
                lock.bluetooth.clone()
            };

            match action.as_str() {
                "disconnect" if bt_state.connected => {
                    let _ = Command::new("bluetoothctl")
                        .args(["disconnect", &bt_state.device_address])
                        .output();
                    return Ok(WaybarOutput::default());
                }
                "cycle_mode" if bt_state.connected => {
                    let plugin = PLUGINS
                        .iter()
                        .find(|p| p.can_handle(&bt_state.device_alias, &bt_state.device_address));
                    if let Some(p) = plugin {
                        p.cycle_mode(&bt_state.device_address, &state).await?;
                    }
                    return Ok(WaybarOutput::default());
                }
                "get_modes" if bt_state.connected => {
                    let plugin = PLUGINS
                        .iter()
                        .find(|p| p.can_handle(&bt_state.device_alias, &bt_state.device_address));
                    let modes = if let Some(p) = plugin {
                        p.get_modes(&bt_state.device_address, &state).await?
                    } else {
                        vec![]
                    };
                    return Ok(WaybarOutput {
                        text: modes.join("\n"),
                        ..Default::default()
                    });
                }
                "set_mode" if bt_state.connected => {
                    if let Some(mode) = args.get(1) {
                        let plugin = PLUGINS.iter().find(|p| {
                            p.can_handle(&bt_state.device_alias, &bt_state.device_address)
                        });
                        if let Some(p) = plugin {
                            p.set_mode(mode, &bt_state.device_address, &state).await?;
                        }
                    }
                    return Ok(WaybarOutput::default());
                }
                "show" => {}
                _ => {}
            }

            if !bt_state.adapter_powered {
                return Ok(WaybarOutput {
                    text: config.bt.format_disabled.clone(),
                    tooltip: Some("Bluetooth Disabled".to_string()),
                    class: Some("disabled".to_string()),
                    percentage: None,
                });
            }

            if bt_state.connected {
                let mut tokens: Vec<(String, TokenValue)> = vec![
                    (
                        "alias".to_string(),
                        TokenValue::String(bt_state.device_alias.clone()),
                    ),
                    (
                        "mac".to_string(),
                        TokenValue::String(bt_state.device_address.clone()),
                    ),
                ];

                let mut class = vec!["connected".to_string()];
                let mut has_plugin = false;

                for (k, v) in &bt_state.plugin_data {
                    if k == "plugin_class" {
                        class.push(v.clone());
                        has_plugin = true;
                    } else if k == "plugin_error" {
                        class.push("plugin-error".to_string());
                    } else {
                        tokens.push((k.clone(), TokenValue::String(v.clone())));
                    }
                }

                let format = if has_plugin {
                    &config.bt.format_plugin
                } else {
                    &config.bt.format_connected
                };

                let text = format_template(format, &tokens);
                let tooltip = format!(
                    "{} | MAC: {}\nBattery: {}",
                    bt_state.device_alias,
                    bt_state.device_address,
                    bt_state
                        .battery_percentage
                        .map(|b| format!("{}%", b))
                        .unwrap_or_else(|| "N/A".to_string())
                );

                Ok(WaybarOutput {
                    text,
                    tooltip: Some(tooltip),
                    class: Some(class.join(" ")),
                    percentage: bt_state.battery_percentage,
                })
            } else {
                Ok(WaybarOutput {
                    text: config.bt.format_disconnected.clone(),
                    tooltip: Some("Bluetooth On (Disconnected)".to_string()),
                    class: Some("disconnected".to_string()),
                    percentage: None,
                })
            }
        }
    }
}
