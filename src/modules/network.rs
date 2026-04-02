use crate::config::Config;
use crate::error::Result;
use crate::modules::WaybarModule;
use crate::output::WaybarOutput;
use crate::state::{AppReceivers, NetworkState};
use crate::utils::{TokenValue, format_template};
use nix::ifaddrs::getifaddrs;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

pub struct NetworkModule;

pub struct NetworkDaemon {
    last_time: u64,
    last_rx_bytes: u64,
    last_tx_bytes: u64,
    cached_interface: Option<String>,
    cached_ip: Option<String>,
}

type PollResult = crate::error::Result<(String, Option<String>, Option<(u64, u64)>)>;

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

    pub async fn poll(
        &mut self,
        state_tx: &watch::Sender<NetworkState>,
    ) -> crate::error::Result<()> {
        let (iface, ip_opt, bytes_opt) = tokio::task::spawn_blocking(|| -> PollResult {
            let iface = get_primary_interface()?;
            if iface.is_empty() {
                return Ok((String::new(), None, None));
            }
            let ip = get_ip_address(&iface);
            let bytes = get_bytes(&iface).ok();
            Ok((iface, ip, bytes))
        })
        .await
        .map_err(|e| crate::error::FluxoError::System(e.to_string()))??;

        if !iface.is_empty() {
            if self.cached_interface.as_ref() != Some(&iface) || self.cached_ip.is_none() {
                self.cached_ip = ip_opt;
                self.cached_interface = Some(iface.clone());
            }
        } else {
            self.cached_interface = None;
            self.cached_ip = None;
            // Provide a default state for "No connection"
            let mut network = state_tx.borrow().clone();
            network.interface.clear();
            network.ip.clear();
            network.rx_mbps = 0.0;
            network.tx_mbps = 0.0;
            let _ = state_tx.send(network);
            return Err(crate::error::FluxoError::Network(
                "No primary interface found".into(),
            ));
        }

        let interface = if let Some(ref interface) = self.cached_interface {
            interface.clone()
        } else {
            // No interface detected
            let mut network = state_tx.borrow().clone();
            network.interface.clear();
            network.ip.clear();
            network.rx_mbps = 0.0;
            network.tx_mbps = 0.0;
            let _ = state_tx.send(network);
            return Err(crate::error::FluxoError::Network(
                "Interface disappeared during poll".into(),
            ));
        };

        let time_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some((rx_bytes_now, tx_bytes_now)) = bytes_opt {
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

                let mut network = state_tx.borrow().clone();
                network.rx_mbps = rx_mbps;
                network.tx_mbps = tx_mbps;
                network.interface = interface.clone();
                network.ip = self.cached_ip.clone().unwrap_or_default();
                let _ = state_tx.send(network);
            } else {
                // First poll: no speed data yet, but update interface/ip
                let mut network = state_tx.borrow().clone();
                network.interface = interface.clone();
                network.ip = self.cached_ip.clone().unwrap_or_default();
                let _ = state_tx.send(network);
            }

            self.last_time = time_now;
            self.last_rx_bytes = rx_bytes_now;
            self.last_tx_bytes = tx_bytes_now;
        } else {
            // Read failed, might be down
            self.cached_interface = None;
            return Err(crate::error::FluxoError::Network(format!(
                "Failed to read bytes for {}",
                interface
            )));
        }

        Ok(())
    }
}

impl WaybarModule for NetworkModule {
    async fn run(
        &self,
        config: &Config,
        state: &AppReceivers,
        _args: &[&str],
    ) -> Result<WaybarOutput> {
        let (interface, ip, rx_mbps, tx_mbps) = {
            let s = state.network.borrow();
            (s.interface.clone(), s.ip.clone(), s.rx_mbps, s.tx_mbps)
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
    let content = std::fs::read_to_string("/proc/net/route")?;

    let mut defaults = Vec::new();
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 8 {
            let iface = parts[0];
            let dest = parts[1];
            let metric = parts[6].parse::<i32>().unwrap_or(0);
            let mask = u32::from_str_radix(parts[7], 16).unwrap_or(0);

            if dest == "00000000" {
                defaults.push((mask, metric, iface.to_string()));
            }
        }
    }

    // Sort by mask descending (longest prefix match first), then by metric ascending
    defaults.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if let Some((_, _, dev)) = defaults.first() {
        Ok(dev.clone())
    } else {
        Ok(String::new())
    }
}

fn get_ip_address(interface: &str) -> Option<String> {
    let addrs = getifaddrs().ok()?;
    for ifaddr in addrs {
        if ifaddr.interface_name == interface
            && let Some(address) = ifaddr.address
            && let Some(sockaddr) = address.as_sockaddr_in()
        {
            return Some(sockaddr.ip().to_string());
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

    #[tokio::test]
    async fn test_network_no_connection() {
        let state = mock_state(AppState::default());
        let config = Config::default();
        let output = NetworkModule
            .run(&config, &state.receivers, &[])
            .await
            .unwrap();
        assert_eq!(output.text, "No connection");
    }

    #[tokio::test]
    async fn test_network_connected() {
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
        let output = NetworkModule
            .run(&config, &state.receivers, &[])
            .await
            .unwrap();
        assert!(output.text.contains("eth0"));
        assert!(output.text.contains("192.168.1.100"));
        assert!(output.text.contains("1.50"));
        assert_eq!(output.class.as_deref(), Some("eth0"));
    }

    #[tokio::test]
    async fn test_network_vpn_prefix() {
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
        let output = NetworkModule
            .run(&config, &state.receivers, &[])
            .await
            .unwrap();
        assert!(output.text.starts_with("  "));
    }

    #[tokio::test]
    async fn test_network_no_ip() {
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
        let output = NetworkModule
            .run(&config, &state.receivers, &[])
            .await
            .unwrap();
        assert!(output.text.contains("No IP"));
    }
}
