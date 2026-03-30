use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};
use anyhow::Result;

pub struct DiskModule;

impl WaybarModule for DiskModule {
    fn run(&self, config: &Config, state: &SharedState, args: &[&str]) -> Result<WaybarOutput> {
        let mountpoint = args.first().unwrap_or(&"/");

        let disks = if let Ok(s) = state.read() {
            s.disks.clone()
        } else {
            return Err(anyhow::anyhow!("Failed to read state"));
        };

        for disk in &disks {
            if disk.mount_point == *mountpoint {
                let total = disk.total_bytes as f64;
                let available = disk.available_bytes as f64;
                let used = total - available;

                let used_gb = used / 1024.0 / 1024.0 / 1024.0;
                let total_gb = total / 1024.0 / 1024.0 / 1024.0;
                let free_gb = available / 1024.0 / 1024.0 / 1024.0;

                let percentage = if total > 0.0 {
                    (used / total) * 100.0
                } else {
                    0.0
                };

                let class = if percentage > 95.0 {
                    "max"
                } else if percentage > 80.0 {
                    "high"
                } else {
                    "normal"
                };

                let text = format_template(
                    &config.disk.format,
                    &[
                        ("mount", TokenValue::String(mountpoint)),
                        ("used", TokenValue::Float(used_gb)),
                        ("total", TokenValue::Float(total_gb)),
                    ],
                );

                return Ok(WaybarOutput {
                    text,
                    tooltip: Some(format!(
                        "Used: {:.1}G\nTotal: {:.1}G\nFree: {:.1}G",
                        used_gb, total_gb, free_gb
                    )),
                    class: Some(class.to_string()),
                    percentage: Some(percentage as u8),
                });
            }
        }

        Err(anyhow::anyhow!("Mountpoint {} not found", mountpoint))
    }
}
