//! Waybar signalling: watch state channels, send `SIGRTMIN+N` on changes.
//!
//! Waybar's custom modules use `signal = N` to rerun their command when they
//! receive `SIGRTMIN+N`. This task subscribes to every watched module's
//! `watch::Receiver`, evaluates the module when its channel fires, and only
//! signals Waybar when the rendered output actually changed. A 50 ms per-signal
//! debounce prevents storms during rapid state churn.

use crate::config::Config;
use crate::state::AppReceivers;
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::{ProcessesToUpdate, System};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, warn};

/// Sends real-time signals to the Waybar process.
///
/// Resolves Waybar's PID lazily and caches it — the PID is invalidated on
/// signal failure (e.g. Waybar was restarted) and rediscovered via `sysinfo`.
pub struct WaybarSignaler {
    cached_pid: Option<i32>,
    sys: System,
    last_signal_sent: HashMap<i32, Instant>,
}

impl WaybarSignaler {
    /// Create a new signaler with no cached PID.
    pub fn new() -> Self {
        Self {
            cached_pid: None,
            sys: System::new(),
            last_signal_sent: HashMap::new(),
        }
    }

    fn find_waybar_pid(&mut self) -> Option<i32> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        for (pid, process) in self.sys.processes() {
            if process.name() == "waybar" {
                return Some(pid.as_u32() as i32);
            }
        }
        None
    }

    fn send_signal(&mut self, signal_num: i32) {
        if let Some(last) = self.last_signal_sent.get(&signal_num)
            && last.elapsed() < Duration::from_millis(50)
        {
            return;
        }

        let mut valid_pid = false;
        if let Some(pid) = self.cached_pid
            && unsafe { libc::kill(pid, 0) } == 0
        {
            valid_pid = true;
        }

        if !valid_pid {
            self.cached_pid = self.find_waybar_pid();
        }

        if let Some(pid) = self.cached_pid {
            let sig = libc::SIGRTMIN() + signal_num;
            if unsafe { libc::kill(pid, sig) } == 0 {
                debug!("Sent SIGRTMIN+{} to waybar (PID: {})", signal_num, pid);
                self.last_signal_sent.insert(signal_num, Instant::now());
            } else {
                warn!("Failed to send SIGRTMIN+{} to waybar", signal_num);
                self.cached_pid = None;
            }
        } else {
            debug!("Waybar process not found, skipping signal.");
        }
    }
}

/// Generates [`WaybarSignaler::run`] from the central module registry.
///
/// For each watched module we emit:
/// * a cfg-gated `watch::Receiver::changed()` future (or `pending::<()>` when
///   the feature is disabled, so the `tokio::select!` arm compiles uniformly),
/// * a `select!` arm that re-evaluates the module and signals Waybar once per
///   changed output.
macro_rules! gen_signaler_run {
    ($( { $feature:literal, $field:ident, $state:ty, [$($name:literal),+], [$($sig_name:literal),+], $module:path, $signal:ident, [$($default_arg:literal),*], $config:ident } )*) => {
        impl WaybarSignaler {
            /// Run the signaler event loop forever.
            ///
            /// Consumes `self`; intended to be spawned as a long-lived task.
            pub async fn run(mut self, config_lock: Arc<RwLock<Config>>, mut receivers: AppReceivers) {
                let mut last_outputs: HashMap<&'static str, String> = HashMap::new();

                loop {
                    let signals = config_lock.read().await.signals.clone();

                    macro_rules! check_and_signal {
                        ($module_name:expr, $signal_opt:expr) => {
                            if let Some(sig) = $signal_opt {
                                let config = config_lock.read().await;
                                if let Some(out) = crate::daemon::evaluate_module_for_signaler(
                                    $module_name,
                                    &receivers,
                                    &config,
                                )
                                .await
                                {
                                    if last_outputs.get($module_name) != Some(&out) {
                                        last_outputs.insert($module_name, out);
                                        self.send_signal(sig);
                                    }
                                }
                            }
                        };
                    }

                    // Generate cfg-gated futures for each watched module
                    $(
                        #[cfg(not(feature = $feature))]
                        let $field = std::future::pending::<
                            std::result::Result<(), tokio::sync::watch::error::RecvError>,
                        >();
                        #[cfg(feature = $feature)]
                        let $field = receivers.$field.changed();
                    )*

                    // MPRIS scroll tick (special case — not a watched module)
                    #[cfg(not(feature = "mod-dbus"))]
                    let mpris_scroll_tick = std::future::pending::<
                        std::result::Result<(), tokio::sync::watch::error::RecvError>,
                    >();
                    #[cfg(feature = "mod-dbus")]
                    let mpris_scroll_tick = receivers.mpris_scroll_tick.changed();

                    tokio::select! {
                        $(
                            res = $field, if signals.$signal.is_some() => {
                                if res.is_ok() {
                                    $(check_and_signal!($sig_name, signals.$signal);)+
                                }
                            }
                        )*

                        // MPRIS scroll tick (separate from mpris data changes)
                        res = mpris_scroll_tick, if signals.mpris.is_some() => {
                            if res.is_ok()
                                && let Some(sig) = signals.mpris { self.send_signal(sig); }
                        }

                        _ = sleep(Duration::from_secs(5)) => {
                            // heartbeat: re-read signals config on each iteration
                        }
                    }
                }
            }
        }
    };
}

for_each_watched_module!(gen_signaler_run);
