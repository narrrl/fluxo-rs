use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};
use anyhow::Result;

pub struct BtrfsModule;

impl WaybarModule for BtrfsModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let disks = if let Ok(s) = state.read() {
            s.disks.clone()
        } else {
            return Err(anyhow::anyhow!("Failed to read state"));
        };

        let mut total_used: f64 = 0.0;
        let mut total_size: f64 = 0.0;

        for disk in &disks {
            if disk.filesystem.contains("btrfs") {
                let size = disk.total_bytes as f64;
                let available = disk.available_bytes as f64;
                total_size += size;
                total_used += size - available;
            }
        }

        if total_size == 0.0 {
            return Ok(WaybarOutput {
                text: "No BTRFS".to_string(),
                tooltip: None,
                class: Some("normal".to_string()),
                percentage: None,
            });
        }

        let used_gb = total_used / 1024.0 / 1024.0 / 1024.0;
        let size_gb = total_size / 1024.0 / 1024.0 / 1024.0;
        let percentage = (total_used / total_size) * 100.0;

        let class = if percentage > 95.0 {
            "max"
        } else if percentage > 80.0 {
            "high"
        } else {
            "normal"
        };

        let text = format_template(
            &config.pool.format,
            &[
                ("used", TokenValue::Float(used_gb)),
                ("total", TokenValue::Float(size_gb)),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("BTRFS Usage: {:.1}%", percentage)),
            class: Some(class.to_string()),
            percentage: Some(percentage as u8),
        })
    }
}
