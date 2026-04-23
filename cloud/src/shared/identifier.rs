use std::{fs, path::Path};

/// Serial number and device identification utilities

const SN_FILE: &str = "device_sn.txt";
const DEFAULT_SN: &str = "TINYIOTHUB-DEFAULT-001";

/// Initialize serial number configuration
pub fn init_sn_config() {
    if !Path::new(SN_FILE).exists()
        && let Err(e) = fs::write(SN_FILE, DEFAULT_SN) {
            tracing::error!("Failed to create SN file: {}", e);
        }
}

/// Get device serial number
pub fn get_sn() -> String {
    match fs::read_to_string(SN_FILE) {
        Ok(sn) => {
            let trimmed = sn.trim();
            if trimmed.is_empty() {
                DEFAULT_SN.to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => {
            tracing::warn!("Failed to read SN file, using default");
            DEFAULT_SN.to_string()
        }
    }
}

/// Set device serial number
pub fn set_sn(sn: &str) -> bool {
    match fs::write(SN_FILE, sn) {
        Ok(_) => {
            tracing::info!("Serial number updated: {}", sn);
            true
        }
        Err(e) => {
            tracing::error!("Failed to write SN file: {}", e);
            false
        }
    }
}

/// Generate a new random serial number
pub fn generate_sn() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_part: u32 = rng.gen_range(100000..999999);
    format!("TINYIOTHUB-{:06}", random_part)
}

/// Get device MAC address (placeholder implementation)
pub fn get_mac_address() -> String {
    // TODO: Implement actual MAC address retrieval
    "00:11:22:33:44:55".to_string()
}

/// Get device IP address (placeholder implementation)
pub fn get_ip_address() -> String {
    // 尝试从配置读取
    match crate::infrastructure::config::get() {
        config => config.network.defaults.ip_address.clone(),
    }
}

/// Get device hostname
pub fn get_hostname() -> String {
    match std::env::var("HOSTNAME") {
        Ok(hostname) => hostname,
        Err(_) => {
            // Try to get from system
            match std::process::Command::new("hostname").output() {
                Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
                Err(_) => "tinyiothub".to_string(),
            }
        }
    }
}

/// Get system uptime in seconds
pub fn get_uptime_seconds() -> u64 {
    // TODO: Implement actual uptime retrieval
    0
}

/// Format uptime as human readable string
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, secs)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Get device power status (placeholder implementation)
pub fn get_power_status() -> String {
    // TODO: Implement actual power status retrieval
    "AC Power".to_string()
}

/// Get device temperature (placeholder implementation)
pub fn get_device_temperature() -> f32 {
    // TODO: Implement actual temperature reading
    25.0
}

/// Get memory usage information
pub fn get_memory_info() -> MemoryInfo {
    // TODO: Implement actual memory info retrieval
    MemoryInfo {
        total: 1024 * 1024 * 1024, // 1GB
        used: 512 * 1024 * 1024,   // 512MB
        free: 512 * 1024 * 1024,   // 512MB
    }
}

/// Memory information structure
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

impl MemoryInfo {
    /// Get memory usage percentage
    pub fn usage_percentage(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f32 / self.total as f32) * 100.0
        }
    }
}

/// Get CPU usage percentage (placeholder implementation)
pub fn get_cpu_usage() -> f32 {
    // TODO: Implement actual CPU usage retrieval
    15.5
}

/// Get disk usage information
pub fn get_disk_info() -> DiskInfo {
    // TODO: Implement actual disk info retrieval
    DiskInfo {
        total: 8 * 1024 * 1024 * 1024, // 8GB
        used: 2 * 1024 * 1024 * 1024,  // 2GB
        free: 6 * 1024 * 1024 * 1024,  // 6GB
    }
}

/// Disk information structure
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

impl DiskInfo {
    /// Get disk usage percentage
    pub fn usage_percentage(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f32 / self.total as f32) * 100.0
        }
    }
}
/// Generate a unique ID using UUID v4
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate a short ID (8 characters)
pub fn generate_short_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();

    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a numeric ID
pub fn generate_numeric_id() -> u64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(1000000000..9999999999)
}
