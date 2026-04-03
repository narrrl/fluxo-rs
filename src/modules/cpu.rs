use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::AppReceivers;
use crate::utils::{TokenValue, classify_usage, format_template};

pub struct CpuModule;

impl WaybarModule for CpuModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let (usage, temp, model) = {
            let s = state.cpu.borrow();
            (s.usage, s.temp, s.model.clone())
        };

        let text = format_template(
            &config.cpu.format,
            &[
                ("usage", TokenValue::Float(usage)),
                ("temp", TokenValue::Float(temp)),
            ],
        );

        let class = classify_usage(usage, 75.0, 95.0);

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

    #[tokio::test]
    async fn test_cpu_normal() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 25.0,
                temp: 45.0,
                model: "Test CPU".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state.receivers, &[]).await.unwrap();
        assert!(output.text.contains("25.0"));
        assert!(output.text.contains("45.0"));
        assert_eq!(output.class.as_deref(), Some("normal"));
        assert_eq!(output.percentage, Some(25));
        assert_eq!(output.tooltip.as_deref(), Some("Test CPU"));
    }

    #[tokio::test]
    async fn test_cpu_high() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 80.0,
                temp: 70.0,
                model: "Test".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state.receivers, &[]).await.unwrap();
        assert_eq!(output.class.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn test_cpu_max() {
        let state = mock_state(AppState {
            cpu: CpuState {
                usage: 99.0,
                temp: 95.0,
                model: "Test".into(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = CpuModule.run(&config, &state.receivers, &[]).await.unwrap();
        assert_eq!(output.class.as_deref(), Some("max"));
    }
}
