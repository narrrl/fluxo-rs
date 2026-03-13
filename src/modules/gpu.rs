use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct GpuModule;

impl WaybarModule for GpuModule {
    fn run(&self, _config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let (active, vendor, usage, vram_used, vram_total, temp, model) = {
            if let Ok(state_lock) = state.read() {
                (
                    state_lock.gpu.active,
                    state_lock.gpu.vendor.clone(),
                    state_lock.gpu.usage,
                    state_lock.gpu.vram_used,
                    state_lock.gpu.vram_total,
                    state_lock.gpu.temp,
                    state_lock.gpu.model.clone(),
                )
            } else {
                (false, String::from("Unknown"), 0.0, 0.0, 0.0, 0.0, String::from("Unknown"))
            }
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

        let text = if vendor == "Intel" {
            // Intel usually doesn't expose easy VRAM or Temp without root
            format!("iGPU: {:.0}%", usage)
        } else {
            format!("{}: {:.0}% {:.1}/{:.1}GB {:.1}C", vendor, usage, vram_used, vram_total, temp)
        };

        let tooltip = if vendor == "Intel" {
            format!("Model: {}\nApprox Usage: {:.0}%", model, usage)
        } else {
            format!("Model: {}\nUsage: {:.0}%\nVRAM: {:.1}/{:.1}GB\nTemp: {:.1}°C", model, usage, vram_used, vram_total, temp)
        };

        Ok(WaybarOutput {
            text,
            tooltip: Some(tooltip),
            class: Some(class.to_string()),
            percentage: Some(usage as u8),
        })
    }
}
