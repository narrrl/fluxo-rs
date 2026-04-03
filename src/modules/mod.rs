pub mod audio;
pub mod backlight;
pub mod bt;
pub mod btrfs;
pub mod cpu;
pub mod disk;
pub mod dnd;
pub mod game;
pub mod gpu;
pub mod hardware;
pub mod keyboard;
pub mod memory;
pub mod mpris;
pub mod network;
pub mod power;
pub mod sys;

use crate::config::Config;
use crate::error::Result as FluxoResult;
use crate::output::WaybarOutput;
use crate::state::AppReceivers;

pub trait WaybarModule {
    fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> impl std::future::Future<Output = FluxoResult<WaybarOutput>> + Send;
}
