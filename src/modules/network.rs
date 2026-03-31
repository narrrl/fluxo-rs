use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template, run_command};
use anyhow::Result;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct NetworkModule;

pub struct NetworkDaemon {
    last_time: u64,
    last_rx_bytes: u64,
    last_tx_bytes: u64,
    cached_interface: Option<String>,
    cached_ip: Option<String>,
}

impl NetworkDaemon {
    pub fn new() -> Self {
        Self {
            last_time: 0,
            last_rx_bytes: 0,
            last_tx_bytes: 0,
            cached_interface: None,
            cached_ip: None,
        }
    }

    pub fn poll(&mut self, state: SharedState) {
        // Cache invalidation: if the interface directory doesn't exist, clear cache
        if let Some(ref iface) = self.cached_interface
            && !std::path::Path::new(&format!("/sys/class/net/{}", iface)).exists()
        {
            self.cached_interface = None;
            self.cached_ip = None;
        }

        // Re-detect interface if needed
        if self.cached_interface.is_none()
            && let Ok(iface) = get_primary_interface()
            && !iface.is_empty()
        {
            self.cached_ip = get_ip_address(&iface);
            self.cached_interface = Some(iface);
        }

        if let Some(ref interface) = self.cached_interface {
            let time_now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            if let Ok((rx_bytes_now, tx_bytes_now)) = get_bytes(interface) {
                if self.last_time > 0 && time_now > self.last_time {
                    let time_diff = (time_now - self.last_time) as f64;
                    let rx_mbps = (rx_bytes_now.saturating_sub(self.last_rx_bytes)) as f64
                        / time_diff
                        / 1024.0
                        / 1024.0;
                    let tx_mbps = (tx_bytes_now.saturating_sub(self.last_tx_bytes)) as f64
                        / time_diff
                        / 1024.0
                        / 1024.0;

                    if let Ok(mut state_lock) = state.write() {
                        state_lock.network.rx_mbps = rx_mbps;
                        state_lock.network.tx_mbps = tx_mbps;
                        state_lock.network.interface = interface.clone();
                        state_lock.network.ip = self.cached_ip.clone().unwrap_or_default();
                    }
                } else if let Ok(mut state_lock) = state.write() {
                    // First poll: no speed data yet, but update interface/ip
                    state_lock.network.interface = interface.clone();
                    state_lock.network.ip = self.cached_ip.clone().unwrap_or_default();
                }

                self.last_time = time_now;
                self.last_rx_bytes = rx_bytes_now;
                self.last_tx_bytes = tx_bytes_now;
            } else {
                // Read failed, might be down
                self.cached_interface = None;
            }
        } else if let Ok(mut state_lock) = state.write() {
            // No interface detected
            state_lock.network.interface.clear();
            state_lock.network.ip.clear();
        }
    }
}

impl WaybarModule for NetworkModule {
    fn run(&self, config: &Config, state: &SharedState, _args: &[&str]) -> Result<WaybarOutput> {
        let (interface, ip, rx_mbps, tx_mbps) = if let Ok(s) = state.read() {
            (
                s.network.interface.clone(),
                s.network.ip.clone(),
                s.network.rx_mbps,
                s.network.tx_mbps,
            )
        } else {
            return Ok(WaybarOutput {
                text: "No connection".to_string(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        };

        if interface.is_empty() {
            return Ok(WaybarOutput {
                text: "No connection".to_string(),
                tooltip: None,
                class: None,
                percentage: None,
            });
        }

        let ip_display = if ip.is_empty() { "No IP" } else { &ip };

        let mut output_text = format_template(
            &config.network.format,
            &[
                ("interface", TokenValue::String(&interface)),
                ("ip", TokenValue::String(ip_display)),
                ("rx", TokenValue::Float(rx_mbps)),
                ("tx", TokenValue::Float(tx_mbps)),
            ],
        );

        if interface.starts_with("tun")
            || interface.starts_with("wg")
            || interface.starts_with("ppp")
            || interface.starts_with("pvpn")
        {
            output_text = format!("  {}", output_text);
        }

        Ok(WaybarOutput {
            text: output_text,
            tooltip: Some(format!("Interface: {}\nIP: {}", interface, ip_display)),
            class: Some(interface),
            percentage: None,
        })
    }
}

fn get_primary_interface() -> Result<String> {
    let stdout = run_command("ip", &["route", "list"])?;

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
    let stdout = run_command("ip", &["-4", "addr", "show", interface]).ok()?;
    for line in stdout.lines() {
        if line.trim().starts_with("inet ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, NetworkState, mock_state};

    #[test]
    fn test_network_no_connection() {
        let state = mock_state(AppState::default());
        let config = Config::default();
        let output = NetworkModule.run(&config, &state, &[]).unwrap();
        assert_eq!(output.text, "No connection");
    }

    #[test]
    fn test_network_connected() {
        let state = mock_state(AppState {
            network: NetworkState {
                rx_mbps: 1.5,
                tx_mbps: 0.3,
                interface: "eth0".to_string(),
                ip: "192.168.1.100".to_string(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = NetworkModule.run(&config, &state, &[]).unwrap();
        assert!(output.text.contains("eth0"));
        assert!(output.text.contains("192.168.1.100"));
        assert!(output.text.contains("1.50"));
        assert_eq!(output.class.as_deref(), Some("eth0"));
    }

    #[test]
    fn test_network_vpn_prefix() {
        let state = mock_state(AppState {
            network: NetworkState {
                rx_mbps: 0.0,
                tx_mbps: 0.0,
                interface: "wg0".to_string(),
                ip: "10.0.0.1".to_string(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = NetworkModule.run(&config, &state, &[]).unwrap();
        assert!(output.text.starts_with("  "));
    }

    #[test]
    fn test_network_no_ip() {
        let state = mock_state(AppState {
            network: NetworkState {
                rx_mbps: 0.0,
                tx_mbps: 0.0,
                interface: "eth0".to_string(),
                ip: String::new(),
            },
            ..Default::default()
        });
        let config = Config::default();
        let output = NetworkModule.run(&config, &state, &[]).unwrap();
        assert!(output.text.contains("No IP"));
    }
}
