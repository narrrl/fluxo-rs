use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template, run_command};
use anyhow::Result;

pub struct BudsModule;

impl WaybarModule for BudsModule {
    fn run(&self, config: &Config, _state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let action = args.first().unwrap_or(&"show");
        let mac = &config.buds.mac;

        match *action {
            "cycle_anc" => {
                let current_mode = run_command("pbpctrl", &["get", "anc"])?;

                let next_mode = match current_mode.as_str() {
                    "active" => "aware",
                    "aware" => "off",
                    _ => "active",
                };

                let _ = run_command("pbpctrl", &["set", "anc", next_mode]);
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "connect" => {
                let _ = run_command("bluetoothctl", &["connect", mac]);
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "disconnect" => {
                let _ = run_command("bluetoothctl", &["disconnect", mac]);
                return Ok(WaybarOutput {
                    text: String::new(),
                    tooltip: None,
                    class: None,
                    percentage: None,
                });
            }
            "show" => {}
            other => {
                return Err(anyhow::anyhow!("Unknown buds action: '{}'", other));
            }
        }

        let bt_str = run_command("bluetoothctl", &["info", mac])?;

        if !bt_str.contains("Connected: yes") {
            return Ok(WaybarOutput {
                text: config.buds.format_disconnected.clone(),
                tooltip: Some("Pixel Buds Pro 2 not connected".to_string()),
                class: Some("disconnected".to_string()),
                percentage: None,
            });
        }

        let bat_output = match run_command("pbpctrl", &["show", "battery"]) {
            Ok(output) => output,
            Err(_) => {
                return Ok(WaybarOutput {
                    text: config.buds.format_disconnected.clone(),
                    tooltip: Some("Pixel Buds Pro 2 connected (No Data)".to_string()),
                    class: Some("disconnected".to_string()),
                    percentage: None,
                });
            }
        };

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

        let left_display = if left_bud == "unknown" {
            "---".to_string()
        } else {
            format!("{}%", left_bud)
        };
        let right_display = if right_bud == "unknown" {
            "---".to_string()
        } else {
            format!("{}%", right_bud)
        };

        let current_mode = run_command("pbpctrl", &["get", "anc"]).unwrap_or_default();

        let (anc_icon, class) = match current_mode.as_str() {
            "active" => ("ANC", "anc-active"),
            "aware" => ("Aware", "anc-aware"),
            "off" => ("Off", "anc-off"),
            _ => ("?", "anc-unknown"),
        };

        let text = format_template(
            &config.buds.format,
            &[
                ("left", TokenValue::String(&left_display)),
                ("right", TokenValue::String(&right_display)),
                ("anc", TokenValue::String(anc_icon)),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some("Pixel Buds Pro 2".to_string()),
            class: Some(class.to_string()),
            percentage: None,
        })
    }
}
