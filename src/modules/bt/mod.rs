pub mod buds;
pub mod maestro;

use crate::config::Config;
use crate::error::Result as FluxoResult;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, BtState};
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
                                match p.get_data(config, state, &bt_state.device_address).await {
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

        let _ = tx.send(bt_state);

        Ok(())
    }
}

static PLUGINS: LazyLock<Vec<Box<dyn BtPlugin>>> =
    LazyLock::new(|| vec![Box::new(PixelBudsPlugin)]);

fn trigger_robust_poll(state: AppReceivers) {
    tokio::spawn(async move {
        // Poll immediately and then a few times over the next few seconds
        // to catch slow state changes in bluez or plugins.
        for delay in [200, 500, 1000, 2000, 3000] {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            let _ = state.bt_force_poll.try_send(());
        }
    });
}

pub struct BtModule;

impl WaybarModule for BtModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> FluxoResult<WaybarOutput> {
        let action = args.first().cloned().unwrap_or("show").to_string();
        let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let bt_state = state.bluetooth.borrow().clone();

        match action.as_str() {
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
            "disconnect" if bt_state.connected => {
                if let Ok(session) = bluer::Session::new().await
                    && let Ok(adapter) = session.default_adapter().await
                    && let Ok(addr) = bt_state.device_address.parse::<bluer::Address>()
                    && let Ok(device) = adapter.device(addr)
                {
                    let _ = device.disconnect().await;
                }
                trigger_robust_poll(state.clone());
                return Ok(WaybarOutput::default());
            }
            "menu_data" => {
                let mut devs = Vec::new();
                if let Ok(session) = bluer::Session::new().await
                    && let Ok(adapter) = session.default_adapter().await
                    && let Ok(addresses) = adapter.device_addresses().await
                {
                    for addr in addresses {
                        if let Ok(device) = adapter.device(addr)
                            && device.is_paired().await.unwrap_or(false)
                        {
                            let alias = device.alias().await.unwrap_or_else(|_| addr.to_string());
                            devs.push(format!("{} ({})", alias, addr));
                        }
                    }
                }
                return Ok(WaybarOutput {
                    text: devs.join("\n"),
                    ..Default::default()
                });
            }
            "cycle_mode" if bt_state.connected => {
                let plugin = PLUGINS
                    .iter()
                    .find(|p| p.can_handle(&bt_state.device_alias, &bt_state.device_address));
                if let Some(p) = plugin {
                    p.cycle_mode(&bt_state.device_address, state).await?;
                    trigger_robust_poll(state.clone());
                }
                return Ok(WaybarOutput::default());
            }
            "get_modes" if bt_state.connected => {
                let plugin = PLUGINS
                    .iter()
                    .find(|p| p.can_handle(&bt_state.device_alias, &bt_state.device_address));
                let modes = if let Some(p) = plugin {
                    p.get_modes(&bt_state.device_address, state).await?
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
                    let plugin = PLUGINS
                        .iter()
                        .find(|p| p.can_handle(&bt_state.device_alias, &bt_state.device_address));
                    if let Some(p) = plugin {
                        p.set_mode(mode, &bt_state.device_address, state).await?;
                        trigger_robust_poll(state.clone());
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
