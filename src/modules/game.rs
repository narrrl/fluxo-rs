use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;
use std::process::Command;

pub struct GameModule;

impl WaybarModule for GameModule {
    fn run(&self, _config: &Config, _state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let output = Command::new("hyprctl")
            .args(["getoption", "animations:enabled", "-j"])
            .output();

        let mut is_gamemode = false; // default to deactivated

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            
            // The JSON from hyprctl looks like {"int": 0, "float": 0.0, ...}
            // If int is 0, animations are disabled (Gamemode active)
            // If int is 1, animations are enabled (Gamemode deactivated)
            if stdout.contains("\"int\": 0") {
                is_gamemode = true;
            }
        }

        if is_gamemode {
            Ok(WaybarOutput {
                text: "<span size='large'>󰊖</span>".to_string(),
                tooltip: Some("Gamemode activated".to_string()),
                class: Some("active".to_string()),
                percentage: None,
            })
        } else {
            Ok(WaybarOutput {
                text: "<span size='large'></span>".to_string(),
                tooltip: Some("Gamemode deactivated".to_string()),
                class: None,
                percentage: None,
            })
        }
    }
}
