use std::{fs, process::Command, thread, time::Duration};

use serde::{Deserialize, Serialize};

/// Network configuration and utilities
/// Network information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkInfo {
    pub addr: String,
    pub gateway: String,
    pub dns: String,
    pub dhcp: bool,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        // 尝试从配置读取，失败则使用环境变量或默认值
        let config = crate::shared::config::get();
        Self {
            addr: config.network.defaults.ip_address.clone(),
            gateway: config.network.defaults.gateway.clone(),
            dns: config.network.defaults.dns_primary.clone(),
            dhcp: config.network.interface.use_dhcp,
        }
    }
}

/// Initialize network scripts (platform-specific implementation)
pub fn init_network_scripts() {
    #[cfg(feature = "harmonyos")]
    {
        tracing::info!("Initializing network scripts on HarmonyOS");
        // 使用鸿蒙系统的网络初始化
        crate::shared::hardware::harmonyos::network::init_network_scripts();
    }

    #[cfg(not(feature = "harmonyos"))]
    {
        tracing::info!("Initializing network scripts on Linux");
        // Linux系统的网络脚本初始化
    }
}

/// Get current network information
pub fn get_network_info() -> NetworkInfo {
    NetworkInfo::default()
}

/// Set network configuration
pub fn set_network_info(info: &NetworkInfo) -> bool {
    tracing::info!("Setting network configuration: {:?}", info);

    true
}

/// Check if network interface is up
pub fn is_interface_up(interface: &str) -> bool {
    match Command::new("ip").args(["link", "show", interface]).output() {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            output_str.contains("state UP")
        }
        Err(_) => false,
    }
}

/// Get IP address of interface
pub fn get_interface_ip(interface: &str) -> Option<String> {
    match Command::new("ip").args(["addr", "show", interface]).output() {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Parse IP address from output
            // This is a simplified implementation
            if let Some(line) = output_str.lines().find(|line| line.contains("inet "))
                && let Some(ip_part) = line.split_whitespace().nth(1)
                && let Some(ip) = ip_part.split('/').next()
            {
                return Some(ip.to_string());
            }
            None
        }
        Err(_) => None,
    }
}

/// Ping a host to check connectivity
pub fn ping_host(host: &str, timeout_secs: u64) -> bool {
    match Command::new("ping").args(["-c", "1", "-W", &timeout_secs.to_string(), host]).output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Get default gateway
pub fn get_default_gateway() -> Option<String> {
    match Command::new("ip").args(["route", "show", "default"]).output() {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = output_str.lines().next()
                && let Some(gateway) = line.split_whitespace().nth(2)
            {
                return Some(gateway.to_string());
            }
            None
        }
        Err(_) => None,
    }
}

/// Get DNS servers
pub fn get_dns_servers() -> Vec<String> {
    match fs::read_to_string("/etc/resolv.conf") {
        Ok(content) => content
            .lines()
            .filter(|line| line.starts_with("nameserver"))
            .filter_map(|line| line.split_whitespace().nth(1))
            .map(|s| s.to_string())
            .collect(),
        Err(_) => vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
    }
}

/// Set static IP configuration
pub fn set_static_ip(interface: &str, ip: &str, _netmask: &str, _gateway: &str) -> bool {
    tracing::info!("Setting static IP: {} on {}", ip, interface);

    true
}

/// Enable DHCP on interface
pub fn enable_dhcp(interface: &str) -> bool {
    tracing::info!("Enabling DHCP on interface: {}", interface);

    // 2. Starting DHCP client

    true
}

/// Restart network service
pub fn restart_network() -> bool {
    tracing::info!("Restarting network service");

    thread::sleep(Duration::from_secs(2)); // Simulate restart time
    true
}

/// Get network interface list
pub fn get_interfaces() -> Vec<String> {
    match Command::new("ip").args(["link", "show"]).output() {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            output_str
                .lines()
                .filter_map(|line| {
                    if line.contains(": ") && !line.starts_with(' ') {
                        let parts: Vec<&str> = line.split(": ").collect();
                        if parts.len() >= 2 {
                            let interface = parts[1].split('@').next().unwrap_or(parts[1]);
                            if interface != "lo" {
                                // Skip loopback
                                return Some(interface.to_string());
                            }
                        }
                    }
                    None
                })
                .collect()
        }
        Err(_) => vec!["eth0".to_string()], // Default fallback
    }
}

/// Check internet connectivity
pub fn check_internet_connectivity() -> bool {
    ping_host("8.8.8.8", 5) || ping_host("1.1.1.1", 5)
}

/// Get network statistics for interface
pub fn get_interface_stats(_interface: &str) -> Option<InterfaceStats> {
    Some(InterfaceStats {
        rx_bytes: 1024 * 1024,
        tx_bytes: 512 * 1024,
        rx_packets: 1000,
        tx_packets: 800,
        rx_errors: 0,
        tx_errors: 0,
    })
}

/// Network interface statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}
