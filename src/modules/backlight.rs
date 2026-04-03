use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, BacklightState};
use crate::utils::{TokenValue, format_template};
use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info};

pub struct BacklightModule;

impl WaybarModule for BacklightModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let percentage = state.backlight.borrow().percentage;

        let icon = if percentage < 30 {
            "󰃞"
        } else if percentage < 70 {
            "󰃟"
        } else {
            "󰃠"
        };

        let text = format_template(
            &config.backlight.format,
            &[
                ("percentage", TokenValue::Int(percentage as i64)),
                ("icon", TokenValue::String(icon.to_string())),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("Brightness: {}%", percentage)),
            class: Some("normal".to_string()),
            percentage: Some(percentage),
        })
    }
}

pub struct BacklightDaemon;

impl BacklightDaemon {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, tx: watch::Sender<BacklightState>) {
        std::thread::spawn(move || {
            let base_dir = PathBuf::from("/sys/class/backlight");
            let mut device_dir = None;

            if let Ok(entries) = std::fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        device_dir = Some(path);
                        break;
                    }
                }
            }

            let Some(dir) = device_dir else {
                error!("No backlight device found in /sys/class/backlight");
                return;
            };

            info!("Monitoring backlight device: {:?}", dir);

            let max_brightness_path = dir.join("max_brightness");
            let brightness_path = dir.join("actual_brightness");
            let brightness_path_fallback = dir.join("brightness");

            let target_file = if brightness_path.exists() {
                brightness_path
            } else {
                brightness_path_fallback
            };

            let get_percentage = || -> u8 {
                let max: f64 = std::fs::read_to_string(&max_brightness_path)
                    .unwrap_or_default()
                    .trim()
                    .parse()
                    .unwrap_or(100.0);
                let current: f64 = std::fs::read_to_string(&target_file)
                    .unwrap_or_default()
                    .trim()
                    .parse()
                    .unwrap_or(0.0);

                if max > 0.0 {
                    ((current / max) * 100.0).round() as u8
                } else {
                    0
                }
            };

            // Initial poll
            let _ = tx.send(BacklightState {
                percentage: get_percentage(),
            });

            // Set up notify watcher
            let (ev_tx, ev_rx) = mpsc::channel();
            let mut watcher = RecommendedWatcher::new(
                move |res: notify::Result<Event>| {
                    if let Ok(event) = res {
                        if event.kind.is_modify() {
                            let _ = ev_tx.send(());
                        }
                    }
                },
                NotifyConfig::default(),
            )
            .unwrap();

            if let Err(e) = watcher.watch(&target_file, RecursiveMode::NonRecursive) {
                error!("Failed to watch backlight file: {}", e);
                return;
            }

            loop {
                // Block until an event occurs or a timeout to catch missed events
                if ev_rx.recv_timeout(Duration::from_secs(5)).is_ok() {
                    // Debounce rapid events
                    std::thread::sleep(Duration::from_millis(50));
                    while ev_rx.try_recv().is_ok() {}

                    let _ = tx.send(BacklightState {
                        percentage: get_percentage(),
                    });
                } else {
                    // Timeout hit, poll just in case
                    let current = get_percentage();
                    if tx.borrow().percentage != current {
                        let _ = tx.send(BacklightState {
                            percentage: current,
                        });
                    }
                }
            }
        });
    }
}
