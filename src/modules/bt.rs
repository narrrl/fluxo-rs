use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template, run_command};
use anyhow::Result;
use std::process::Command;

pub struct BtModule;

impl WaybarModule for BtModule {
    fn run(&self, config: &Config, _state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let action = args.first().unwrap_or(&"show");

        if *action == "disconnect" {
            if let Some(mac) = find_audio_device() {
                let _ = Command::new("bluetoothctl")
                    .args(["disconnect", &mac])
                    .output();
            }
            return Ok(WaybarOutput {
                text: String::new(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        }

        if let Ok(stdout) = run_command("bluetoothctl", &["show"])
            && stdout.contains("Powered: no")
        {
            return Ok(WaybarOutput {
                text: config.bt.format_disabled.clone(),
                tooltip: Some("Bluetooth Disabled".to_string()),
                class: Some("disabled".to_string()),
                percentage: None,
            });
        }

        if let Some(mac) = find_audio_device() {
            let info = run_command("bluetoothctl", &["info", &mac])?;

            let mut alias = mac.clone();
            let mut battery = None;
            let mut trusted = "no";

            for line in info.lines() {
                if line.contains("Alias:") {
                    alias = line.split("Alias:").nth(1).unwrap_or("").trim().to_string();
                } else if line.contains("Battery Percentage:") {
                    if let Some(bat_str) = line.split('(').nth(1).and_then(|s| s.split(')').next())
                    {
                        battery = bat_str.parse::<u8>().ok();
                    }
                } else if line.contains("Trusted: yes") {
                    trusted = "yes";
                }
            }

            let tooltip = format!(
                "{} | MAC: {}\nTrusted: {} | Bat: {}",
                alias,
                mac,
                trusted,
                battery
                    .map(|b| format!("{}%", b))
                    .unwrap_or_else(|| "N/A".to_string())
            );

            let text = format_template(
                &config.bt.format_connected,
                &[("alias", TokenValue::String(&alias))],
            );

            Ok(WaybarOutput {
                text,
                tooltip: Some(tooltip),
                class: Some("connected".to_string()),
                percentage: battery,
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

fn find_audio_device() -> Option<String> {
    if let Ok(sink) = run_command("pactl", &["get-default-sink"])
        && sink.starts_with("bluez_output.")
    {
        let parts: Vec<&str> = sink.split('.').collect();
        if parts.len() >= 2 {
            return Some(parts[1].replace('_', ":"));
        }
    }

    if let Ok(stdout) = run_command("bluetoothctl", &["devices", "Connected"]) {
        for line in stdout.lines() {
            if line.starts_with("Device ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let mac = parts[1];
                    if let Ok(info_str) = run_command("bluetoothctl", &["info", mac])
                        && info_str.contains("0000110b-0000-1000-8000-00805f9b34fb")
                    {
                        return Some(mac.to_string());
                    }
                }
            }
        }
    }

    None
}
