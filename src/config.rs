use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

#[derive(Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub signals: SignalsConfig,
    #[cfg(feature = "mod-network")]
    #[serde(default)]
    pub network: NetworkConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub cpu: CpuConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub memory: MemoryConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub gpu: GpuConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub sys: SysConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub disk: DiskConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub pool: PoolConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub power: PowerConfig,
    #[cfg(feature = "mod-audio")]
    #[serde(default)]
    pub audio: AudioConfig,
    #[cfg(feature = "mod-bt")]
    #[serde(default)]
    pub bt: BtConfig,
    #[cfg(feature = "mod-hardware")]
    #[serde(default)]
    pub game: GameConfig,
    #[cfg(feature = "mod-dbus")]
    #[serde(default)]
    pub mpris: MprisConfig,
    #[cfg(feature = "mod-dbus")]
    #[serde(default)]
    pub backlight: BacklightConfig,
    #[cfg(feature = "mod-dbus")]
    #[serde(default)]
    pub keyboard: KeyboardConfig,
    #[cfg(feature = "mod-dbus")]
    #[serde(default)]
    pub dnd: DndConfig,
}

#[derive(Deserialize, Clone)]
pub struct GeneralConfig {
    pub menu_command: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            menu_command: "fuzzel --dmenu --prompt \"$FLUXO_PROMPT\"".to_string(),
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize, Default, Clone)]
pub struct SignalsConfig {
    pub network: Option<i32>,
    pub cpu: Option<i32>,
    pub memory: Option<i32>,
    pub gpu: Option<i32>,
    pub sys: Option<i32>,
    pub disk: Option<i32>,
    pub pool: Option<i32>,
    pub power: Option<i32>,
    pub audio: Option<i32>,
    pub bt: Option<i32>,
    pub game: Option<i32>,
    pub mpris: Option<i32>,
    pub backlight: Option<i32>,
    pub keyboard: Option<i32>,
    pub dnd: Option<i32>,
}

#[derive(Deserialize, Clone)]
pub struct NetworkConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{interface} ({ip}):  {rx:>5.2} MB/s   {tx:>5.2} MB/s".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct CpuConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "CPU: {usage:>4.1}% {temp:>4.1}C".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{used:>5.2}/{total:>5.2}GB".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct GpuConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format_amd: String,
    pub format_intel: String,
    pub format_nvidia: String,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format_amd: "AMD: {usage:>3.0}% {vram_used:>4.1}/{vram_total:>4.1}GB {temp:>4.1}C"
                .to_string(),
            format_intel: "iGPU: {usage:>3.0}%".to_string(),
            format_nvidia: "NV: {usage:>3.0}% {vram_used:>4.1}/{vram_total:>4.1}GB {temp:>4.1}C"
                .to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct SysConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for SysConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "UP: {uptime} | LOAD: {load1:>4.2} {load5:>4.2} {load15:>4.2}".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct DiskConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for DiskConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{mount} {used:>5.1}/{total:>5.1}G".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct PoolConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{used:>4.0}G / {total:>4.0}G".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct PowerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{percentage:>3}%  {icon}".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct AudioConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format_sink_unmuted: String,
    pub format_sink_muted: String,
    pub format_source_unmuted: String,
    pub format_source_muted: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format_sink_unmuted: "{name} {volume:>3}% {icon}".to_string(),
            format_sink_muted: "{name} {icon}".to_string(),
            format_source_unmuted: "{name} {volume:>3}% {icon}".to_string(),
            format_source_muted: "{name} {icon}".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct BtConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format_connected: String,
    pub format_plugin: String,
    pub format_disconnected: String,
    pub format_disabled: String,
}

impl Default for BtConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format_connected: "{alias} 󰂰".to_string(),
            format_plugin: "{alias} [{left}|{right}] {anc} 󰂰".to_string(),
            format_disconnected: "󰂯".to_string(),
            format_disabled: "󰂲 Off".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct GameConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format_active: String,
    pub format_inactive: String,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format_active: "<span size='large'>󰊖</span>".to_string(),
            format_inactive: "<span size='large'></span>".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct MprisConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub scroll: bool,
    #[serde(default = "default_scroll_speed")]
    pub scroll_speed: u64,
    #[serde(default = "default_scroll_separator")]
    pub scroll_separator: String,
}

fn default_scroll_speed() -> u64 {
    500
}

fn default_scroll_separator() -> String {
    " /// ".to_string()
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{status_icon} {artist} - {title}".to_string(),
            max_length: None,
            scroll: false,
            scroll_speed: 500,
            scroll_separator: " /// ".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct BacklightConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{percentage:>3}% {icon}".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct KeyboardConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: String,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{layout}".to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct DndConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format_dnd: String,
    pub format_normal: String,
}

fn default_true() -> bool {
    true
}

impl Default for DndConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format_dnd: "<span size='large'>󰂛</span>".to_string(),
            format_normal: "<span size='large'>󰂚</span>".to_string(),
        }
    }
}

fn extract_tokens(format_str: &str) -> Vec<String> {
    crate::utils::TOKEN_RE
        .captures_iter(format_str)
        .map(|cap| cap[1].to_string())
        .collect()
}

fn validate_format(label: &str, format_str: &str, known_tokens: &[&str]) {
    for token in extract_tokens(format_str) {
        if !known_tokens.contains(&token.as_str()) {
            warn!(
                "Config [{}]: unknown token '{{{}}}' in format string. Known tokens: {:?}",
                label, token, known_tokens
            );
        }
    }
}

impl Config {
    /// Check if a module is enabled in the configuration.
    /// Returns false if the module is explicitly disabled; true if enabled or unknown.
    pub fn is_module_enabled(&self, module_name: &str) -> bool {
        match module_name {
            #[cfg(feature = "mod-network")]
            "net" | "network" => self.network.enabled,
            #[cfg(feature = "mod-hardware")]
            "cpu" => self.cpu.enabled,
            #[cfg(feature = "mod-hardware")]
            "mem" | "memory" => self.memory.enabled,
            #[cfg(feature = "mod-hardware")]
            "gpu" => self.gpu.enabled,
            #[cfg(feature = "mod-hardware")]
            "sys" => self.sys.enabled,
            #[cfg(feature = "mod-hardware")]
            "disk" => self.disk.enabled,
            #[cfg(feature = "mod-hardware")]
            "pool" | "btrfs" => self.pool.enabled,
            #[cfg(feature = "mod-hardware")]
            "power" => self.power.enabled,
            #[cfg(feature = "mod-hardware")]
            "game" => self.game.enabled,
            #[cfg(feature = "mod-audio")]
            "vol" | "audio" | "mic" => self.audio.enabled,
            #[cfg(feature = "mod-bt")]
            "bt" | "bluetooth" => self.bt.enabled,
            #[cfg(feature = "mod-dbus")]
            "mpris" => self.mpris.enabled,
            #[cfg(feature = "mod-dbus")]
            "backlight" => self.backlight.enabled,
            #[cfg(feature = "mod-dbus")]
            "kbd" | "keyboard" => self.keyboard.enabled,
            #[cfg(feature = "mod-dbus")]
            "dnd" => self.dnd.enabled,
            _ => true,
        }
    }

    pub fn validate(&self) {
        #[cfg(feature = "mod-network")]
        validate_format(
            "network",
            &self.network.format,
            &["interface", "ip", "rx", "tx"],
        );
        #[cfg(feature = "mod-hardware")]
        {
            validate_format("cpu", &self.cpu.format, &["usage", "temp"]);
            validate_format("memory", &self.memory.format, &["used", "total"]);
            validate_format(
                "gpu.amd",
                &self.gpu.format_amd,
                &["usage", "vram_used", "vram_total", "temp"],
            );
            validate_format("gpu.intel", &self.gpu.format_intel, &["usage", "freq"]);
            validate_format(
                "gpu.nvidia",
                &self.gpu.format_nvidia,
                &["usage", "vram_used", "vram_total", "temp"],
            );
            validate_format(
                "sys",
                &self.sys.format,
                &["uptime", "load1", "load5", "load15", "procs"],
            );
            validate_format("disk", &self.disk.format, &["mount", "used", "total"]);
            validate_format("pool", &self.pool.format, &["used", "total"]);
            validate_format("power", &self.power.format, &["percentage", "icon"]);
        }
        #[cfg(feature = "mod-audio")]
        {
            validate_format(
                "audio.sink_unmuted",
                &self.audio.format_sink_unmuted,
                &["name", "icon", "volume"],
            );
            validate_format(
                "audio.sink_muted",
                &self.audio.format_sink_muted,
                &["name", "icon"],
            );
            validate_format(
                "audio.source_unmuted",
                &self.audio.format_source_unmuted,
                &["name", "icon", "volume"],
            );
            validate_format(
                "audio.source_muted",
                &self.audio.format_source_muted,
                &["name", "icon"],
            );
        }
        #[cfg(feature = "mod-bt")]
        {
            validate_format("bt.connected", &self.bt.format_connected, &["alias"]);
            validate_format(
                "bt.plugin",
                &self.bt.format_plugin,
                &["alias", "left", "right", "anc", "mac"],
            );
        }
        #[cfg(feature = "mod-dbus")]
        {
            validate_format(
                "mpris",
                &self.mpris.format,
                &["artist", "title", "album", "status_icon"],
            );
            validate_format("backlight", &self.backlight.format, &["percentage", "icon"]);
            validate_format("keyboard", &self.keyboard.format, &["layout"]);
        }
    }
}

pub fn default_config_path() -> PathBuf {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
            PathBuf::from(home).join(".config")
        });
    config_dir.join("fluxo/config.toml")
}

pub fn load_config(custom_path: Option<PathBuf>) -> Config {
    let config_path = custom_path.unwrap_or_else(default_config_path);

    if let Ok(content) = fs::read_to_string(&config_path) {
        match toml::from_str::<Config>(&content) {
            Ok(cfg) => {
                info!("Successfully loaded configuration from {:?}", config_path);
                cfg.validate();
                cfg
            }
            Err(e) => {
                warn!("Failed to parse config at {:?}: {}", config_path, e);
                warn!("Falling back to default configuration.");
                Config::default()
            }
        }
    } else {
        debug!(
            "No config file found at {:?}, using default settings.",
            config_path
        );
        Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(
            config.general.menu_command,
            "fuzzel --dmenu --prompt \"$FLUXO_PROMPT\""
        );
        assert!(config.cpu.format.contains("usage"));
        assert!(config.cpu.format.contains("temp"));
        assert!(config.memory.format.contains("used"));
        assert!(config.memory.format.contains("total"));
    }

    #[test]
    fn test_load_missing_config() {
        let config = load_config(Some(PathBuf::from("/nonexistent/config.toml")));
        // Should fallback to defaults without panicking
        assert_eq!(
            config.general.menu_command,
            "fuzzel --dmenu --prompt \"$FLUXO_PROMPT\""
        );
    }

    #[test]
    fn test_load_valid_partial_config() {
        let mut tmpfile = tempfile::NamedTempFile::new().unwrap();
        // In TOML, braces have no special meaning in strings
        writeln!(tmpfile, "[cpu]").unwrap();
        writeln!(tmpfile, "format = \"custom: {{usage}}\"").unwrap();

        let config = load_config(Some(tmpfile.path().to_path_buf()));
        // TOML treats {{ as literal {{ (no escape), so the value is "custom: {{usage}}"
        assert!(config.cpu.format.contains("usage"));
        // Other sections still have defaults
        assert!(config.memory.format.contains("used"));
    }

    #[test]
    fn test_load_invalid_toml() {
        let mut tmpfile = tempfile::NamedTempFile::new().unwrap();
        write!(tmpfile, "this is not valid toml {{{{").unwrap();

        let config = load_config(Some(tmpfile.path().to_path_buf()));
        // Should fallback to defaults
        assert_eq!(
            config.general.menu_command,
            "fuzzel --dmenu --prompt \"$FLUXO_PROMPT\""
        );
    }

    #[test]
    fn test_load_empty_config() {
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        // Empty file is valid TOML, all sections default

        let config = load_config(Some(tmpfile.path().to_path_buf()));
        assert_eq!(
            config.general.menu_command,
            "fuzzel --dmenu --prompt \"$FLUXO_PROMPT\""
        );
        assert!(config.cpu.format.contains("usage"));
    }
}
