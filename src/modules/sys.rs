//! Uptime + load average renderer. Reads from the `sys` watch channel.

use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::AppReceivers;
use crate::utils::{TokenValue, format_template};

/// Renders uptime and load averages with a detailed tooltip.
pub struct SysModule;

impl WaybarModule for SysModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let (load1, load5, load15, uptime_secs, process_count) = {
            let state_lock = state.sys.borrow();
            (
                state_lock.load_1,
                state_lock.load_5,
                state_lock.load_15,
                state_lock.uptime,
                state_lock.process_count,
            )
        };

        let hours = uptime_secs / 3600;
        let minutes = (uptime_secs % 3600) / 60;
        let uptime_str = if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        };

        let text = format_template(
            &config.sys.format,
            &[
                ("uptime", TokenValue::String(uptime_str.clone())),
                ("load1", TokenValue::Float(load1)),
                ("load5", TokenValue::Float(load5)),
                ("load15", TokenValue::Float(load15)),
            ],
        );

        Ok(WaybarOutput {
            text,
            tooltip: Some(format!(
                "Uptime: {}\nProcesses: {}\nLoad Avg: {:.2}, {:.2}, {:.2}",
                uptime_str, process_count, load1, load5, load15
            )),
            class: Some("normal".to_string()),
            percentage: None,
        })
    }
}
