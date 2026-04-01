use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Instant;

#[derive(Default, Clone)]
pub struct AppState {
    pub network: NetworkState,
    pub cpu: CpuState,
    pub memory: MemoryState,
    pub sys: SysState,
    pub gpu: GpuState,
    pub disks: Vec<DiskInfo>,
    pub bluetooth: BtState,
    pub audio: AudioState,
    pub health: HashMap<String, ModuleHealth>,
}

#[derive(Clone, Default)]
pub struct ModuleHealth {
    pub consecutive_failures: u32,
    pub last_failure: Option<Instant>,
    pub backoff_until: Option<Instant>,
}

#[derive(Default, Clone)]
pub struct AudioState {
    pub sink: AudioDeviceInfo,
    pub source: AudioSourceInfo,
}

#[derive(Default, Clone)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub description: String,
    pub volume: u8,
    pub muted: bool,
}

#[derive(Default, Clone)]
pub struct AudioSourceInfo {
    pub name: String,
    pub description: String,
    pub volume: u8,
    pub muted: bool,
}

#[derive(Default, Clone)]
pub struct BtState {
    pub connected: bool,
    pub adapter_powered: bool,
    pub device_alias: String,
    pub device_address: String,
    pub battery_percentage: Option<u8>,
    pub plugin_data: Vec<(String, String)>,
}

#[derive(Default, Clone)]
pub struct DiskInfo {
    pub mount_point: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Default, Clone)]
pub struct NetworkState {
    pub rx_mbps: f64,
    pub tx_mbps: f64,
    pub interface: String,
    pub ip: String,
}

#[derive(Clone)]
pub struct CpuState {
    pub usage: f64,
    pub temp: f64,
    pub model: String,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            usage: 0.0,
            temp: 0.0,
            model: String::from("Unknown"),
        }
    }
}

#[derive(Default, Clone)]
pub struct MemoryState {
    pub used_gb: f64,
    pub total_gb: f64,
}

#[derive(Default, Clone)]
pub struct SysState {
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub uptime: u64,
    pub process_count: usize,
}

#[derive(Clone)]
pub struct GpuState {
    pub active: bool,
    pub vendor: String,
    pub usage: f64,
    pub vram_used: f64,
    pub vram_total: f64,
    pub temp: f64,
    pub model: String,
}

impl Default for GpuState {
    fn default() -> Self {
        Self {
            active: false,
            vendor: String::from("Unknown"),
            usage: 0.0,
            vram_used: 0.0,
            vram_total: 0.0,
            temp: 0.0,
            model: String::from("Unknown"),
        }
    }
}

pub type SharedState = Arc<RwLock<AppState>>;

#[cfg(test)]
pub fn mock_state(state: AppState) -> SharedState {
    Arc::new(RwLock::new(state))
}
