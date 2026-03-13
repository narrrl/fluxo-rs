use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct SysModule;

impl WaybarModule for SysModule {
    fn run(&self, _config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let (load1, load5, load15, uptime_secs, process_count) = {
            if let Ok(state_lock) = state.read() {
                (
                    state_lock.sys.load_1,
                    state_lock.sys.load_5,
                    state_lock.sys.load_15,
                    state_lock.sys.uptime,
                    state_lock.sys.process_count,
                )
            } else {
                (0.0, 0.0, 0.0, 0, 0)
            }
        };

        let hours = uptime_secs / 3600;
        let minutes = (uptime_secs % 3600) / 60;
        let uptime_str = if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        };

        Ok(WaybarOutput {
            text: format!("UP: {} | LOAD: {:.2} {:.2} {:.2}", uptime_str, load1, load5, load15),
            tooltip: Some(format!(
                "Uptime: {}\nProcesses: {}\nLoad Avg: {:.2}, {:.2}, {:.2}",
                uptime_str, process_count, load1, load5, load15
            )),
            class: Some("normal".to_string()),
            percentage: None,
        })
    }
}
