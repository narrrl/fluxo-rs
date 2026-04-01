pub mod audio;
pub mod bt;
pub mod btrfs;
pub mod cpu;
pub mod disk;
pub mod game;
pub mod gpu;
pub mod hardware;
pub mod memory;
pub mod network;
pub mod power;
pub mod sys;

use crate::config::Config;
use crate::error::Result as FluxoResult;
use crate::output::WaybarOutput;
use crate::state::SharedState;

pub trait WaybarModule {
    fn run(
        &self,
        config: &Config,
        state: &SharedState,
        args: &[&str],
    ) -> impl std::future::Future<Output = FluxoResult<WaybarOutput>> + Send;
}
