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
use tokio::sync::{mpsc, watch};
use tracing::error;

pub enum AudioCommand {
    ChangeVolume {
        is_sink: bool,
        step_val: u32,
        is_up: bool,
    },
    ToggleMute {
        is_sink: bool,
    },
    CycleDevice {
        is_sink: bool,
    },
}

pub struct AudioDaemon;

impl AudioDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(
        &self,
        state_tx: &watch::Sender<AudioState>,
        mut cmd_rx: mpsc::Receiver<AudioCommand>,
    ) {
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
            let tx_cb = tx.clone();

            context.set_subscribe_callback(Some(Box::new(move |facility, _operation, _index| {
                match facility {
                    Some(Facility::Sink) | Some(Facility::Source) | Some(Facility::Server) => {
                        let _ = tx_cb.send(());
                    }
                    _ => {}
                }
            })));

            mainloop.unlock();

            loop {
                if let Ok(cmd) = cmd_rx.try_recv() {
                    mainloop.lock();
                    match cmd {
                        AudioCommand::ChangeVolume {
                            is_sink,
                            step_val,
                            is_up,
                        } => {
                            let current = state_tx.borrow().clone();
                            let (name, mut vol, channels) = if is_sink {
                                (
                                    current.sink.name.clone(),
                                    current.sink.volume,
                                    current.sink.channels,
                                )
                            } else {
                                (
                                    current.source.name.clone(),
                                    current.source.volume,
                                    current.source.channels,
                                )
                            };

                            if is_up {
                                vol = vol.saturating_add(step_val as u8).min(150);
                            } else {
                                vol = vol.saturating_sub(step_val as u8);
                            }

                            let pulse_vol = Volume(
                                (vol as f64 / 100.0 * Volume::NORMAL.0 as f64).round() as u32,
                            );
                            let mut cvol = libpulse_binding::volume::ChannelVolumes::default();
                            cvol.set(channels.max(1), pulse_vol);

                            if is_sink {
                                context
                                    .introspect()
                                    .set_sink_volume_by_name(&name, &cvol, None);
                            } else {
                                context
                                    .introspect()
                                    .set_source_volume_by_name(&name, &cvol, None);
                            }
                        }
                        AudioCommand::ToggleMute { is_sink } => {
                            let current = state_tx.borrow().clone();
                            let (name, muted) = if is_sink {
                                (current.sink.name.clone(), current.sink.muted)
                            } else {
                                (current.source.name.clone(), current.source.muted)
                            };

                            if is_sink {
                                context
                                    .introspect()
                                    .set_sink_mute_by_name(&name, !muted, None);
                            } else {
                                context
                                    .introspect()
                                    .set_source_mute_by_name(&name, !muted, None);
                            }
                        }
                        AudioCommand::CycleDevice { is_sink } => {
                            let current = state_tx.borrow().clone();
                            let current_name = if is_sink {
                                current.sink.name.clone()
                            } else {
                                current.source.name.clone()
                            };

                            let devices = if is_sink {
                                &current.available_sinks
                            } else {
                                &current.available_sources
                            };
                            if !devices.is_empty() {
                                let current_idx =
                                    devices.iter().position(|d| d == &current_name).unwrap_or(0);
                                let next_idx = (current_idx + 1) % devices.len();
                                let next_dev = &devices[next_idx];

                                if is_sink {
                                    context.set_default_sink(next_dev, |_| {});
                                } else {
                                    context.set_default_source(next_dev, |_| {});
                                }
                            }
                        }
                    }
                    mainloop.unlock();
                    let _ = tx.send(());
                }

                let _ = rx.recv_timeout(Duration::from_millis(50));
                while rx.try_recv().is_ok() {}

                mainloop.lock();

                // Fetch data and update available sinks/sources
                let _ = fetch_audio_data_sync(&mut context, &state_tx);

                mainloop.unlock();
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
        let mut current = tx_sink.borrow().clone();
        match res {
            ListResult::Item(item) => {
                if let Some(name) = item.name.as_ref() {
                    let name_str = name.to_string();
                    if !current.available_sinks.contains(&name_str) {
                        current.available_sinks.push(name_str);
                    }
                }

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
                    current.sink.volume = ((item.volume.avg().0 as f64 / Volume::NORMAL.0 as f64)
                        * 100.0)
                        .round() as u8;
                    current.sink.muted = item.mute;
                    current.sink.channels = item.volume.len();
                }
                let _ = tx_sink.send(current);
            }
            ListResult::End => {
                // Clear the list on End so it rebuilds fresh next time
                current.available_sinks.clear();
                let _ = tx_sink.send(current);
            }
            ListResult::Error => {}
        }
    });

    let tx_source = state_tx.clone();
    context.introspect().get_source_info_list(move |res| {
        let mut current = tx_source.borrow().clone();
        match res {
            ListResult::Item(item) => {
                if let Some(name) = item.name.as_ref() {
                    let name_str = name.to_string();
                    // PulseAudio includes monitor sources, ignore them if we want to
                    if !name_str.contains(".monitor")
                        && !current.available_sources.contains(&name_str)
                    {
                        current.available_sources.push(name_str);
                    }
                }

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
                    current.source.volume = ((item.volume.avg().0 as f64 / Volume::NORMAL.0 as f64)
                        * 100.0)
                        .round() as u8;
                    current.source.muted = item.mute;
                    current.source.channels = item.volume.len();
                }
                let _ = tx_source.send(current);
            }
            ListResult::End => {
                // Clear the list on End so it rebuilds fresh next time
                current.available_sources.clear();
                let _ = tx_source.send(current);
            }
            ListResult::Error => {}
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
        let step = args.get(2).unwrap_or(&"5");

        match *action {
            "up" => {
                self.change_volume(state, target_type, step, true).await?;
                Ok(WaybarOutput::default())
            }
            "down" => {
                self.change_volume(state, target_type, step, false).await?;
                Ok(WaybarOutput::default())
            }
            "mute" => {
                self.toggle_mute(state, target_type).await?;
                Ok(WaybarOutput::default())
            }
            "cycle" => {
                self.cycle_device(state, target_type).await?;
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

    async fn change_volume(
        &self,
        state: &AppReceivers,
        target_type: &str,
        step: &str,
        is_up: bool,
    ) -> Result<()> {
        let is_sink = target_type == "sink";
        let step_val: u32 = step.parse().unwrap_or(5);
        let _ = state
            .audio_cmd_tx
            .send(AudioCommand::ChangeVolume {
                is_sink,
                step_val,
                is_up,
            })
            .await;
        Ok(())
    }

    async fn toggle_mute(&self, state: &AppReceivers, target_type: &str) -> Result<()> {
        let is_sink = target_type == "sink";
        let _ = state
            .audio_cmd_tx
            .send(AudioCommand::ToggleMute { is_sink })
            .await;
        Ok(())
    }

    async fn cycle_device(&self, state: &AppReceivers, target_type: &str) -> Result<()> {
        let is_sink = target_type == "sink";
        let _ = state
            .audio_cmd_tx
            .send(AudioCommand::CycleDevice { is_sink })
            .await;
        Ok(())
    }
}
