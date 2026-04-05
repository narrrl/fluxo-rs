//! Waybar module implementations.
//!
//! Each submodule exposes a [`WaybarModule`] type (CPU, network, audio, …)
//! and is feature-gated by a `mod-<kind>` flag. The [`WaybarModule`] trait is
//! what [`crate::registry::dispatch`] uses to evaluate a module on demand.
//!
//! Modules come in two flavours:
//!
//! * **Watched** — the daemon spawns a background polling/event task that
//!   pushes state into a `watch::Receiver`, which the module reads lock-free
//!   (network, cpu, audio, bluetooth, …).
//! * **Dispatch-only** — evaluated on demand only, without a watch channel
//!   (power, game, btrfs).

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

/// Common interface implemented by every Waybar module.
///
/// Given the current daemon config, the shared state receivers, and any
/// caller-supplied arguments, a module produces a single [`WaybarOutput`].
/// Implementations must be cheap to evaluate — they are invoked on every
/// client request and on every signaler state change.
pub trait WaybarModule {
    /// Evaluate the module and return its rendered output.
    fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        args: &[&str],
    ) -> impl std::future::Future<Output = FluxoResult<WaybarOutput>> + Send;
}
