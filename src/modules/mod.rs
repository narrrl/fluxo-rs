pub mod network;
pub mod cpu;
pub mod memory;
pub mod hardware;
pub mod disk;
pub mod btrfs;
pub mod audio;
pub mod gpu;
pub mod sys;
pub mod bt;
pub mod buds;
pub mod power;
pub mod game;

use crate::config::Config;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;

pub trait WaybarModule {
    fn run(&self, config: &Config, state: &SharedState, args: &[&str]) -> Result<WaybarOutput>;
}

