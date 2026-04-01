use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};
use anyhow::Result;
use std::fs;

pub struct PowerModule;

impl WaybarModule for PowerModule {
    fn run(&self, config: &Config, _state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let critical_threshold = 15;
        let warning_threshold = 50;

        // Find the first battery
        let mut battery_path = None;
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("BAT") {
                    battery_path = Some(entry.path());
                    break;
                }
            }
        }

        // Check AC status
        let mut ac_online = false;
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("AC") || name.starts_with("ADP") {
                    let online_path = entry.path().join("online");
                    if let Ok(online_str) = fs::read_to_string(online_path)
                        && online_str.trim() == "1"
                    {
                        ac_online = true;
                        break;
                    }
                }
            }
        }

        let Some(bat_path) = battery_path else {
            if ac_online {
                return Ok(WaybarOutput {
                    text: "".to_string(),
                    tooltip: Some("AC Power (No Battery)".to_string()),
                    class: Some("ac".to_string()),
                    percentage: None,
                });
            } else {
                return Ok(WaybarOutput {
                    text: "".to_string(),
                    tooltip: Some("Error: Battery not found".to_string()),
                    class: Some("unknown".to_string()),
                    percentage: None,
                });
            }
        };

        let capacity_str =
            fs::read_to_string(bat_path.join("capacity")).unwrap_or_else(|_| "0".to_string());
        let percentage: u8 = capacity_str.trim().parse().unwrap_or(0);
        let status_str =
            fs::read_to_string(bat_path.join("status")).unwrap_or_else(|_| "Unknown".to_string());
        let state = status_str.trim().to_lowercase();

        let (icon, class, tooltip) = if state == "charging" || ac_online {
            (
                "",
                "charging",
                format!("TLP: AC | Charging at {}%", percentage),
            )
        } else if state == "discharging" {
            let t = format!("TLP: Battery | Discharging at {}%", percentage);
            if percentage <= critical_threshold {
                ("", "critical", t)
            } else if percentage <= warning_threshold {
                ("", "warning", t)
            } else if percentage <= 85 {
                ("", "bat", t)
            } else {
                ("", "bat", t)
            }
        } else {
            (
                "",
                "charging",
                format!("TLP: AC | Fully Charged at {}%", percentage),
            )
        };

        let text = format_template(
            &config.power.format,
            &[
                ("percentage", TokenValue::Int(percentage as i64)),
                ("icon", TokenValue::String(icon.to_string())),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(tooltip),
            class: Some(class.to_string()),
            percentage: Some(percentage),
        })
    }
}
