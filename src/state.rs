use crate::output::WaybarOutput;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::Instant;

macro_rules! gen_app_receivers {
    ($( { $feature:literal, $field:ident, $state:ty, [$($name:literal),+], [$($sig_name:literal),+], $module:path, $signal:ident, [$($default_arg:literal),*], $config:ident } )*) => {
        #[derive(Clone)]
        pub struct AppReceivers {
            $(
                #[cfg(feature = $feature)]
                pub $field: watch::Receiver<$state>,
            )*
            #[cfg(feature = "mod-bt")]
            pub bt_cycle: Arc<RwLock<usize>>,
            #[cfg(feature = "mod-dbus")]
            pub mpris_scroll: Arc<RwLock<MprisScrollState>>,
            #[cfg(feature = "mod-dbus")]
            pub mpris_scroll_tick: watch::Receiver<u64>,
            pub health: Arc<RwLock<HashMap<String, ModuleHealth>>>,
            #[cfg(feature = "mod-bt")]
            pub bt_force_poll: mpsc::Sender<()>,
            #[cfg(feature = "mod-audio")]
            pub audio_cmd_tx: mpsc::Sender<crate::modules::audio::AudioCommand>,
        }
    };
}
for_each_watched_module!(gen_app_receivers);

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
    pub source: AudioDeviceInfo,
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
pub struct BtDeviceInfo {
    pub device_alias: String,
    pub device_address: String,
    pub battery_percentage: Option<u8>,
    pub plugin_data: Vec<(String, String)>,
}

#[derive(Default, Clone)]
pub struct BtState {
    pub adapter_powered: bool,
    pub devices: Vec<BtDeviceInfo>,
}

impl BtState {
    pub fn active_device(&self, index: usize) -> Option<&BtDeviceInfo> {
        if self.devices.is_empty() {
            None
        } else {
            Some(&self.devices[index % self.devices.len()])
        }
    }
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
pub struct MprisScrollState {
    pub offset: usize,
    pub full_text: String,
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
    #[cfg(feature = "mod-network")]
    _net_tx: watch::Sender<NetworkState>,
    #[cfg(feature = "mod-hardware")]
    _cpu_tx: watch::Sender<CpuState>,
    #[cfg(feature = "mod-hardware")]
    _mem_tx: watch::Sender<MemoryState>,
    #[cfg(feature = "mod-hardware")]
    _sys_tx: watch::Sender<SysState>,
    #[cfg(feature = "mod-hardware")]
    _gpu_tx: watch::Sender<GpuState>,
    #[cfg(feature = "mod-hardware")]
    _disks_tx: watch::Sender<Vec<DiskInfo>>,
    #[cfg(feature = "mod-bt")]
    _bt_tx: watch::Sender<BtState>,
    #[cfg(feature = "mod-audio")]
    _audio_tx: watch::Sender<AudioState>,
    #[cfg(feature = "mod-dbus")]
    _mpris_tx: watch::Sender<MprisState>,
    #[cfg(feature = "mod-dbus")]
    _backlight_tx: watch::Sender<BacklightState>,
    #[cfg(feature = "mod-dbus")]
    _keyboard_tx: watch::Sender<KeyboardState>,
    #[cfg(feature = "mod-dbus")]
    _dnd_tx: watch::Sender<DndState>,
}

#[cfg(test)]
#[derive(Default, Clone)]
pub struct AppState {
    #[cfg(feature = "mod-network")]
    pub network: NetworkState,
    #[cfg(feature = "mod-hardware")]
    pub cpu: CpuState,
    #[cfg(feature = "mod-hardware")]
    pub memory: MemoryState,
    #[cfg(feature = "mod-hardware")]
    pub sys: SysState,
    #[cfg(feature = "mod-hardware")]
    pub gpu: GpuState,
    #[cfg(feature = "mod-hardware")]
    pub disks: Vec<DiskInfo>,
    #[cfg(feature = "mod-bt")]
    pub bluetooth: BtState,
    #[cfg(feature = "mod-audio")]
    pub audio: AudioState,
    #[cfg(feature = "mod-dbus")]
    pub mpris: MprisState,
    #[cfg(feature = "mod-dbus")]
    pub backlight: BacklightState,
    #[cfg(feature = "mod-dbus")]
    pub keyboard: KeyboardState,
    #[cfg(feature = "mod-dbus")]
    pub dnd: DndState,
    pub health: HashMap<String, ModuleHealth>,
}

#[cfg(test)]
pub fn mock_state(state: AppState) -> MockState {
    #[cfg(feature = "mod-network")]
    let (net_tx, net_rx) = watch::channel(state.network);
    #[cfg(feature = "mod-hardware")]
    let (cpu_tx, cpu_rx) = watch::channel(state.cpu);
    #[cfg(feature = "mod-hardware")]
    let (mem_tx, mem_rx) = watch::channel(state.memory);
    #[cfg(feature = "mod-hardware")]
    let (sys_tx, sys_rx) = watch::channel(state.sys);
    #[cfg(feature = "mod-hardware")]
    let (gpu_tx, gpu_rx) = watch::channel(state.gpu);
    #[cfg(feature = "mod-hardware")]
    let (disks_tx, disks_rx) = watch::channel(state.disks);
    #[cfg(feature = "mod-bt")]
    let (bt_tx, bt_rx) = watch::channel(state.bluetooth);
    #[cfg(feature = "mod-audio")]
    let (audio_tx, audio_rx) = watch::channel(state.audio);
    #[cfg(feature = "mod-dbus")]
    let (mpris_tx, mpris_rx) = watch::channel(state.mpris);
    #[cfg(feature = "mod-dbus")]
    let (backlight_tx, backlight_rx) = watch::channel(state.backlight);
    #[cfg(feature = "mod-dbus")]
    let (keyboard_tx, keyboard_rx) = watch::channel(state.keyboard);
    #[cfg(feature = "mod-dbus")]
    let (dnd_tx, dnd_rx) = watch::channel(state.dnd);
    #[cfg(feature = "mod-bt")]
    let (bt_force_tx, _) = mpsc::channel(1);
    #[cfg(feature = "mod-audio")]
    let (audio_cmd_tx, _) = mpsc::channel(1);

    MockState {
        receivers: AppReceivers {
            #[cfg(feature = "mod-network")]
            network: net_rx,
            #[cfg(feature = "mod-hardware")]
            cpu: cpu_rx,
            #[cfg(feature = "mod-hardware")]
            memory: mem_rx,
            #[cfg(feature = "mod-hardware")]
            sys: sys_rx,
            #[cfg(feature = "mod-hardware")]
            gpu: gpu_rx,
            #[cfg(feature = "mod-hardware")]
            disks: disks_rx,
            #[cfg(feature = "mod-bt")]
            bluetooth: bt_rx,
            #[cfg(feature = "mod-bt")]
            bt_cycle: Arc::new(RwLock::new(0usize)),
            #[cfg(feature = "mod-audio")]
            audio: audio_rx,
            #[cfg(feature = "mod-dbus")]
            mpris: mpris_rx,
            #[cfg(feature = "mod-dbus")]
            backlight: backlight_rx,
            #[cfg(feature = "mod-dbus")]
            keyboard: keyboard_rx,
            #[cfg(feature = "mod-dbus")]
            dnd: dnd_rx,
            #[cfg(feature = "mod-dbus")]
            mpris_scroll: Arc::new(RwLock::new(MprisScrollState::default())),
            #[cfg(feature = "mod-dbus")]
            mpris_scroll_tick: {
                let (_, rx) = watch::channel(0u64);
                rx
            },
            health: Arc::new(RwLock::new(state.health)),
            #[cfg(feature = "mod-bt")]
            bt_force_poll: bt_force_tx,
            #[cfg(feature = "mod-audio")]
            audio_cmd_tx,
        },
        #[cfg(feature = "mod-network")]
        _net_tx: net_tx,
        #[cfg(feature = "mod-hardware")]
        _cpu_tx: cpu_tx,
        #[cfg(feature = "mod-hardware")]
        _mem_tx: mem_tx,
        #[cfg(feature = "mod-hardware")]
        _sys_tx: sys_tx,
        #[cfg(feature = "mod-hardware")]
        _gpu_tx: gpu_tx,
        #[cfg(feature = "mod-hardware")]
        _disks_tx: disks_tx,
        #[cfg(feature = "mod-bt")]
        _bt_tx: bt_tx,
        #[cfg(feature = "mod-audio")]
        _audio_tx: audio_tx,
        #[cfg(feature = "mod-dbus")]
        _mpris_tx: mpris_tx,
        #[cfg(feature = "mod-dbus")]
        _backlight_tx: backlight_tx,
        #[cfg(feature = "mod-dbus")]
        _keyboard_tx: keyboard_tx,
        #[cfg(feature = "mod-dbus")]
        _dnd_tx: dnd_tx,
    }
}
