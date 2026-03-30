use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};
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

        let text = format_template(
            &config.cpu.format,
            &[
                ("usage", TokenValue::Float(usage)),
                ("temp", TokenValue::Float(temp)),
            ],
        );

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
