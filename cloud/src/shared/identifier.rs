use std::{fs, path::Path};

/// Serial number and device identification utilities

const SN_FILE: &str = "device_sn.txt";
const DEFAULT_SN: &str = "TINYIOTHUB-DEFAULT-001";

/// Initialize serial number configuration
pub fn init_sn_config() {
    if !Path::new(SN_FILE).exists()
        && let Err(e) = fs::write(SN_FILE, DEFAULT_SN)
    {
        tracing::error!("Failed to create SN file: {}", e);
    }
}

/// Get device serial number
pub fn get_sn() -> String {
    match fs::read_to_string(SN_FILE) {
        Ok(sn) => {
            let trimmed = sn.trim();
            if trimmed.is_empty() { DEFAULT_SN.to_string() } else { trimmed.to_string() }
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
    "00:11:22:33:44:55".to_string()
}

/// Get device IP address (placeholder implementation)
pub fn get_ip_address() -> String {
    // 尝试从配置读取
    match crate::shared::config::get() {
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
    "AC Power".to_string()
}

/// Get device temperature (placeholder implementation)
pub fn get_device_temperature() -> f32 {
    25.0
}

/// Get memory usage information
pub fn get_memory_info() -> MemoryInfo {
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
        if self.total == 0 { 0.0 } else { (self.used as f32 / self.total as f32) * 100.0 }
    }
}

/// Get CPU usage percentage (placeholder implementation)
pub fn get_cpu_usage() -> f32 {
    15.5
}

/// Get disk usage information
pub fn get_disk_info() -> DiskInfo {
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
        if self.total == 0 { 0.0 } else { (self.used as f32 / self.total as f32) * 100.0 }
    }
}
/// Generate a unique ID using UUID v4
pub fn generate_id() -> String {
    tinyiothub_core::generate_id()
}

/// Generate current timestamp as "%Y-%m-%d %H:%M:%S" string (UTC)
pub fn now_string() -> String {
    tinyiothub_core::now_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(0), "0s");
        assert_eq!(format_uptime(45), "45s");
        assert_eq!(format_uptime(125), "2m 5s");
        assert_eq!(format_uptime(3665), "1h 1m 5s");
        assert_eq!(format_uptime(90061), "1d 1h 1m 1s");
        assert_eq!(format_uptime(86400), "1d 0h 0m 0s");
    }

    #[test]
    fn test_generate_sn_format() {
        let sn = generate_sn();
        assert!(sn.starts_with("TINYIOTHUB-"));
        assert_eq!(sn.len(), 17); // "TINYIOTHUB-" (11) + 6 digits
    }

    #[test]
    fn test_generate_short_id() {
        let id = generate_short_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_numeric_id_range() {
        let id = generate_numeric_id();
        assert!(id >= 1000000000);
        assert!(id <= 9999999999);
    }

    #[test]
    fn test_memory_info_usage_percentage() {
        let mem = MemoryInfo { total: 1024, used: 512, free: 512 };
        assert!((mem.usage_percentage() - 50.0).abs() < f32::EPSILON);

        let mem_zero = MemoryInfo { total: 0, used: 0, free: 0 };
        assert_eq!(mem_zero.usage_percentage(), 0.0);

        let mem_full = MemoryInfo { total: 100, used: 100, free: 0 };
        assert!((mem_full.usage_percentage() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_disk_info_usage_percentage() {
        let disk = DiskInfo { total: 1024, used: 256, free: 768 };
        assert!((disk.usage_percentage() - 25.0).abs() < f32::EPSILON);

        let disk_zero = DiskInfo { total: 0, used: 0, free: 0 };
        assert_eq!(disk_zero.usage_percentage(), 0.0);
    }
}
