//! PulseAudio/PipeWire sink + source indicator with live event subscription.
//!
//! The daemon runs on its own OS thread because libpulse's threaded mainloop
//! must drive callbacks inside its own lock scope. Volume/mute changes are
//! routed back via an async [`mpsc`] channel — the module handlers [`run`]s
//! only push commands; the thread performs the actual libpulse calls.

use crate::config::Config;
use crate::error::{FluxoError, Result};
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, AudioDeviceInfo, AudioState};
use crate::utils::{TokenValue, format_template};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{Facility, InterestMaskSet};
use libpulse_binding::context::{Context, FlagSet as ContextFlag};
use libpulse_binding::mainloop::threaded::Mainloop as ThreadedMainloop;
use libpulse_binding::volume::Volume;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::error;

/// Commands the module handler sends to the audio daemon thread.
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

/// Long-lived daemon driving libpulse's threaded mainloop.
pub struct AudioDaemon;

impl AudioDaemon {
    /// Construct a new (stateless) daemon.
    pub fn new() -> Self {
        Self
    }

    /// Spawn the audio thread, subscribe to sink/source/server events, and
    /// start consuming [`AudioCommand`]s.
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

            let _ = fetch_audio_data_sync(&mut context, &state_tx);

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

                let _ = fetch_audio_data_sync(&mut context, &state_tx);

                mainloop.unlock();
            }
        });
    }
}

use std::time::Duration;

/// Trigger async libpulse introspection: server defaults + sink/source lists.
/// Callbacks publish onto `state_tx` as results land.
fn fetch_audio_data_sync(
    context: &mut Context,
    state_tx: &watch::Sender<AudioState>,
) -> Result<()> {
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

    fetch_sinks(context, state_tx);
    fetch_sources(context, state_tx);

    Ok(())
}

/// Shared bookkeeping for a device list fetch.
struct PendingList {
    names: Arc<std::sync::Mutex<Vec<String>>>,
}

impl PendingList {
    fn new() -> Self {
        Self {
            names: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn push(&self, name: String) {
        self.names.lock().unwrap().push(name);
    }

    fn drain(&self) -> Vec<String> {
        self.names.lock().unwrap().drain(..).collect()
    }
}

/// Extract common device info from a pulse item's volume/mute/description fields.
fn device_info_from(
    description: Option<&str>,
    volume: &libpulse_binding::volume::ChannelVolumes,
    muted: bool,
) -> (String, u8, bool, u8) {
    let desc = description.unwrap_or_default().to_string();
    let vol = ((volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.0).round() as u8;
    let channels = volume.len();
    (desc, vol, muted, channels)
}

/// Write `info` into `target` only when `item_name` matches the currently
/// selected default device — other sinks/sources are ignored here.
fn apply_device_info(target: &mut AudioDeviceInfo, item_name: &str, info: (String, u8, bool, u8)) {
    if item_name == target.name {
        target.description = info.0;
        target.volume = info.1;
        target.muted = info.2;
        target.channels = info.3;
    }
}

/// Dispatch `get_sink_info_list` and collect names into `available_sinks`.
fn fetch_sinks(context: &mut Context, state_tx: &watch::Sender<AudioState>) {
    let tx = state_tx.clone();
    let pending = PendingList::new();
    let pending_cb = PendingList {
        names: Arc::clone(&pending.names),
    };
    context.introspect().get_sink_info_list(move |res| {
        let mut current = tx.borrow().clone();
        match res {
            ListResult::Item(item) => {
                let name_str = item
                    .name
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !name_str.is_empty() {
                    pending_cb.push(name_str.clone());
                }
                let info = device_info_from(
                    item.description.as_ref().map(|s| s.as_ref()),
                    &item.volume,
                    item.mute,
                );
                apply_device_info(&mut current.sink, &name_str, info);
                let _ = tx.send(current);
            }
            ListResult::End => {
                current.available_sinks = pending_cb.drain();
                let _ = tx.send(current);
            }
            ListResult::Error => {}
        }
    });
}

/// Dispatch `get_source_info_list` and collect names (skipping `.monitor`
/// virtual sources) into `available_sources`.
fn fetch_sources(context: &mut Context, state_tx: &watch::Sender<AudioState>) {
    let tx = state_tx.clone();
    let pending = PendingList::new();
    let pending_cb = PendingList {
        names: Arc::clone(&pending.names),
    };
    context.introspect().get_source_info_list(move |res| {
        let mut current = tx.borrow().clone();
        match res {
            ListResult::Item(item) => {
                let name_str = item
                    .name
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !name_str.is_empty() && !name_str.contains(".monitor") {
                    pending_cb.push(name_str.clone());
                }
                let info = device_info_from(
                    item.description.as_ref().map(|s| s.as_ref()),
                    &item.volume,
                    item.mute,
                );
                apply_device_info(&mut current.source, &name_str, info);
                let _ = tx.send(current);
            }
            ListResult::End => {
                current.available_sources = pending_cb.drain();
                let _ = tx.send(current);
            }
            ListResult::Error => {}
        }
    });
}

/// Renders sink/source + dispatches volume/mute/cycle commands.
/// Args: `[sink|source] [show|up|down|mute|cycle] [step]`.
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
                self.get_status(config, state, target_type).await
            }
            "down" => {
                self.change_volume(state, target_type, step, false).await?;
                self.get_status(config, state, target_type).await
            }
            "mute" => {
                self.toggle_mute(state, target_type).await?;
                self.get_status(config, state, target_type).await
            }
            "cycle" => {
                self.cycle_device(state, target_type).await?;
                self.get_status(config, state, target_type).await
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
