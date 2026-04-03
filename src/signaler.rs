use crate::config::Config;
use crate::state::AppReceivers;
use nix::libc;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::{ProcessesToUpdate, System};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, warn};

pub struct WaybarSignaler {
    cached_pid: Option<Pid>,
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

    fn find_waybar_pid(&mut self) -> Option<Pid> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        for (pid, process) in self.sys.processes() {
            if process.name() == "waybar" {
                return Some(Pid::from_raw(pid.as_u32() as i32));
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
            && kill(pid, None).is_ok()
        {
            valid_pid = true;
        }

        if !valid_pid {
            self.cached_pid = self.find_waybar_pid();
        }

        if let Some(pid) = self.cached_pid {
            let rt_sig = match Signal::try_from(libc::SIGRTMIN() + signal_num) {
                Ok(sig) => sig,
                Err(_) => {
                    unsafe {
                        if libc::kill(pid.as_raw(), libc::SIGRTMIN() + signal_num) == 0 {
                            debug!("Sent raw SIGRTMIN+{} to waybar (PID: {})", signal_num, pid);
                            self.last_signal_sent.insert(signal_num, Instant::now());
                        } else {
                            warn!("Failed to send raw SIGRTMIN+{} to waybar", signal_num);
                        }
                    }
                    return;
                }
            };

            if let Err(e) = kill(pid, rt_sig) {
                warn!("Failed to signal waybar: {}", e);
                self.cached_pid = None;
            } else {
                debug!("Sent SIGRTMIN+{} to waybar (PID: {})", signal_num, pid);
                self.last_signal_sent.insert(signal_num, Instant::now());
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

            tokio::select! {
                res = receivers.network.changed(), if signals.network.is_some() => {
                    if res.is_ok() { check_and_signal!("net", signals.network); }
                }
                res = receivers.cpu.changed(), if signals.cpu.is_some() => {
                    if res.is_ok() { check_and_signal!("cpu", signals.cpu); }
                }
                res = receivers.memory.changed(), if signals.memory.is_some() => {
                    if res.is_ok() { check_and_signal!("mem", signals.memory); }
                }
                res = receivers.sys.changed(), if signals.sys.is_some() => {
                    if res.is_ok() { check_and_signal!("sys", signals.sys); }
                }
                res = receivers.gpu.changed(), if signals.gpu.is_some() => {
                    if res.is_ok() { check_and_signal!("gpu", signals.gpu); }
                }
                res = receivers.disks.changed(), if signals.disk.is_some() => {
                    if res.is_ok() { check_and_signal!("disk", signals.disk); }
                }
                res = receivers.bluetooth.changed(), if signals.bt.is_some() => {
                    if res.is_ok() { check_and_signal!("bt", signals.bt); }
                }
                res = receivers.audio.changed(), if signals.audio.is_some() => {
                    if res.is_ok() {
                        check_and_signal!("vol", signals.audio);
                        check_and_signal!("mic", signals.audio);
                    }
                }
                res = receivers.backlight.changed(), if signals.backlight.is_some() => {
                    if res.is_ok() { check_and_signal!("backlight", signals.backlight); }
                }
                res = receivers.keyboard.changed(), if signals.keyboard.is_some() => {
                    if res.is_ok() { check_and_signal!("keyboard", signals.keyboard); }
                }
                res = receivers.dnd.changed(), if signals.dnd.is_some() => {
                    if res.is_ok() { check_and_signal!("dnd", signals.dnd); }
                }
                res = receivers.mpris.changed(), if signals.mpris.is_some() => {
                    if res.is_ok() { check_and_signal!("mpris", signals.mpris); }
                }
                _ = sleep(Duration::from_secs(5)) => {
                    // loop and refresh config
                }
            }
        }
    }
}
