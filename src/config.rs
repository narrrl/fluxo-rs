use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub network: NetworkConfig,
}

#[derive(Deserialize)]
pub struct NetworkConfig {
    pub format: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            format: "{interface} ({ip}):  {rx} MB/s   {tx} MB/s".to_string(),
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
