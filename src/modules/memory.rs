use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct MemoryModule;

impl WaybarModule for MemoryModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let (used_gb, total_gb) = {
            if let Ok(state_lock) = state.read() {
                (
                    state_lock.memory.used_gb,
                    state_lock.memory.total_gb,
                )
            } else {
                (0.0, 0.0)
            }
        };

        let ratio = if total_gb > 0.0 { (used_gb / total_gb) * 100.0 } else { 0.0 };

        let text = config.memory.format
            .replace("{used:>5.2}", &format!("{:>5.2}", used_gb))
            .replace("{total:>5.2}", &format!("{:>5.2}", total_gb))
            .replace("{used}", &format!("{:.2}", used_gb))
            .replace("{total}", &format!("{:.2}", total_gb));

        let class = if ratio > 95.0 {
            "max"
        } else if ratio > 75.0 {
            "high"
        } else {
            "normal"
        };

        Ok(WaybarOutput {
            text,
            tooltip: None,
            class: Some(class.to_string()),
            percentage: Some(ratio as u8),
        })
    }
}
