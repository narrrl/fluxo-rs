use std::sync::{Arc, RwLock};

#[derive(Default, Clone)]
pub struct AppState {
    pub network: NetworkState,
    pub cpu: CpuState,
    pub memory: MemoryState,
}

#[derive(Default, Clone)]
pub struct NetworkState {
    pub rx_mbps: f64,
    pub tx_mbps: f64,
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

pub type SharedState = Arc<RwLock<AppState>>;
