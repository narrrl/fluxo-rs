use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, KeyboardState};
use crate::utils::{TokenValue, format_template};
use anyhow::anyhow;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::watch;
use tracing::{error, info};

pub struct KeyboardModule;

impl WaybarModule for KeyboardModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let layout = state.keyboard.borrow().layout.clone();

        if layout.is_empty() {
            return Ok(WaybarOutput {
                text: "Layout Loading...".to_string(),
                tooltip: None,
                class: Some("loading".to_string()),
                percentage: None,
            });
        }

        let text = format_template(
            &config.keyboard.format,
            &[("layout", TokenValue::String(layout.clone()))],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("Keyboard Layout: {}", layout)),
            class: Some("normal".to_string()),
            percentage: None,
        })
    }
}

pub struct KeyboardDaemon;

impl KeyboardDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, tx: watch::Sender<KeyboardState>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::listen_loop(&tx).await {
                    error!("Keyboard layout listener error: {}", e);
                    // Fallback to waiting before reconnecting to prevent tight loop
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        });
    }

    async fn listen_loop(tx: &watch::Sender<KeyboardState>) -> anyhow::Result<()> {
        let path = crate::utils::get_hyprland_socket(".socket2.sock")?;

        info!("Connecting to Hyprland event socket: {:?}", path);
        let stream = UnixStream::connect(path).await?;
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();

        // Fetch initial layout natively via hyprctl
        if let Ok(output) = tokio::process::Command::new("hyprctl")
            .args(["devices", "-j"])
            .output()
            .await
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            && let Some(keyboards) = json.get("keyboards").and_then(|v| v.as_array())
            && let Some(main_kb) = keyboards.last()
        {
            // The last active one is usually the main one
            if let Some(layout) = main_kb.get("active_keymap").and_then(|v| v.as_str()) {
                let _ = tx.send(KeyboardState {
                    layout: layout.to_string(),
                });
            }
        }

        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(payload) = line.strip_prefix("activelayout>>") {
                // payload format: keyboard_name,layout_name
                let parts: Vec<&str> = payload.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let layout = parts[1].to_string();
                    let _ = tx.send(KeyboardState { layout });
                }
            }
        }

        Err(anyhow!("Hyprland socket closed or read error"))
    }
}
