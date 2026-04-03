use crate::output::WaybarOutput;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::Instant;

#[derive(Clone)]
pub struct AppReceivers {
    pub network: watch::Receiver<NetworkState>,
    pub cpu: watch::Receiver<CpuState>,
    pub memory: watch::Receiver<MemoryState>,
    pub sys: watch::Receiver<SysState>,
    pub gpu: watch::Receiver<GpuState>,
    pub disks: watch::Receiver<Vec<DiskInfo>>,
    pub bluetooth: watch::Receiver<BtState>,
    pub audio: watch::Receiver<AudioState>,
    pub mpris: watch::Receiver<MprisState>,
    pub backlight: watch::Receiver<BacklightState>,
    pub keyboard: watch::Receiver<KeyboardState>,
    pub dnd: watch::Receiver<DndState>,
    pub health: Arc<RwLock<HashMap<String, ModuleHealth>>>,
    pub bt_force_poll: mpsc::Sender<()>,
    pub audio_cmd_tx: mpsc::Sender<crate::modules::audio::AudioCommand>,
}

#[derive(Clone, Default)]
pub struct ModuleHealth {
    pub consecutive_failures: u32,
    pub last_failure: Option<Instant>,
    pub backoff_until: Option<Instant>,
    pub last_successful_output: Option<WaybarOutput>,
}

#[derive(Default, Clone)]
pub struct AudioState {
    pub sink: AudioDeviceInfo,
    pub source: AudioSourceInfo,
    pub available_sinks: Vec<String>,
    pub available_sources: Vec<String>,
}

#[derive(Default, Clone)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub description: String,
    pub volume: u8,
    pub muted: bool,
    pub channels: u8,
}

#[derive(Default, Clone)]
pub struct AudioSourceInfo {
    pub name: String,
    pub description: String,
    pub volume: u8,
    pub muted: bool,
    pub channels: u8,
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

#[derive(Default, Clone)]
pub struct DndState {
    pub is_dnd: bool,
}

#[derive(Default, Clone)]
pub struct KeyboardState {
    pub layout: String,
}

#[derive(Default, Clone)]
pub struct BacklightState {
    pub percentage: u8,
}

#[derive(Default, Clone)]
pub struct MprisState {
    pub is_playing: bool,
    pub is_paused: bool,
    pub is_stopped: bool,
    pub artist: String,
    pub title: String,
    pub album: String,
}

#[cfg(test)]
pub struct MockState {
    pub receivers: AppReceivers,
    // Keep senders alive so receivers don't return Closed errors
    _net_tx: watch::Sender<NetworkState>,
    _cpu_tx: watch::Sender<CpuState>,
    _mem_tx: watch::Sender<MemoryState>,
    _sys_tx: watch::Sender<SysState>,
    _gpu_tx: watch::Sender<GpuState>,
    _disks_tx: watch::Sender<Vec<DiskInfo>>,
    _bt_tx: watch::Sender<BtState>,
    _audio_tx: watch::Sender<AudioState>,
    _mpris_tx: watch::Sender<MprisState>,
    _backlight_tx: watch::Sender<BacklightState>,
    _keyboard_tx: watch::Sender<KeyboardState>,
    _dnd_tx: watch::Sender<DndState>,
}

#[cfg(test)]
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
    pub mpris: MprisState,
    pub backlight: BacklightState,
    pub keyboard: KeyboardState,
    pub dnd: DndState,
    pub health: HashMap<String, ModuleHealth>,
}

#[cfg(test)]
pub fn mock_state(state: AppState) -> MockState {
    let (net_tx, net_rx) = watch::channel(state.network);
    let (cpu_tx, cpu_rx) = watch::channel(state.cpu);
    let (mem_tx, mem_rx) = watch::channel(state.memory);
    let (sys_tx, sys_rx) = watch::channel(state.sys);
    let (gpu_tx, gpu_rx) = watch::channel(state.gpu);
    let (disks_tx, disks_rx) = watch::channel(state.disks);
    let (bt_tx, bt_rx) = watch::channel(state.bluetooth);
    let (audio_tx, audio_rx) = watch::channel(state.audio);
    let (mpris_tx, mpris_rx) = watch::channel(state.mpris);
    let (backlight_tx, backlight_rx) = watch::channel(state.backlight);
    let (keyboard_tx, keyboard_rx) = watch::channel(state.keyboard);
    let (dnd_tx, dnd_rx) = watch::channel(state.dnd);
    let (bt_force_tx, _) = mpsc::channel(1);
    let (audio_cmd_tx, _) = mpsc::channel(1);

    MockState {
        receivers: AppReceivers {
            network: net_rx,
            cpu: cpu_rx,
            memory: mem_rx,
            sys: sys_rx,
            gpu: gpu_rx,
            disks: disks_rx,
            bluetooth: bt_rx,
            audio: audio_rx,
            mpris: mpris_rx,
            backlight: backlight_rx,
            keyboard: keyboard_rx,
            dnd: dnd_rx,
            health: Arc::new(RwLock::new(state.health)),
            bt_force_poll: bt_force_tx,
            audio_cmd_tx,
        },
        _net_tx: net_tx,
        _cpu_tx: cpu_tx,
        _mem_tx: mem_tx,
        _sys_tx: sys_tx,
        _gpu_tx: gpu_tx,
        _disks_tx: disks_tx,
        _bt_tx: bt_tx,
        _audio_tx: audio_tx,
        _mpris_tx: mpris_tx,
        _backlight_tx: backlight_tx,
        _keyboard_tx: keyboard_tx,
        _dnd_tx: dnd_tx,
    }
}
