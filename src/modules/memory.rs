use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};

pub struct MemoryModule;

impl WaybarModule for MemoryModule {
    async fn run(
        &self,
        config: &Config,
        state: &SharedState,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let (used_gb, total_gb) = {
            let state_lock = state.read().await;
            (state_lock.memory.used_gb, state_lock.memory.total_gb)
        };

        let ratio = if total_gb > 0.0 {
            (used_gb / total_gb) * 100.0
        } else {
            0.0
        };

        let text = format_template(
            &config.memory.format,
            &[
                ("used", TokenValue::Float(used_gb)),
                ("total", TokenValue::Float(total_gb)),
            ],
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, MemoryState, mock_state};

    #[tokio::test]
    async fn test_memory_normal() {
        let state = mock_state(AppState {
            memory: MemoryState {
                used_gb: 8.0,
                total_gb: 32.0,
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = MemoryModule.run(&config, &state, &[]).await.unwrap();
        assert!(output.text.contains("8.00"));
        assert!(output.text.contains("32.00"));
        assert_eq!(output.class.as_deref(), Some("normal"));
        assert_eq!(output.percentage, Some(25)); // 8/32 = 25%
    }

    #[tokio::test]
    async fn test_memory_high() {
        let state = mock_state(AppState {
            memory: MemoryState {
                used_gb: 26.0,
                total_gb: 32.0,
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = MemoryModule.run(&config, &state, &[]).await.unwrap();
        assert_eq!(output.class.as_deref(), Some("high")); // 81%
    }

    #[tokio::test]
    async fn test_memory_zero_total() {
        let state = mock_state(AppState {
            memory: MemoryState {
                used_gb: 0.0,
                total_gb: 0.0,
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = MemoryModule.run(&config, &state, &[]).await.unwrap();
        assert_eq!(output.class.as_deref(), Some("normal"));
        assert_eq!(output.percentage, Some(0));
    }
}
