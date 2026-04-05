//! Central declarative macro that registers every watched module.
//!
//! Every piece of per-module boilerplate (AppReceivers field, IPC dispatch arm,
//! signaler future binding, signaler select arm, config enabled-lookup, default
//! signaler args) is generated from this single table. Adding a new module is
//! a one-line edit here plus writing the module file itself.

/// Central module registry. Defines all modules with watch channels in one place.
///
/// Invoke with a callback macro name. The callback receives repeated entries of the form:
///   { $feature:literal, $field:ident, $state:ty, [$($name:literal),+], [$($sig_name:literal),+], $module:path, $signal:ident, [$($default_arg:literal),*], $config:ident }
///
/// Fields:
///   - feature:       Cargo feature gate (e.g., "mod-network")
///   - field:         AppReceivers field name (e.g., network)
///   - state:         State type for the watch channel (e.g., NetworkState)
///   - names:         CLI name aliases for dispatch (e.g., ["net", "network"])
///   - signaler_names: Waybar module names to signal when the channel fires
///     (usually [first dispatch name], but audio signals ["vol", "mic"])
///   - module:        Module struct implementing WaybarModule (e.g., network::NetworkModule)
///   - signal:        SignalsConfig field name (e.g., network)
///   - default_args:  Default args for signaler evaluation
///   - config:        Config section field name (e.g., network)
///
/// Modules without watch channels (power, game, pool/btrfs) are handled manually.
macro_rules! for_each_watched_module {
    ($m:ident) => {
        $m! {
            { "mod-network",  network,   crate::state::NetworkState,       ["net", "network"],  ["net"],        crate::modules::network::NetworkModule,     network,   [],               network   }
            { "mod-hardware", cpu,       crate::state::CpuState,           ["cpu"],             ["cpu"],        crate::modules::cpu::CpuModule,             cpu,       [],               cpu       }
            { "mod-hardware", memory,    crate::state::MemoryState,        ["mem", "memory"],   ["mem"],        crate::modules::memory::MemoryModule,       memory,    [],               memory    }
            { "mod-hardware", sys,       crate::state::SysState,           ["sys"],             ["sys"],        crate::modules::sys::SysModule,             sys,       [],               sys       }
            { "mod-hardware", gpu,       crate::state::GpuState,           ["gpu"],             ["gpu"],        crate::modules::gpu::GpuModule,             gpu,       [],               gpu       }
            { "mod-hardware", disks,     Vec<crate::state::DiskInfo>,      ["disk"],            ["disk"],       crate::modules::disk::DiskModule,           disk,      ["/"],            disk      }
            { "mod-bt",       bluetooth, crate::state::BtState,            ["bt", "bluetooth"], ["bt"],         crate::modules::bt::BtModule,               bt,        ["show"],         bt        }
            { "mod-audio",    audio,     crate::state::AudioState,         ["vol", "audio"],    ["vol", "mic"], crate::modules::audio::AudioModule,         audio,     ["sink", "show"], audio     }
            { "mod-dbus",     mpris,     crate::state::MprisState,         ["mpris"],           ["mpris"],      crate::modules::mpris::MprisModule,         mpris,     [],               mpris     }
            { "mod-dbus",     backlight, crate::state::BacklightState,     ["backlight"],       ["backlight"],  crate::modules::backlight::BacklightModule, backlight, [],               backlight }
            { "mod-dbus",     keyboard,  crate::state::KeyboardState,      ["kbd", "keyboard"], ["kbd"],        crate::modules::keyboard::KeyboardModule,   keyboard,  [],               keyboard  }
            { "mod-dbus",     dnd,       crate::state::DndState,           ["dnd"],             ["dnd"],        crate::modules::dnd::DndModule,             dnd,       [],               dnd       }
        }
    };
}
