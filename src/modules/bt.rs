use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;
use std::process::Command;

pub struct BtModule;

impl WaybarModule for BtModule {
    fn run(&self, _config: &Config, _state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let action = args.first().unwrap_or(&"show");

        if *action == "disconnect" {
            if let Some(mac) = find_audio_device() {
                let _ = Command::new("bluetoothctl").args(["disconnect", &mac]).output();
            }
            return Ok(WaybarOutput {
                text: String::new(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        }

        // Check if bluetooth is powered on
        if let Ok(output) = Command::new("bluetoothctl").arg("show").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("Powered: no") {
                return Ok(WaybarOutput {
                    text: "󰂲 Off".to_string(),
                    tooltip: Some("Bluetooth Disabled".to_string()),
                    class: Some("disabled".to_string()),
                    percentage: None,
                });
            }
        }

        if let Some(mac) = find_audio_device() {
            let info_output = Command::new("bluetoothctl").args(["info", &mac]).output()?;
            let info = String::from_utf8_lossy(&info_output.stdout);

            let mut alias = mac.clone();
            let mut battery = None;
            let mut trusted = "no";

            for line in info.lines() {
                if line.contains("Alias:") {
                    alias = line.split("Alias:").nth(1).unwrap_or("").trim().to_string();
                } else if line.contains("Battery Percentage:") {
                    if let Some(bat_str) = line.split('(').nth(1).and_then(|s| s.split(')').next()) {
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
                battery.map(|b| format!("{}%", b)).unwrap_or_else(|| "N/A".to_string())
            );

            Ok(WaybarOutput {
                text: format!("{} 󰂰", alias),
                tooltip: Some(tooltip),
                class: Some("connected".to_string()),
                percentage: battery,
            })
        } else {
            // No device connected but Bluetooth is on
            Ok(WaybarOutput {
                text: "󰂯".to_string(),
                tooltip: Some("Bluetooth On (Disconnected)".to_string()),
                class: Some("disconnected".to_string()),
                percentage: None,
            })
        }
    }
}

fn find_audio_device() -> Option<String> {
    // 1. Try to check if current default sink is a bluetooth device
    if let Ok(output) = Command::new("pactl").arg("get-default-sink").output() {
        let sink = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sink.starts_with("bluez_output.") {
            let parts: Vec<&str> = sink.split('.').collect();
            if parts.len() >= 2 {
                return Some(parts[1].replace('_', ":"));
            }
        }
    }

    // 2. Fallback: Search bluetoothctl for connected devices with Audio Sink UUID
    if let Ok(output) = Command::new("bluetoothctl").args(["devices", "Connected"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("Device ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let mac = parts[1];
                    if let Ok(info) = Command::new("bluetoothctl").args(["info", mac]).output() {
                        let info_str = String::from_utf8_lossy(&info.stdout);
                        if info_str.contains("0000110b-0000-1000-8000-00805f9b34fb") { // Audio Sink UUID
                            return Some(mac.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}
