use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct SysModule;

impl WaybarModule for SysModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
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

        let text = config.sys.format
            .replace("{uptime}", &uptime_str)
            .replace("{load1:>4.2}", &format!("{:>4.2}", load1))
            .replace("{load5:>4.2}", &format!("{:>4.2}", load5))
            .replace("{load15:>4.2}", &format!("{:>4.2}", load15))
            .replace("{load1}", &format!("{:.2}", load1))
            .replace("{load5}", &format!("{:.2}", load5))
            .replace("{load15}", &format!("{:.2}", load15));

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
