//! Gamemode indicator. Queries Hyprland's animation setting over its IPC
//! socket; animations disabled => gamemode active. Dispatch-only.

use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::AppReceivers;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Renders a glyph depending on whether Hyprland animations are disabled.
pub struct GameModule;

impl WaybarModule for GameModule {
    async fn run(
        &self,
        config: &Config,
        _state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let is_gamemode = hyprland_ipc("j/getoption animations:enabled")
            .await
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

/// Send `cmd` to Hyprland's `.socket.sock` and return the response body.
async fn hyprland_ipc(cmd: &str) -> Result<String> {
    let path = crate::utils::get_hyprland_socket(".socket.sock")?;

    let mut stream = UnixStream::connect(path).await?;
    stream.write_all(cmd.as_bytes()).await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    Ok(response)
}
