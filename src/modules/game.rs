use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::anyhow;
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

pub struct GameModule;

impl WaybarModule for GameModule {
    async fn run(
        &self,
        config: &Config,
        _state: &SharedState,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let is_gamemode = hyprland_ipc("j/getoption animations:enabled")
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

fn hyprland_ipc(cmd: &str) -> Result<String> {
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| anyhow!("HYPRLAND_INSTANCE_SIGNATURE not set"))?;
    let path = format!("/tmp/hypr/{}/.socket.sock", signature);

    let mut stream = UnixStream::connect(path)?;
    stream.write_all(cmd.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    Ok(response)
}
