use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::run_command;
use anyhow::Result;

pub struct GameModule;

impl WaybarModule for GameModule {
    fn run(&self, config: &Config, _state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let is_gamemode = run_command("hyprctl", &["getoption", "animations:enabled", "-j"])
            .map(|stdout| stdout.contains("\"int\": 0"))
            .unwrap_or(false);

        if is_gamemode {
            Ok(WaybarOutput {
                text: config.game.format_active.clone(),
                tooltip: Some("Gamemode activated".to_string()),
                class: Some("active".to_string()),
                percentage: None,
            })
        } else {
            Ok(WaybarOutput {
                text: config.game.format_inactive.clone(),
                tooltip: Some("Gamemode deactivated".to_string()),
                class: None,
                percentage: None,
            })
        }
    }
}
