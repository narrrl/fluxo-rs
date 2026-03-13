use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct CpuModule;

impl WaybarModule for CpuModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let (usage, temp, model) = {
            if let Ok(state_lock) = state.read() {
                (
                    state_lock.cpu.usage,
                    state_lock.cpu.temp,
                    state_lock.cpu.model.clone(),
                )
            } else {
                (0.0, 0.0, String::from("Unknown"))
            }
        };

        let text = config.cpu.format
            .replace("{usage:>4.1}", &format!("{:>4.1}", usage))
            .replace("{temp:>4.1}", &format!("{:>4.1}", temp))
            .replace("{usage}", &format!("{:.1}", usage))
            .replace("{temp}", &format!("{:.1}", temp));
        
        let class = if usage > 95.0 {
            "max"
        } else if usage > 75.0 {
            "high"
        } else {
            "normal"
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(model),
            class: Some(class.to_string()),
            percentage: Some(usage as u8),
        })
    }
}
