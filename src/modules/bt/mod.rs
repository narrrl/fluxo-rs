pub mod buds;
pub mod maestro;

use crate::config::Config;
use crate::error::Result as FluxoResult;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, BtDeviceInfo, BtState};
use crate::utils::{TokenValue, format_template};
use anyhow::Result;
use std::sync::LazyLock;
use tokio::sync::watch;
use tracing::{error, warn};

use self::buds::{BtPlugin, PixelBudsPlugin};

pub struct BtDaemon {
    session: Option<bluer::Session>,
}

impl BtDaemon {
    pub fn new() -> Self {
        Self { session: None }
    }

    pub async fn poll(
        &mut self,
        tx: &watch::Sender<BtState>,
        state: &AppReceivers,
        config: &Config,
    ) {
        if let Err(e) = self.poll_async(tx, state, config).await {
            error!("BT daemon error: {}", e);
        }
    }

    async fn poll_async(
        &mut self,
        tx: &watch::Sender<BtState>,
        state: &AppReceivers,
        config: &Config,
    ) -> Result<()> {
        if self.session.is_none() {
            self.session = Some(bluer::Session::new().await?);
        }
        let session = self.session.as_ref().unwrap();
        let adapter = session.default_adapter().await?;
        let adapter_powered = adapter.is_powered().await.unwrap_or(false);

        let mut connected_devices = Vec::new();

        if adapter_powered {
            let mut addresses = adapter.device_addresses().await?;
            addresses.sort();
            let audio_sink_uuid = bluer::Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);

            for addr in addresses {
                let device = adapter.device(addr)?;
                if !device.is_connected().await.unwrap_or(false) {
                    continue;
                }
                let uuids = device.uuids().await?.unwrap_or_default();
                if !uuids.contains(&audio_sink_uuid) {
                    continue;
                }

                let mut dev_info = BtDeviceInfo {
                    device_address: addr.to_string(),
                    device_alias: device.alias().await.unwrap_or_else(|_| addr.to_string()),
                    battery_percentage: device.battery_percentage().await.unwrap_or(None),
                    plugin_data: vec![],
                };

                for p in PLUGINS.iter() {
                    if p.can_handle(&dev_info.device_alias, &dev_info.device_address) {
                        match p.get_data(config, state, &dev_info.device_address).await {
                            Ok(data) => {
                                dev_info.plugin_data = data
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
                                dev_info
                                    .plugin_data
                                    .push(("plugin_error".to_string(), e.to_string()));
                            }
                        }
                        break;
                    }
                }
                connected_devices.push(dev_info);
            }
        }

        let _ = tx.send(BtState {
            adapter_powered,
            devices: connected_devices,
        });

        Ok(())
    }
}

static PLUGINS: LazyLock<Vec<Box<dyn BtPlugin>>> =
    LazyLock::new(|| vec![Box::new(PixelBudsPlugin)]);

fn trigger_robust_poll(state: AppReceivers) {
    tokio::spawn(async move {
        for delay in [200, 500, 1000, 2000, 3000] {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            let _ = state.bt_force_poll.try_send(());
        }
    });
}

/// Resolve a target MAC: use explicit arg if given, otherwise fall back to the active device.
async fn resolve_target_mac(
    bt_state: &BtState,
    state: &AppReceivers,
    explicit_mac: Option<&str>,
) -> Option<String> {
    if let Some(mac) = explicit_mac {
        return Some(mac.to_string());
    }
    let idx = *state.bt_cycle.read().await;
    bt_state
        .active_device(idx)
        .map(|d| d.device_address.clone())
}

/// Find a device in the current state by MAC.
fn find_device<'a>(bt_state: &'a BtState, mac: &str) -> Option<&'a BtDeviceInfo> {
    bt_state.devices.iter().find(|d| d.device_address == mac)
}

pub struct BtModule;

impl WaybarModule for BtModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> FluxoResult<WaybarOutput> {
        let action = args.first().cloned().unwrap_or("show");
        let bt_state = state.bluetooth.borrow().clone();

        match action {
            "connect" => {
                if let Some(mac) = args.get(1) {
                    if let Ok(session) = bluer::Session::new().await
                        && let Ok(adapter) = session.default_adapter().await
                        && let Ok(addr) = mac.parse::<bluer::Address>()
                        && let Ok(device) = adapter.device(addr)
                    {
                        let _ = device.connect().await;
                    }
                    trigger_robust_poll(state.clone());
                }
                return Ok(WaybarOutput::default());
            }
            "disconnect" => {
                let target_mac = resolve_target_mac(&bt_state, state, args.get(1).copied()).await;
                if let Some(mac) = target_mac {
                    if let Ok(session) = bluer::Session::new().await
                        && let Ok(adapter) = session.default_adapter().await
                        && let Ok(addr) = mac.parse::<bluer::Address>()
                        && let Ok(device) = adapter.device(addr)
                    {
                        let _ = device.disconnect().await;
                    }
                    trigger_robust_poll(state.clone());
                }
                return Ok(WaybarOutput::default());
            }
            "cycle" => {
                let count = bt_state.devices.len();
                if count > 1 {
                    let mut idx = state.bt_cycle.write().await;
                    *idx = (*idx + 1) % count;
                }
                let _ = state.bt_force_poll.try_send(());
                return Ok(WaybarOutput::default());
            }
            "menu_data" => {
                let mut lines = Vec::new();

                // Connected devices
                for dev in &bt_state.devices {
                    lines.push(format!(
                        "CONNECTED:{}|{}",
                        dev.device_alias, dev.device_address
                    ));
                }

                // Paired-but-not-connected devices
                if let Ok(session) = bluer::Session::new().await
                    && let Ok(adapter) = session.default_adapter().await
                    && let Ok(addresses) = adapter.device_addresses().await
                {
                    let connected_macs: std::collections::HashSet<&str> = bt_state
                        .devices
                        .iter()
                        .map(|d| d.device_address.as_str())
                        .collect();

                    for addr in addresses {
                        let addr_str = addr.to_string();
                        if connected_macs.contains(addr_str.as_str()) {
                            continue;
                        }
                        if let Ok(device) = adapter.device(addr)
                            && device.is_paired().await.unwrap_or(false)
                        {
                            let alias = device.alias().await.unwrap_or_else(|_| addr.to_string());
                            lines.push(format!("PAIRED:{}|{}", alias, addr_str));
                        }
                    }
                }

                return Ok(WaybarOutput {
                    text: lines.join("\n"),
                    ..Default::default()
                });
            }
            "get_modes" => {
                let target_mac = resolve_target_mac(&bt_state, state, args.get(1).copied()).await;
                if let Some(mac) = target_mac
                    && let Some(dev) = find_device(&bt_state, &mac)
                {
                    let plugin = PLUGINS
                        .iter()
                        .find(|p| p.can_handle(&dev.device_alias, &dev.device_address));
                    if let Some(p) = plugin {
                        let modes = p.get_modes(&mac, state).await?;
                        return Ok(WaybarOutput {
                            text: modes.join("\n"),
                            ..Default::default()
                        });
                    }
                }
                return Ok(WaybarOutput::default());
            }
            "set_mode" => {
                if let Some(mode) = args.get(1) {
                    let target_mac =
                        resolve_target_mac(&bt_state, state, args.get(2).copied()).await;
                    if let Some(mac) = target_mac
                        && let Some(dev) = find_device(&bt_state, &mac)
                    {
                        let plugin = PLUGINS
                            .iter()
                            .find(|p| p.can_handle(&dev.device_alias, &dev.device_address));
                        if let Some(p) = plugin {
                            p.set_mode(mode, &mac, state).await?;
                            trigger_robust_poll(state.clone());
                        }
                    }
                }
                return Ok(WaybarOutput::default());
            }
            "cycle_mode" => {
                let target_mac = resolve_target_mac(&bt_state, state, args.get(1).copied()).await;
                if let Some(mac) = target_mac
                    && let Some(dev) = find_device(&bt_state, &mac)
                {
                    let plugin = PLUGINS
                        .iter()
                        .find(|p| p.can_handle(&dev.device_alias, &dev.device_address));
                    if let Some(p) = plugin {
                        p.cycle_mode(&mac, state).await?;
                        trigger_robust_poll(state.clone());
                    }
                }
                return Ok(WaybarOutput::default());
            }
            _ => {}
        }

        // "show" and fallthrough
        if !bt_state.adapter_powered {
            return Ok(WaybarOutput {
                text: config.bt.format_disabled.clone(),
                tooltip: Some("Bluetooth Disabled".to_string()),
                class: Some("disabled".to_string()),
                percentage: None,
            });
        }

        let cycle_idx = *state.bt_cycle.read().await;
        if let Some(dev) = bt_state.active_device(cycle_idx) {
            let mut tokens: Vec<(String, TokenValue)> = vec![
                (
                    "alias".to_string(),
                    TokenValue::String(dev.device_alias.clone()),
                ),
                (
                    "mac".to_string(),
                    TokenValue::String(dev.device_address.clone()),
                ),
            ];

            let mut class = vec!["connected".to_string()];
            let mut has_plugin = false;

            for (k, v) in &dev.plugin_data {
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
                dev.device_alias,
                dev.device_address,
                dev.battery_percentage
                    .map(|b| format!("{}%", b))
                    .unwrap_or_else(|| "N/A".to_string())
            );

            Ok(WaybarOutput {
                text,
                tooltip: Some(tooltip),
                class: Some(class.join(" ")),
                percentage: dev.battery_percentage,
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
