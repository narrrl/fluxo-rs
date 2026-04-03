use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, MprisState};
use crate::utils::{TokenValue, format_template};
use tokio::sync::watch;
use tracing::{debug, info};
use zbus::{Connection, proxy};

pub struct MprisModule;

impl WaybarModule for MprisModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let mpris = state.mpris.borrow().clone();

        if mpris.is_stopped && mpris.title.is_empty() {
            return Ok(WaybarOutput {
                text: String::new(),
                tooltip: None,
                class: Some("stopped".to_string()),
                percentage: None,
            });
        }

        let status_icon = if mpris.is_playing {
            "󰏤"
        } else if mpris.is_paused {
            "󰐊"
        } else {
            "󰓛"
        };

        let class = if mpris.is_playing {
            "playing"
        } else if mpris.is_paused {
            "paused"
        } else {
            "stopped"
        };

        let text = format_template(
            &config.mpris.format,
            &[
                ("artist", TokenValue::String(mpris.artist.clone())),
                ("title", TokenValue::String(mpris.title.clone())),
                ("album", TokenValue::String(mpris.album.clone())),
                ("status_icon", TokenValue::String(status_icon.to_string())),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("{} - {}", mpris.artist, mpris.title)),
            class: Some(class.to_string()),
            percentage: None,
        })
    }
}

pub struct MprisDaemon;

#[proxy(
    interface = "org.freedesktop.DBus",
    default_service = "org.freedesktop.DBus",
    default_path = "/org/freedesktop/DBus"
)]
trait DBus {
    fn list_names(&self) -> zbus::Result<Vec<String>>;
}

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MprisPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::Value<'_>>>;
}

impl MprisDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, tx: watch::Sender<MprisState>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::listen_loop(&tx).await {
                    debug!("MPRIS listener ended or error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        });
    }

    async fn listen_loop(tx: &watch::Sender<MprisState>) -> anyhow::Result<()> {
        let connection = Connection::session().await?;

        info!("Connected to D-Bus for MPRIS monitoring");

        // Periodically poll for the active player and update the MPRIS state.
        // This avoids complex dynamic signal tracking across ephemeral player instances.

        let dbus_proxy = DBusProxy::new(&connection).await?;

        loop {
            let names = dbus_proxy.list_names().await?;
            let mut active_player = None;

            for name in names {
                if name.starts_with("org.mpris.MediaPlayer2.") {
                    active_player = Some(name);
                    break; // Just grab the first active player for now
                }
            }

            if let Some(player_name) = active_player {
                if let Ok(player_proxy) = MprisPlayerProxy::builder(&connection)
                    .destination(player_name.clone())?
                    .build()
                    .await
                {
                    let status = player_proxy.playback_status().await.unwrap_or_default();
                    let metadata = player_proxy.metadata().await.unwrap_or_default();

                    let is_playing = status == "Playing";
                    let is_paused = status == "Paused";
                    let is_stopped = status == "Stopped";

                    let mut artist = String::new();
                    let mut title = String::new();
                    let mut album = String::new();

                    if let Some(v) = metadata.get("xesam:artist") {
                        if let Ok(arr) = zbus::zvariant::Array::try_from(v) {
                            let mut artists = Vec::new();
                            for i in 0..arr.len() {
                                if let Ok(Some(s)) = arr.get::<&str>(i) {
                                    artists.push(s.to_string());
                                }
                            }
                            artist = artists.join(", ");
                        } else if let Ok(a) = <&str>::try_from(v) {
                            artist = a.to_string();
                        }
                    }
                    if let Some(v) = metadata.get("xesam:title")
                        && let Ok(t) = <&str>::try_from(v)
                    {
                        title = t.to_string();
                    }
                    if let Some(v) = metadata.get("xesam:album")
                        && let Ok(a) = <&str>::try_from(v)
                    {
                        album = a.to_string();
                    }

                    // Only send if changed
                    let current = tx.borrow();
                    if current.is_playing != is_playing
                        || current.is_paused != is_paused
                        || current.is_stopped != is_stopped
                        || current.title != title
                        || current.artist != artist
                        || current.album != album
                    {
                        drop(current); // Drop borrow before send
                        let _ = tx.send(MprisState {
                            is_playing,
                            is_paused,
                            is_stopped,
                            artist,
                            title,
                            album,
                        });
                    }
                }
            } else {
                let current = tx.borrow();
                if !current.is_stopped || !current.title.is_empty() {
                    drop(current);
                    let _ = tx.send(MprisState {
                        is_playing: false,
                        is_paused: false,
                        is_stopped: true,
                        artist: String::new(),
                        title: String::new(),
                        album: String::new(),
                    });
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }
}
