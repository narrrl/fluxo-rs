use crate::config::Config;
use crate::state::AppReceivers;
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::{ProcessesToUpdate, System};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, warn};

pub struct WaybarSignaler {
    cached_pid: Option<i32>,
    sys: System,
    last_signal_sent: HashMap<i32, Instant>,
}

impl WaybarSignaler {
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

            // For disabled features, create futures that never resolve
            #[cfg(not(feature = "mod-network"))]
            let net_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-network")]
            let net_changed = receivers.network.changed();

            #[cfg(not(feature = "mod-hardware"))]
            let cpu_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-hardware")]
            let cpu_changed = receivers.cpu.changed();

            #[cfg(not(feature = "mod-hardware"))]
            let mem_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-hardware")]
            let mem_changed = receivers.memory.changed();

            #[cfg(not(feature = "mod-hardware"))]
            let sys_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-hardware")]
            let sys_changed = receivers.sys.changed();

            #[cfg(not(feature = "mod-hardware"))]
            let gpu_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-hardware")]
            let gpu_changed = receivers.gpu.changed();

            #[cfg(not(feature = "mod-hardware"))]
            let disks_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-hardware")]
            let disks_changed = receivers.disks.changed();

            #[cfg(not(feature = "mod-bt"))]
            let bt_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-bt")]
            let bt_changed = receivers.bluetooth.changed();

            #[cfg(not(feature = "mod-audio"))]
            let audio_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-audio")]
            let audio_changed = receivers.audio.changed();

            #[cfg(not(feature = "mod-dbus"))]
            let backlight_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-dbus")]
            let backlight_changed = receivers.backlight.changed();

            #[cfg(not(feature = "mod-dbus"))]
            let keyboard_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-dbus")]
            let keyboard_changed = receivers.keyboard.changed();

            #[cfg(not(feature = "mod-dbus"))]
            let dnd_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-dbus")]
            let dnd_changed = receivers.dnd.changed();

            #[cfg(not(feature = "mod-dbus"))]
            let mpris_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-dbus")]
            let mpris_changed = receivers.mpris.changed();

            #[cfg(not(feature = "mod-dbus"))]
            let mpris_scroll_tick_changed = std::future::pending::<
                std::result::Result<(), tokio::sync::watch::error::RecvError>,
            >();
            #[cfg(feature = "mod-dbus")]
            let mpris_scroll_tick_changed = receivers.mpris_scroll_tick.changed();

            tokio::select! {
                res = net_changed, if signals.network.is_some() => {
                    if res.is_ok() { check_and_signal!("net", signals.network); }
                }
                res = cpu_changed, if signals.cpu.is_some() => {
                    if res.is_ok() { check_and_signal!("cpu", signals.cpu); }
                }
                res = mem_changed, if signals.memory.is_some() => {
                    if res.is_ok() { check_and_signal!("mem", signals.memory); }
                }
                res = sys_changed, if signals.sys.is_some() => {
                    if res.is_ok() { check_and_signal!("sys", signals.sys); }
                }
                res = gpu_changed, if signals.gpu.is_some() => {
                    if res.is_ok() { check_and_signal!("gpu", signals.gpu); }
                }
                res = disks_changed, if signals.disk.is_some() => {
                    if res.is_ok() { check_and_signal!("disk", signals.disk); }
                }
                res = bt_changed, if signals.bt.is_some() => {
                    if res.is_ok() { check_and_signal!("bt", signals.bt); }
                }
                res = audio_changed, if signals.audio.is_some() => {
                    if res.is_ok() {
                        check_and_signal!("vol", signals.audio);
                        check_and_signal!("mic", signals.audio);
                    }
                }
                res = backlight_changed, if signals.backlight.is_some() => {
                    if res.is_ok() { check_and_signal!("backlight", signals.backlight); }
                }
                res = keyboard_changed, if signals.keyboard.is_some() => {
                    if res.is_ok() { check_and_signal!("keyboard", signals.keyboard); }
                }
                res = dnd_changed, if signals.dnd.is_some() => {
                    if res.is_ok() { check_and_signal!("dnd", signals.dnd); }
                }
                res = mpris_changed, if signals.mpris.is_some() => {
                    if res.is_ok() { check_and_signal!("mpris", signals.mpris); }
                }
                res = mpris_scroll_tick_changed, if signals.mpris.is_some() => {
                    if res.is_ok()
                        && let Some(sig) = signals.mpris { self.send_signal(sig); }
                }
                _ = sleep(Duration::from_secs(5)) => {
                    // loop and refresh config
                }
            }
        }
    }
}
