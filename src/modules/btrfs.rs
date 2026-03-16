use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{format_template, TokenValue};
use anyhow::Result;
use sysinfo::Disks;

pub struct BtrfsModule;

impl WaybarModule for BtrfsModule {
    fn run(&self, config: &Config, _state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let disks = Disks::new_with_refreshed_list();
        let mut total_used: f64 = 0.0;
        let mut total_size: f64 = 0.0;

        for disk in &disks {
            if disk.file_system().to_string_lossy().to_lowercase().contains("btrfs") {
                let size = disk.total_space() as f64;
                let available = disk.available_space() as f64;
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
            ]
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!("BTRFS Usage: {:.1}%", percentage)),
            class: Some(class.to_string()),
            percentage: Some(percentage as u8),
        })
    }
}
