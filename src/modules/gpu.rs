use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};

pub struct GpuModule;

impl WaybarModule for GpuModule {
    async fn run(
        &self,
        config: &Config,
        state: &SharedState,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let (active, vendor, usage, vram_used, vram_total, temp, model) = {
            let state_lock = state.read().await;
            (
                state_lock.gpu.active,
                state_lock.gpu.vendor.clone(),
                state_lock.gpu.usage,
                state_lock.gpu.vram_used,
                state_lock.gpu.vram_total,
                state_lock.gpu.temp,
                state_lock.gpu.model.clone(),
            )
        };

        if !active {
            return Ok(WaybarOutput {
                text: "No GPU".to_string(),
                tooltip: None,
                class: Some("normal".to_string()),
                percentage: None,
            });
        }

        let class = if usage > 95.0 {
            "max"
        } else if usage > 75.0 {
            "high"
        } else {
            "normal"
        };

        let format_str = match vendor.as_str() {
            "Intel" => &config.gpu.format_intel,
            "NVIDIA" => &config.gpu.format_nvidia,
            _ => &config.gpu.format_amd,
        };

        let text = format_template(
            format_str,
            &[
                ("usage", TokenValue::Float(usage)),
                ("vram_used", TokenValue::Float(vram_used)),
                ("vram_total", TokenValue::Float(vram_total)),
                ("temp", TokenValue::Float(temp)),
            ],
        );

        let tooltip = if vendor == "Intel" {
            format!("Model: {}\nApprox Usage: {:.0}%", model, usage)
        } else {
            format!(
                "Model: {}\nUsage: {:.0}%\nVRAM: {:.1}/{:.1}GB\nTemp: {:.1}°C",
                model, usage, vram_used, vram_total, temp
            )
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(tooltip),
            class: Some(class.to_string()),
            percentage: Some(usage as u8),
        })
    }
}
