use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, MprisScrollState, MprisState};
use crate::utils::{TokenValue, format_template};
use std::sync::Arc;
use tokio::sync::{RwLock, watch};
use tokio::time::Duration;
use tracing::{debug, info};
use zbus::{Connection, proxy};

fn format_mpris_text(format: &str, mpris: &MprisState) -> (String, &'static str) {
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
        format,
        &[
            ("artist", TokenValue::String(mpris.artist.clone())),
            ("title", TokenValue::String(mpris.title.clone())),
            ("album", TokenValue::String(mpris.album.clone())),
            ("status_icon", TokenValue::String(status_icon.to_string())),
        ],
    );

    (text, class)
}

fn apply_scroll_window(full_text: &str, max_len: usize, offset: usize, separator: &str) -> String {
    let char_count = full_text.chars().count();
    let total_len = char_count + separator.chars().count();
    let offset = offset % total_len;
    full_text
        .chars()
        .chain(separator.chars())
        .cycle()
        .skip(offset)
        .take(max_len)
        .collect()
}

fn truncate_with_ellipsis(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
    format!("{}...", truncated)
}

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

        let (full_text, class) = format_mpris_text(&config.mpris.format, &mpris);

        let text = if config.mpris.scroll {
            if let Some(max_len) = config.mpris.max_length {
                let scroll = state.mpris_scroll.read().await;
                apply_scroll_window(
                    &full_text,
                    max_len,
                    scroll.offset,
                    &config.mpris.scroll_separator,
                )
            } else {
                full_text.clone()
            }
        } else if let Some(max_len) = config.mpris.max_length {
            truncate_with_ellipsis(&full_text, max_len)
        } else {
            full_text.clone()
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("{} - {}", mpris.artist, mpris.title)),
            class: Some(class.to_string()),
            percentage: None,
        })
    }
}

pub async fn mpris_scroll_ticker(
    config: Arc<RwLock<Config>>,
    mut mpris_rx: watch::Receiver<MprisState>,
    scroll_state: Arc<RwLock<MprisScrollState>>,
    tick_tx: watch::Sender<u64>,
) {
    let mut generation: u64 = 0;
    let mut last_track_key = String::new();

    loop {
        let mpris = mpris_rx.borrow_and_update().clone();
        let cfg = config.read().await;
        let scroll_enabled = cfg.mpris.scroll;
        let has_max_length = cfg.mpris.max_length.is_some();
        let scroll_speed = cfg.mpris.scroll_speed;
        let format_str = cfg.mpris.format.clone();
        drop(cfg);

        let (full_text, _) = format_mpris_text(&format_str, &mpris);
        let track_key = format!("{}|{}|{}", mpris.artist, mpris.title, mpris.album);

        if track_key != last_track_key {
            let mut state = scroll_state.write().await;
            state.offset = 0;
            state.full_text = full_text.clone();
            last_track_key = track_key;
            generation += 1;
            let _ = tick_tx.send(generation);
        }

        if scroll_enabled && has_max_length && mpris.is_playing {
            tokio::time::sleep(Duration::from_millis(scroll_speed)).await;
            let mut state = scroll_state.write().await;
            state.offset += 1;
            state.full_text = full_text;
            drop(state);
            generation += 1;
            let _ = tick_tx.send(generation);
            continue;
        }

        // Not scrolling — wait for next state change
        if mpris_rx.changed().await.is_err() {
            break;
        }
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
