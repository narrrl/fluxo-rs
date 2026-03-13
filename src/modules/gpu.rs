use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub struct GpuModule;

impl WaybarModule for GpuModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
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

        let format_str = match vendor.as_str() {
            "Intel" => &config.gpu.format_intel,
            "NVIDIA" => &config.gpu.format_nvidia,
            _ => &config.gpu.format_amd,
        };

        let text = format_str
            .replace("{usage:>3.0}", &format!("{:>3.0}", usage))
            .replace("{vram_used:>4.1}", &format!("{:>4.1}", vram_used))
            .replace("{vram_total:>4.1}", &format!("{:>4.1}", vram_total))
            .replace("{temp:>4.1}", &format!("{:>4.1}", temp))
            .replace("{usage}", &format!("{:.0}", usage))
            .replace("{vram_used}", &format!("{:.1}", vram_used))
            .replace("{vram_total}", &format!("{:.1}", vram_total))
            .replace("{temp}", &format!("{:.1}", temp));

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
