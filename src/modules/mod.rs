#[cfg(feature = "mod-audio")]
pub mod audio;
#[cfg(feature = "mod-dbus")]
pub mod backlight;
#[cfg(feature = "mod-bt")]
pub mod bt;
#[cfg(feature = "mod-hardware")]
pub mod btrfs;
#[cfg(feature = "mod-hardware")]
pub mod cpu;
#[cfg(feature = "mod-hardware")]
pub mod disk;
#[cfg(feature = "mod-dbus")]
pub mod dnd;
#[cfg(feature = "mod-hardware")]
pub mod game;
#[cfg(feature = "mod-hardware")]
pub mod gpu;
#[cfg(feature = "mod-hardware")]
pub mod hardware;
#[cfg(feature = "mod-dbus")]
pub mod keyboard;
#[cfg(feature = "mod-hardware")]
pub mod memory;
#[cfg(feature = "mod-dbus")]
pub mod mpris;
#[cfg(feature = "mod-network")]
pub mod network;
#[cfg(feature = "mod-hardware")]
pub mod power;
#[cfg(feature = "mod-hardware")]
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
