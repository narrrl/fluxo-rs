use crate::config::Config;
use crate::error::{FluxoError, Result as FluxoResult};
use crate::output::WaybarOutput;
use crate::state::AppReceivers;

#[allow(unused_imports)]
use crate::modules::WaybarModule;

pub async fn dispatch(
    module_name: &str,
    #[allow(unused)] config: &Config,
    #[allow(unused)] state: &AppReceivers,
    #[allow(unused)] args: &[&str],
) -> FluxoResult<WaybarOutput> {
    if !config.is_module_enabled(module_name) {
        return Err(FluxoError::Disabled(module_name.to_string()));
    }

    match module_name {
        #[cfg(feature = "mod-network")]
        "net" | "network" => {
            crate::modules::network::NetworkModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "cpu" => {
            crate::modules::cpu::CpuModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "mem" | "memory" => {
            crate::modules::memory::MemoryModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "disk" => {
            crate::modules::disk::DiskModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "pool" | "btrfs" => {
            crate::modules::btrfs::BtrfsModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-audio")]
        "vol" | "audio" => {
            crate::modules::audio::AudioModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-audio")]
        "mic" => {
            crate::modules::audio::AudioModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "gpu" => {
            crate::modules::gpu::GpuModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "sys" => {
            crate::modules::sys::SysModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-bt")]
        "bt" | "bluetooth" => crate::modules::bt::BtModule.run(config, state, args).await,
        #[cfg(feature = "mod-hardware")]
        "power" => {
            crate::modules::power::PowerModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-hardware")]
        "game" => {
            crate::modules::game::GameModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-dbus")]
        "backlight" => {
            crate::modules::backlight::BacklightModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-dbus")]
        "kbd" | "keyboard" => {
            crate::modules::keyboard::KeyboardModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-dbus")]
        "dnd" => {
            crate::modules::dnd::DndModule
                .run(config, state, args)
                .await
        }
        #[cfg(feature = "mod-dbus")]
        "mpris" => {
            crate::modules::mpris::MprisModule
                .run(config, state, args)
                .await
        }
        _ => Err(FluxoError::Ipc(format!("Unknown module: {}", module_name))),
    }
}

/// Returns the default args used by the signaler when evaluating a module.
pub fn signaler_default_args(module_name: &str) -> &'static [&'static str] {
    match module_name {
        "disk" => &["/"],
        "vol" | "audio" => &["sink", "show"],
        "mic" => &["source", "show"],
        "bt" | "bluetooth" => &["show"],
        _ => &[],
    }
}
