use crate::config::Config;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::SharedState;
use crate::utils::{TokenValue, format_template};
use anyhow::Result;
use nix::ifaddrs::getifaddrs;
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
        // Re-detect interface on every poll to catch VPNs or route changes immediately
        if let Ok(iface) = get_primary_interface()
            && !iface.is_empty()
        {
            // If the interface changed, or we don't have an IP yet, update the IP
            if self.cached_interface.as_ref() != Some(&iface) || self.cached_ip.is_none() {
                self.cached_ip = get_ip_address(&iface);
                self.cached_interface = Some(iface);
            }
        } else {
            self.cached_interface = None;
            self.cached_ip = None;
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
                ("interface", TokenValue::String(interface.clone())),
                ("ip", TokenValue::String(ip_display.to_string())),
                ("rx", TokenValue::Float(rx_mbps)),
                ("tx", TokenValue::Float(tx_mbps)),
            ],
        );

        if interface.starts_with("tun")
            || interface.starts_with("wg")
            || interface.starts_with("ppp")
            || interface.starts_with("pvpn")
            || interface.starts_with("proton")
            || interface.starts_with("ipsec")
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
    let content = fs::read_to_string("/proc/net/route")?;

    let mut defaults = Vec::new();
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 7 {
            let iface = parts[0];
            let dest = parts[1];
            let metric = parts[6].parse::<i32>().unwrap_or(0);

            if dest == "00000000" {
                defaults.push((metric, iface.to_string()));
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
    let addrs = getifaddrs().ok()?;
    for ifaddr in addrs {
        if ifaddr.interface_name == interface {
            if let Some(address) = ifaddr.address {
                if let Some(sockaddr) = address.as_sockaddr_in() {
                    return Some(sockaddr.ip().to_string());
                }
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
