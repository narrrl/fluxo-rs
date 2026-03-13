use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;
use std::process::Command;

pub struct BudsModule;

const MAC_ADDRESS: &str = "B4:23:A2:09:D3:53";

impl WaybarModule for BudsModule {
    fn run(&self, _config: &Config, _state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let action = args.first().unwrap_or(&"show");

        match *action {
            "cycle_anc" => {
                let output = Command::new("pbpctrl").args(["get", "anc"]).output()?;
                let current_mode = String::from_utf8_lossy(&output.stdout).trim().to_string();
                
                let next_mode = match current_mode.as_str() {
                    "active" => "aware",
                    "aware" => "off",
                    _ => "active", // default or off goes to active
                };
                
                Command::new("pbpctrl").args(["set", "anc", next_mode]).status()?;
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "connect" => {
                Command::new("bluetoothctl").args(["connect", MAC_ADDRESS]).status()?;
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "disconnect" => {
                Command::new("bluetoothctl").args(["disconnect", MAC_ADDRESS]).status()?;
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "show" | _ => {}
        }

        // Check if connected
        let bt_info = Command::new("bluetoothctl").args(["info", MAC_ADDRESS]).output()?;
        let bt_str = String::from_utf8_lossy(&bt_info.stdout);
        
        if !bt_str.contains("Connected: yes") {
            return Ok(WaybarOutput {
                text: "<span size='large'></span>".to_string(),
                tooltip: Some("Pixel Buds Pro 2 not connected".to_string()),
                class: Some("disconnected".to_string()),
                percentage: None,
            });
        }

        // Get battery output
        let bat_cmd = Command::new("pbpctrl").args(["show", "battery"]).output();
        if bat_cmd.is_err() || !bat_cmd.as_ref().unwrap().status.success() {
            return Ok(WaybarOutput {
                text: "<span size='large'></span>".to_string(),
                tooltip: Some("Pixel Buds Pro 2 connected (No Data)".to_string()),
                class: Some("disconnected".to_string()),
                percentage: None,
            });
        }
        
        let bat_result = bat_cmd.unwrap();
        let bat_output = String::from_utf8_lossy(&bat_result.stdout);
        let mut left_bud = "unknown";
        let mut right_bud = "unknown";

        for line in bat_output.lines() {
            if line.contains("left bud:") {
                left_bud = line.split_whitespace().nth(2).unwrap_or("unknown");
            } else if line.contains("right bud:") {
                right_bud = line.split_whitespace().nth(2).unwrap_or("unknown");
            }
        }

        if left_bud == "unknown" && right_bud == "unknown" {
            return Ok(WaybarOutput {
                text: "{}".to_string(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        }

        let left_display = if left_bud == "unknown" { "L: ---".to_string() } else { format!("L: {}", left_bud) };
        let right_display = if right_bud == "unknown" { "R: ---".to_string() } else { format!("R: {}", right_bud) };

        // Get ANC info
        let anc_cmd = Command::new("pbpctrl").args(["get", "anc"]).output()?;
        let current_mode = String::from_utf8_lossy(&anc_cmd.stdout).trim().to_string();

        let (anc_icon, class) = match current_mode.as_str() {
            "active" => ("ANC", "anc-active"),
            "aware" => ("Aware", "anc-aware"),
            "off" => ("Off", "anc-off"),
            _ => ("?", "anc-unknown"),
        };

        Ok(WaybarOutput {
            text: format!("{} | {} | {}", left_display, right_display, anc_icon),
            tooltip: Some("Pixel Buds Pro 2".to_string()),
            class: Some(class.to_string()),
            percentage: None,
        })
    }
}
