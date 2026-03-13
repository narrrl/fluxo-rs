use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::{Result, anyhow};
use std::process::Command;

pub struct AudioModule;

impl WaybarModule for AudioModule {
    fn run(&self, _config: &Config, _state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let target_type = args.first().unwrap_or(&"sink");
        let action = args.get(1).unwrap_or(&"show");

        match *action {
            "cycle" => {
                self.cycle_device(target_type)?;
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "show" | _ => {
                self.get_status(target_type)
            }
        }
    }
}

impl AudioModule {
    fn get_status(&self, target_type: &str) -> Result<WaybarOutput> {
        let target = if target_type == "sink" { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" };

        // Get volume and mute status via wpctl (faster than pactl for this)
        let output = Command::new("wpctl")
            .args(["get-volume", target])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Output format: "Volume: 0.50" or "Volume: 0.50 [MUTED]"
        let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
        if parts.len() < 2 {
            return Err(anyhow!("Could not parse wpctl output: {}", stdout));
        }

        let vol_val: f64 = parts[1].parse().unwrap_or(0.0);
        let vol = (vol_val * 100.0).round() as u8;
        let display_vol = std::cmp::min(vol, 100);
        let muted = stdout.contains("[MUTED]");

        let description = self.get_description(target_type)?;
        let name = if description.len() > 20 {
            format!("{}...", &description[..17])
        } else {
            description.clone()
        };

        let (text, class) = if muted {
            let icon = if target_type == "sink" { "" } else { "" };
            (format!("{} {}", name, icon), "muted")
        } else {
            let icon = if target_type == "sink" {
                if display_vol <= 30 { "" }
                else if display_vol <= 60 { "" }
                else { "" }
            } else {
                ""
            };
            (format!("{} {}% {}", name, display_vol, icon), "unmuted")
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(description),
            class: Some(class.to_string()),
            percentage: Some(display_vol),
        })
    }

    fn get_description(&self, target_type: &str) -> Result<String> {
        // Get the default device name
        let info_output = Command::new("pactl").arg("info").output()?;
        let info_stdout = String::from_utf8_lossy(&info_output.stdout);
        let search_key = if target_type == "sink" { "Default Sink:" } else { "Default Source:" };
        
        let default_dev = info_stdout.lines()
            .find(|l| l.contains(search_key))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .ok_or_else(|| anyhow!("Default {} not found", target_type))?;

        // Get the description of that device
        let list_cmd = if target_type == "sink" { "sinks" } else { "sources" };
        let list_output = Command::new("pactl").args(["list", list_cmd]).output()?;
        let list_stdout = String::from_utf8_lossy(&list_output.stdout);

        let mut current_name = String::new();
        for line in list_stdout.lines() {
            if line.trim().starts_with("Name: ") {
                current_name = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
            if current_name == default_dev && line.trim().starts_with("Description: ") {
                return Ok(line.split(':').nth(1).unwrap_or("").trim().to_string());
            }
        }

        Ok(default_dev.to_string())
    }

    fn cycle_device(&self, target_type: &str) -> Result<()> {
        let list_cmd = if target_type == "sink" { "sinks" } else { "sources" };
        let output = Command::new("pactl").args(["list", "short", list_cmd]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut devices: Vec<String> = stdout.lines()
            .filter_map(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[1].to_string();
                    if target_type == "source" && name.contains(".monitor") {
                        None
                    } else {
                        Some(name)
                    }
                } else {
                    None
                }
            })
            .collect();

        if devices.is_empty() { return Ok(()); }

        let info_output = Command::new("pactl").arg("info").output()?;
        let info_stdout = String::from_utf8_lossy(&info_output.stdout);
        let search_key = if target_type == "sink" { "Default Sink:" } else { "Default Source:" };
        
        let current_dev = info_stdout.lines()
            .find(|l| l.contains(search_key))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .unwrap_or("");

        let current_index = devices.iter().position(|d| d == current_dev).unwrap_or(0);
        let next_index = (current_index + 1) % devices.len();
        let next_dev = &devices[next_index];

        let set_cmd = if target_type == "sink" { "set-default-sink" } else { "set-default-source" };
        Command::new("pactl").args([set_cmd, next_dev]).status()?;

        Ok(())
    }
}
