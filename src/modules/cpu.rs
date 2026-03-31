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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, CpuState, mock_state};

    #[test]
    fn test_cpu_normal() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 25.0,
                temp: 45.0,
                model: "Test CPU".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state, &[]).unwrap();
        assert!(output.text.contains("25.0"));
        assert!(output.text.contains("45.0"));
        assert_eq!(output.class.as_deref(), Some("normal"));
        assert_eq!(output.percentage, Some(25));
        assert_eq!(output.tooltip.as_deref(), Some("Test CPU"));
    }

    #[test]
    fn test_cpu_high() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 80.0,
                temp: 70.0,
                model: "Test".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state, &[]).unwrap();
        assert_eq!(output.class.as_deref(), Some("high"));
    }

    #[test]
    fn test_cpu_max() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 99.0,
                temp: 95.0,
                model: "Test".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state, &[]).unwrap();
        assert_eq!(output.class.as_deref(), Some("max"));
    }
}
