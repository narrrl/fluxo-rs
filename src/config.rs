use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub cpu: CpuConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub gpu: GpuConfig,
    #[serde(default)]
    pub sys: SysConfig,
    #[serde(default)]
    pub disk: DiskConfig,
    #[serde(default)]
    pub pool: PoolConfig,
    #[serde(default)]
    pub power: PowerConfig,
}

#[derive(Deserialize)]
pub struct NetworkConfig {
    pub format: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            format: "{interface} ({ip}):  {rx:>5.2} MB/s   {tx:>5.2} MB/s".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct CpuConfig {
    pub format: String,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            format: "CPU: {usage:>4.1}% {temp:>4.1}C".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct MemoryConfig {
    pub format: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            format: "{used:>5.2}/{total:>5.2}GB".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct GpuConfig {
    pub format_amd: String,
    pub format_intel: String,
    pub format_nvidia: String,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            format_amd: "AMD: {usage:>3.0}% {vram_used:>4.1}/{vram_total:>4.1}GB {temp:>4.1}C".to_string(),
            format_intel: "iGPU: {usage:>3.0}%".to_string(),
            format_nvidia: "NV: {usage:>3.0}% {vram_used:>4.1}/{vram_total:>4.1}GB {temp:>4.1}C".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct SysConfig {
    pub format: String,
}

impl Default for SysConfig {
    fn default() -> Self {
        Self {
            format: "UP: {uptime} | LOAD: {load1:>4.2} {load5:>4.2} {load15:>4.2}".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct DiskConfig {
    pub format: String,
}

impl Default for DiskConfig {
    fn default() -> Self {
        Self {
            format: "{mount} {used:>5.1}/{total:>5.1}G".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct PoolConfig {
    pub format: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            format: "{used:>4.0}G / {total:>4.0}G".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct PowerConfig {
    pub format: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            format: "{percentage:>3}%  {icon}".to_string(),
        }
    }
}

pub fn load_config() -> Config {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
            PathBuf::from(home).join(".config")
        });
    let config_path = config_dir.join("fluxo/config.toml");

    if let Ok(content) = fs::read_to_string(config_path) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}
