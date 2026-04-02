use crate::config::Config;
use crate::error::{FluxoError, Result};
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, AudioState};
use crate::utils::{TokenValue, format_template};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{Facility, InterestMaskSet};
use libpulse_binding::context::{Context, FlagSet as ContextFlag};
use libpulse_binding::mainloop::threaded::Mainloop as ThreadedMainloop;
use libpulse_binding::volume::Volume;
use tokio::sync::watch;
use tracing::error;

pub struct AudioDaemon;

impl AudioDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, state_tx: &watch::Sender<AudioState>) {
        let state_tx = state_tx.clone();

        std::thread::spawn(move || {
            let mut mainloop =
                ThreadedMainloop::new().expect("Failed to create pulse threaded mainloop");

            let mut context =
                Context::new(&mainloop, "fluxo-rs").expect("Failed to create pulse context");

            context
                .connect(None, ContextFlag::NOFLAGS, None)
                .expect("Failed to connect pulse context");

            mainloop.start().expect("Failed to start pulse mainloop");

            mainloop.lock();

            // Wait for context to be ready
            loop {
                match context.get_state() {
                    libpulse_binding::context::State::Ready => break,
                    libpulse_binding::context::State::Failed
                    | libpulse_binding::context::State::Terminated => {
                        error!("Pulse context failed or terminated");
                        mainloop.unlock();
                        return;
                    }
                    _ => {
                        mainloop.unlock();
                        std::thread::sleep(Duration::from_millis(50));
                        mainloop.lock();
                    }
                }
            }

            // Initial fetch
            let _ = fetch_audio_data_sync(&mut context, &state_tx);

            // Subscribe to events
            let interest =
                InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SERVER;
            context.subscribe(interest, |_| {});

            let (tx, rx) = std::sync::mpsc::channel();

            context.set_subscribe_callback(Some(Box::new(move |facility, _operation, _index| {
                match facility {
                    Some(Facility::Sink) | Some(Facility::Source) | Some(Facility::Server) => {
                        let _ = tx.send(());
                    }
                    _ => {}
                }
            })));

            mainloop.unlock();

            // Background polling loop driven by events or a 2s fallback timeout
            loop {
                let _ = rx.recv_timeout(Duration::from_secs(2));
                {
                    mainloop.lock();
                    let _ = fetch_audio_data_sync(&mut context, &state_tx);
                    mainloop.unlock();
                }
            }
        });
    }
}

use std::time::Duration;

fn fetch_audio_data_sync(
    context: &mut Context,
    state_tx: &watch::Sender<AudioState>,
) -> Result<()> {
    // We fetch all sinks and sources, and also server info to know defaults.
    // The order doesn't strictly matter as long as we update correctly.

    let tx_server = state_tx.clone();
    context.introspect().get_server_info(move |info| {
        let mut current = tx_server.borrow().clone();
        current.sink.name = info
            .default_sink_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        current.source.name = info
            .default_source_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let _ = tx_server.send(current);
    });

    let tx_sink = state_tx.clone();
    context.introspect().get_sink_info_list(move |res| {
        if let ListResult::Item(item) = res {
            let mut current = tx_sink.borrow().clone();
            // If this matches our default sink name, or if we don't have details for any yet
            let is_default = item
                .name
                .as_ref()
                .map(|s| s.as_ref() == current.sink.name)
                .unwrap_or(false);
            if is_default {
                current.sink.description = item
                    .description
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                current.sink.volume =
                    ((item.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.0).round() as u8;
                current.sink.muted = item.mute;
                let _ = tx_sink.send(current);
            }
        }
    });

    let tx_source = state_tx.clone();
    context.introspect().get_source_info_list(move |res| {
        if let ListResult::Item(item) = res {
            let mut current = tx_source.borrow().clone();
            let is_default = item
                .name
                .as_ref()
                .map(|s| s.as_ref() == current.source.name)
                .unwrap_or(false);
            if is_default {
                current.source.description = item
                    .description
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                current.source.volume =
                    ((item.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.0).round() as u8;
                current.source.muted = item.mute;
                let _ = tx_source.send(current);
            }
        }
    });

    Ok(())
}

pub struct AudioModule;

impl WaybarModule for AudioModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> Result<WaybarOutput> {
        let target_type = args.first().unwrap_or(&"sink");
        let action = args.get(1).unwrap_or(&"show");

        match *action {
            "cycle" => {
                self.cycle_device(target_type).await?;
                Ok(WaybarOutput::default())
            }
            "show" => self.get_status(config, state, target_type).await,
            other => Err(FluxoError::Module {
                module: "audio",
                message: format!("Unknown audio action: '{}'", other),
            }),
        }
    }
}

impl AudioModule {
    async fn get_status(
        &self,
        config: &Config,
        state: &AppReceivers,
        target_type: &str,
    ) -> Result<WaybarOutput> {
        let audio_state = state.audio.borrow().clone();

        let (name, description, volume, muted) = if target_type == "sink" {
            (
                audio_state.sink.name,
                audio_state.sink.description,
                audio_state.sink.volume,
                audio_state.sink.muted,
            )
        } else {
            (
                audio_state.source.name,
                audio_state.source.description,
                audio_state.source.volume,
                audio_state.source.muted,
            )
        };

        if name.is_empty() {
            // Fallback if daemon hasn't populated state yet
            return Ok(WaybarOutput {
                text: "Audio Loading...".to_string(),
                ..Default::default()
            });
        }

        let display_name = if description.len() > 20 {
            format!("{}...", &description[..17])
        } else {
            description.clone()
        };

        let (text, class) = if muted {
            let icon = if target_type == "sink" { "" } else { "" };
            let format_str = if target_type == "sink" {
                &config.audio.format_sink_muted
            } else {
                &config.audio.format_source_muted
            };
            let t = format_template(
                format_str,
                &[
                    ("name", TokenValue::String(display_name)),
                    ("icon", TokenValue::String(icon.to_string())),
                ],
            );
            (t, "muted")
        } else {
            let icon = if target_type == "sink" {
                if volume <= 30 {
                    ""
                } else if volume <= 60 {
                    ""
                } else {
                    ""
                }
            } else {
                ""
            };
            let format_str = if target_type == "sink" {
                &config.audio.format_sink_unmuted
            } else {
                &config.audio.format_source_unmuted
            };
            let t = format_template(
                format_str,
                &[
                    ("name", TokenValue::String(display_name)),
                    ("icon", TokenValue::String(icon.to_string())),
                    ("volume", TokenValue::Int(volume as i64)),
                ],
            );
            (t, "unmuted")
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(description),
            class: Some(class.to_string()),
            percentage: Some(volume),
        })
    }

    async fn cycle_device(&self, target_type: &str) -> Result<()> {
        // Keep using pactl for cycling for now as it's a rare action
        // but we could also implement it natively later.
        let set_cmd = if target_type == "sink" {
            "set-default-sink"
        } else {
            "set-default-source"
        };

        // We need to find the "next" device.
        // For simplicity, let's keep the CLI version for now or refactor later.
        // The user asked for "step by step".

        let list_cmd = if target_type == "sink" {
            "sinks"
        } else {
            "sources"
        };
        let output = tokio::process::Command::new("pactl")
            .args(["list", "short", list_cmd])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let devices: Vec<String> = stdout
            .lines()
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

        if devices.is_empty() {
            return Ok(());
        }

        let info_output = tokio::process::Command::new("pactl")
            .args(["info"])
            .output()
            .await?;
        let info_stdout = String::from_utf8_lossy(&info_output.stdout);
        let search_key = if target_type == "sink" {
            "Default Sink:"
        } else {
            "Default Source:"
        };
        let current_dev = info_stdout
            .lines()
            .find(|l| l.contains(search_key))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .unwrap_or("");

        let current_index = devices.iter().position(|d| d == current_dev).unwrap_or(0);
        let next_index = (current_index + 1) % devices.len();
        let next_dev = &devices[next_index];

        tokio::process::Command::new("pactl")
            .args([set_cmd, next_dev])
            .status()
            .await?;
        Ok(())
    }
}
