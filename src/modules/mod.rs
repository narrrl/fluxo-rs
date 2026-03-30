pub mod audio;
pub mod bt;
pub mod btrfs;
pub mod buds;
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
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub trait WaybarModule {
    fn run(&self, config: &Config, state: &SharedState, args: &[&str]) -> Result<WaybarOutput>;
}
