use crate::config::Config;
use crate::error::{FluxoError, Result as FluxoResult};
use crate::output::WaybarOutput;
use crate::state::AppReceivers;

#[allow(unused_imports)]
use crate::modules::WaybarModule;

macro_rules! gen_dispatch {
    ($( { $feature:literal, $field:ident, $state:ty, [$($name:literal),+], [$($sig_name:literal),+], $module:path, $signal:ident, [$($default_arg:literal),*], $config:ident } )*) => {
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
                $(
                    #[cfg(feature = $feature)]
                    $($name)|+ => {
                        $module.run(config, state, args).await
                    }
                )*

                // Dispatch-only modules (no watch channel)
                #[cfg(feature = "mod-audio")]
                "mic" => {
                    crate::modules::audio::AudioModule
                        .run(config, state, args)
                        .await
                }
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
                #[cfg(feature = "mod-hardware")]
                "pool" | "btrfs" => {
                    crate::modules::btrfs::BtrfsModule
                        .run(config, state, args)
                        .await
                }
                _ => Err(FluxoError::Ipc(format!("Unknown module: {}", module_name))),
            }
        }

        /// Returns the default args used by the signaler when evaluating a module.
        pub fn signaler_default_args(module_name: &str) -> &'static [&'static str] {
            match module_name {
                $(
                    $($name)|+ => &[$($default_arg),*],
                )*
                "mic" => &["source", "show"],
                _ => &[],
            }
        }
    };
}

for_each_watched_module!(gen_dispatch);
