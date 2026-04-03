use crate::config::Config;
use crate::error::{FluxoError, Result};
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::AppReceivers;
use crate::utils::{TokenValue, format_template};

pub struct DiskModule;

impl WaybarModule for DiskModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> Result<WaybarOutput> {
        let mountpoint = args.first().unwrap_or(&"/");

        let disks = state.disks.borrow().clone();

        if disks.is_empty() {
            return Ok(WaybarOutput {
                text: "Disk Loading...".to_string(),
                ..Default::default()
            });
        }

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
                        ("mount", TokenValue::String(mountpoint.to_string())),
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

        Err(FluxoError::Module {
            module: "disk",
            message: format!("Mountpoint {} not found", mountpoint),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, DiskInfo, MockState, mock_state};

    fn state_with_disk(mount: &str, total: u64, available: u64) -> MockState {
        mock_state(AppState {
            disks: vec![DiskInfo {
                mount_point: mount.to_string(),
                filesystem: "ext4".to_string(),
                total_bytes: total,
                available_bytes: available,
            }],
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn test_disk_found() {
        let gb = 1024 * 1024 * 1024;
        let state = state_with_disk("/", 100 * gb, 60 * gb);
        let config = Config::default();
        let output = DiskModule
            .run(&config, &state.receivers, &["/"])
            .await
            .unwrap();
        assert_eq!(output.class.as_deref(), Some("normal"));
        assert_eq!(output.percentage, Some(40)); // 40% used
    }

    #[tokio::test]
    async fn test_disk_high() {
        let gb = 1024 * 1024 * 1024;
        let state = state_with_disk("/", 100 * gb, 15 * gb);
        let config = Config::default();
        let output = DiskModule
            .run(&config, &state.receivers, &["/"])
            .await
            .unwrap();
        assert_eq!(output.class.as_deref(), Some("high")); // 85% used
    }

    #[tokio::test]
    async fn test_disk_not_found() {
        let state = mock_state(AppState::default());
        let config = Config::default();
        let result = DiskModule
            .run(&config, &state.receivers, &["/nonexistent"])
            .await;
        assert!(result.is_err());
    }
}
