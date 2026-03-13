use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use anyhow::Result;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

pub struct NetworkModule;

pub struct NetworkDaemon {
    last_time: u64,
    last_rx_bytes: u64,
    last_tx_bytes: u64,
}

impl NetworkDaemon {
    pub fn new() -> Self {
        Self {
            last_time: 0,
            last_rx_bytes: 0,
            last_tx_bytes: 0,
        }
    }

    pub fn poll(&mut self, state: SharedState) {
        if let Ok(interface) = get_primary_interface() {
            if !interface.is_empty() {
                let time_now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if let Ok((rx_bytes_now, tx_bytes_now)) = get_bytes(&interface) {
                    if self.last_time > 0 && time_now > self.last_time {
                        let time_diff = time_now - self.last_time;
                        let rx_bps = (rx_bytes_now.saturating_sub(self.last_rx_bytes)) / time_diff;
                        let tx_bps = (tx_bytes_now.saturating_sub(self.last_tx_bytes)) / time_diff;

                        let rx_mbps = (rx_bps as f64) / 1024.0 / 1024.0;
                        let tx_mbps = (tx_bps as f64) / 1024.0 / 1024.0;

                        debug!(interface, rx = rx_mbps, tx = tx_mbps, "Network stats updated");

                        if let Ok(mut state_lock) = state.write() {
                            state_lock.network.rx_mbps = rx_mbps;
                            state_lock.network.tx_mbps = tx_mbps;
                        }
                    }

                    self.last_time = time_now;
                    self.last_rx_bytes = rx_bytes_now;
                    self.last_tx_bytes = tx_bytes_now;
                }
            } else {
                warn!("No primary network interface found during poll");
            }
        }
    }
}

impl WaybarModule for NetworkModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let interface = get_primary_interface()?;
        if interface.is_empty() {
            return Ok(WaybarOutput {
                text: "No connection".to_string(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        }

        let ip = get_ip_address(&interface).unwrap_or_else(|| String::from("No IP"));
        
        let (rx_mbps, tx_mbps) = {
            if let Ok(state_lock) = state.read() {
                (state_lock.network.rx_mbps, state_lock.network.tx_mbps)
            } else {
                (0.0, 0.0)
            }
        };

        let mut output_text = config
            .network
            .format
            .replace("{interface}", &interface)
            .replace("{ip}", &ip)
            .replace("{rx}", &format!("{:.2}", rx_mbps))
            .replace("{tx}", &format!("{:.2}", tx_mbps));

        if interface.starts_with("tun")
            || interface.starts_with("wg")
            || interface.starts_with("ppp")
            || interface.starts_with("pvpn")
        {
            output_text = format!("  {}", output_text);
        }

        Ok(WaybarOutput {
            text: output_text,
            tooltip: Some(format!("Interface: {}\nIP: {}", interface, ip)),
            class: Some(interface),
            percentage: None,
        })
    }
}

fn get_primary_interface() -> Result<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "list"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut defaults = Vec::new();
    for line in stdout.lines() {
        if line.starts_with("default") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut dev = "";
            let mut metric = 0;
            for i in 0..parts.len() {
                if parts[i] == "dev" && i + 1 < parts.len() {
                    dev = parts[i + 1];
                }
                if parts[i] == "metric" && i + 1 < parts.len() {
                    metric = parts[i + 1].parse::<i32>().unwrap_or(0);
                }
            }
            if !dev.is_empty() {
                defaults.push((metric, dev.to_string()));
            }
        }
    }

    defaults.sort_by_key(|k| k.0);
    if let Some((_, dev)) = defaults.first() {
        Ok(dev.clone())
    } else {
        Ok(String::new())
    }
}

fn get_ip_address(interface: &str) -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["-4", "addr", "show", interface])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.trim().starts_with("inet ") {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.len() > 1 {
                let ip_cidr = parts[1];
                let ip = ip_cidr.split('/').next().unwrap_or(ip_cidr);
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn get_bytes(interface: &str) -> Result<(u64, u64)> {
    let rx_path = format!("/sys/class/net/{}/statistics/rx_bytes", interface);
    let tx_path = format!("/sys/class/net/{}/statistics/tx_bytes", interface);

    let rx = fs::read_to_string(&rx_path)
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .parse()
        .unwrap_or(0);
    let tx = fs::read_to_string(&tx_path)
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .parse()
        .unwrap_or(0);

    Ok((rx, tx))
}
