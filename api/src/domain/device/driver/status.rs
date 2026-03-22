//! 设备状态管理
//!
//! 统一管理设备连接状态、健康状态和告警状态

use crate::dto::entity::Device;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// 设备连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionStatus {
    /// 未连接
    #[default]
    Disconnected = 0,
    /// 已连接
    Connected = 1,
    /// 告警状态
    Alarm = 2,
    /// 离线
    Offline = 3,
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected)
    }

    pub fn is_online(&self) -> bool {
        matches!(self, ConnectionStatus::Connected | ConnectionStatus::Alarm)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, ConnectionStatus::Offline)
    }
}

impl From<i32> for ConnectionStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => ConnectionStatus::Disconnected,
            1 => ConnectionStatus::Connected,
            2 => ConnectionStatus::Alarm,
            3 => ConnectionStatus::Offline,
            _ => ConnectionStatus::Disconnected,
        }
    }
}

impl From<ConnectionStatus> for String {
    fn from(status: ConnectionStatus) -> Self {
        (status as i32).to_string()
    }
}

impl From<&str> for ConnectionStatus {
    fn from(s: &str) -> Self {
        s.parse::<i32>()
            .map(ConnectionStatus::from)
            .unwrap_or(ConnectionStatus::Disconnected)
    }
}

/// 设备健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// 连接状态
    pub connection: ConnectionStatus,
    /// 最后成功通信时间 (使用 SystemTime 替代 Instant)
    pub last_success_time: Option<SystemTime>,
    /// 最后失败时间
    pub last_failure_time: Option<SystemTime>,
    /// 连续失败次数
    pub consecutive_failures: u32,
    /// 连续成功次数
    pub consecutive_successes: u32,
    /// 总通信次数
    pub total_attempts: u64,
    /// 总成功次数
    pub total_successes: u64,
    /// 总失败次数
    pub total_failures: u64,
    /// 平均响应时间
    pub average_response_time: Duration,
    /// 最后响应时间
    pub last_response_time: Option<Duration>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            connection: ConnectionStatus::default(),
            last_success_time: None,
            last_failure_time: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
            total_attempts: 0,
            total_successes: 0,
            total_failures: 0,
            average_response_time: Duration::from_millis(0),
            last_response_time: None,
        }
    }
}

impl HealthStatus {
    /// 记录成功通信
    pub fn record_success(&mut self, response_time: Duration) {
        self.connection = ConnectionStatus::Connected;
        self.last_success_time = Some(SystemTime::now());
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;
        self.total_attempts += 1;
        self.total_successes += 1;
        self.last_response_time = Some(response_time);

        // 更新平均响应时间
        if self.total_successes == 1 {
            self.average_response_time = response_time;
        } else {
            let total_time = self.average_response_time.as_millis() as u64
                * (self.total_successes - 1)
                + response_time.as_millis() as u64;
            self.average_response_time = Duration::from_millis(total_time / self.total_successes);
        }
    }

    /// 记录失败通信
    pub fn record_failure(&mut self) {
        self.connection = ConnectionStatus::Offline;
        self.last_failure_time = Some(SystemTime::now());
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_attempts += 1;
        self.total_failures += 1;
    }

    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            0.0
        } else {
            self.total_successes as f64 / self.total_attempts as f64
        }
    }

    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        self.connection.is_connected() && self.consecutive_failures < 3
    }

    /// 是否在线
    pub fn is_online(&self) -> bool {
        self.connection.is_online()
    }
}

/// 设备统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceOverview {
    /// 设备ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 健康状态
    pub health: HealthStatus,
    /// 启动时间
    pub start_time: SystemTime,
    /// 运行时长
    pub uptime: Duration,
    /// 最后更新时间
    pub last_update: SystemTime,
}

impl DeviceOverview {
    pub fn new(device: &Device) -> Self {
        let now = SystemTime::now();
        Self {
            device_id: device.id.clone(),
            device_name: device
                .display_name
                .clone()
                .unwrap_or_else(|| device.name.clone()),
            health: HealthStatus::default(),
            start_time: now,
            uptime: Duration::from_secs(0),
            last_update: now,
        }
    }

    /// 更新统计信息
    pub fn update(&mut self) {
        let now = SystemTime::now();
        self.uptime = now.duration_since(self.start_time).unwrap_or_default();
        self.last_update = now;
    }

    /// 记录成功操作
    pub fn record_success(&mut self, response_time: Duration) {
        self.health.record_success(response_time);
        self.update();
    }

    /// 记录失败操作
    pub fn record_failure(&mut self) {
        self.health.record_failure();
        self.update();
    }
}

/// 设备状态管理器
#[derive(Debug)]
pub struct DeviceStatusManager {
    /// 设备统计信息
    overview: DeviceOverview,
}

impl DeviceStatusManager {
    pub fn new(device: &Device) -> Self {
        Self {
            overview: DeviceOverview::new(device),
        }
    }

    /// 获取连接状态
    pub fn get_connection_status(&self) -> ConnectionStatus {
        self.overview.health.connection
    }

    /// 设置连接状态
    pub fn set_connection_status(&mut self, status: ConnectionStatus) {
        self.overview.health.connection = status;
        self.overview.update();
    }

    /// 记录成功操作
    pub fn record_success(&mut self, response_time: Duration) {
        self.overview.record_success(response_time);
    }

    /// 记录失败操作
    pub fn record_failure(&mut self) {
        self.overview.record_failure();
    }

    /// 获取统计信息
    pub fn get_statistics(&self) -> &DeviceOverview {
        &self.overview
    }

    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        self.overview.health.is_healthy()
    }

    /// 是否在线
    pub fn is_online(&self) -> bool {
        self.overview.health.is_online()
    }

    /// 重置状态
    pub fn reset(&mut self) {
        self.overview.health = HealthStatus::default();
        self.overview.start_time = SystemTime::now();
        self.overview.update();
    }

    /// 强制离线
    pub fn set_offline(&mut self) {
        self.overview.health.connection = ConnectionStatus::Offline;
        self.overview.health.last_failure_time = Some(SystemTime::now());
        self.overview.update();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> crate::dto::entity::Device {
        crate::dto::entity::Device {
            id: "device-test-001".to_string(),
            name: "Test Device".to_string(),
            display_name: Some("测试设备".to_string()),
            device_type: Some("sensor".to_string()),
            address: Some("192.168.1.100".to_string()),
            description: Some("测试用设备".to_string()),
            position: None,
            driver_name: Some("simulated".to_string()),
            device_model: None,
            protocol_type: None,
            factory_name: None,
            linked_data: None,
            driver_options: None,
            state: Some(1),
            parent_id: None,
            product_id: None,
            organization_id: None,
            created_at: None,
            updated_at: None,
            tags: None,
            properties: None,
            commands: None,
            is_online: false,
            last_heartbeat: None,
        }
    }

    // ===== ConnectionStatus tests =====

    #[test]
    fn test_connection_status_is_connected() {
        assert!(ConnectionStatus::Connected.is_connected());
        assert!(!ConnectionStatus::Disconnected.is_connected());
        assert!(!ConnectionStatus::Offline.is_connected());
        assert!(!ConnectionStatus::Alarm.is_connected());
    }

    #[test]
    fn test_connection_status_is_online() {
        assert!(ConnectionStatus::Connected.is_online());
        assert!(ConnectionStatus::Alarm.is_online());
        assert!(!ConnectionStatus::Disconnected.is_online());
        assert!(!ConnectionStatus::Offline.is_online());
    }

    #[test]
    fn test_connection_status_is_error() {
        assert!(ConnectionStatus::Offline.is_error());
        assert!(!ConnectionStatus::Connected.is_error());
        assert!(!ConnectionStatus::Disconnected.is_error());
        assert!(!ConnectionStatus::Alarm.is_error());
    }

    #[test]
    fn test_connection_status_from_i32() {
        assert_eq!(ConnectionStatus::from(0), ConnectionStatus::Disconnected);
        assert_eq!(ConnectionStatus::from(1), ConnectionStatus::Connected);
        assert_eq!(ConnectionStatus::from(2), ConnectionStatus::Alarm);
        assert_eq!(ConnectionStatus::from(3), ConnectionStatus::Offline);
        // Unknown values default to Disconnected
        assert_eq!(ConnectionStatus::from(99), ConnectionStatus::Disconnected);
        assert_eq!(ConnectionStatus::from(-1), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_connection_status_from_str() {
        assert_eq!(ConnectionStatus::from("0"), ConnectionStatus::Disconnected);
        assert_eq!(ConnectionStatus::from("1"), ConnectionStatus::Connected);
        assert_eq!(ConnectionStatus::from("2"), ConnectionStatus::Alarm);
        assert_eq!(ConnectionStatus::from("3"), ConnectionStatus::Offline);
        // Invalid strings default to Disconnected
        assert_eq!(ConnectionStatus::from("invalid"), ConnectionStatus::Disconnected);
        assert_eq!(ConnectionStatus::from(""), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_connection_status_as_i32() {
        // Test the From trait implementation gives expected values
        assert_eq!(ConnectionStatus::Disconnected as i32, 0);
        assert_eq!(ConnectionStatus::Connected as i32, 1);
        assert_eq!(ConnectionStatus::Alarm as i32, 2);
        assert_eq!(ConnectionStatus::Offline as i32, 3);
    }

    // ===== HealthStatus tests =====

    #[test]
    fn test_health_status_default() {
        let health = HealthStatus::default();
        assert_eq!(health.connection, ConnectionStatus::Disconnected);
        assert!(health.last_success_time.is_none());
        assert!(health.last_failure_time.is_none());
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.consecutive_successes, 0);
        assert_eq!(health.total_attempts, 0);
        assert_eq!(health.total_successes, 0);
        assert_eq!(health.total_failures, 0);
    }

    #[test]
    fn test_health_status_record_success() {
        let mut health = HealthStatus::default();
        let response_time = Duration::from_millis(100);

        health.record_success(response_time);

        assert_eq!(health.connection, ConnectionStatus::Connected);
        assert!(health.last_success_time.is_some());
        assert_eq!(health.consecutive_successes, 1);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.total_attempts, 1);
        assert_eq!(health.total_successes, 1);
        assert_eq!(health.total_failures, 0);
        assert_eq!(health.average_response_time, response_time);
    }

    #[test]
    fn test_health_status_record_failure() {
        let mut health = HealthStatus::default();

        health.record_failure();

        assert_eq!(health.connection, ConnectionStatus::Offline);
        assert!(health.last_failure_time.is_some());
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.consecutive_successes, 0);
        assert_eq!(health.total_attempts, 1);
        assert_eq!(health.total_successes, 0);
        assert_eq!(health.total_failures, 1);
    }

    #[test]
    fn test_health_status_average_response_time() {
        let mut health = HealthStatus::default();

        // Record first success
        health.record_success(Duration::from_millis(100));
        assert_eq!(health.average_response_time, Duration::from_millis(100));

        // Record second success with different time
        health.record_success(Duration::from_millis(200));
        // Average should be (100 + 200) / 2 = 150
        assert_eq!(health.average_response_time, Duration::from_millis(150));

        // Record third success
        health.record_success(Duration::from_millis(300));
        // Average should be (100 + 200 + 300) / 3 = 200
        assert_eq!(health.average_response_time, Duration::from_millis(200));
    }

    #[test]
    fn test_health_status_success_rate() {
        let mut health = HealthStatus::default();

        // No attempts = 0 rate
        assert_eq!(health.success_rate(), 0.0);

        // 1 success out of 1 = 100%
        health.record_success(Duration::from_millis(100));
        assert_eq!(health.success_rate(), 1.0);

        // 1 failure
        health.record_failure();
        // 1 success out of 2 = 50%
        assert_eq!(health.success_rate(), 0.5);

        // Another failure
        health.record_failure();
        // 1 success out of 3 ≈ 33.33%
        assert!((health.success_rate() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_health_status_is_healthy() {
        let mut health = HealthStatus::default();

        // Default is not connected -> not healthy
        assert!(!health.is_healthy());

        // Connected with no failures -> healthy
        health.record_success(Duration::from_millis(100));
        assert!(health.is_healthy());

        // After 1 failure: connection becomes Offline -> not healthy
        health.record_failure();
        assert!(!health.is_healthy());

        // After recovering with success: healthy again
        health.record_success(Duration::from_millis(100));
        assert!(health.is_healthy());

        // Multiple failures with recovery
        health.record_failure();
        health.record_failure();
        // After 2 failures with connection Offline -> not healthy
        assert!(!health.is_healthy());
        // Recover
        health.record_success(Duration::from_millis(50));
        assert!(health.is_healthy());
    }

    // ===== DeviceOverview tests =====

    #[test]
    fn test_device_overview_new() {
        let device = create_test_device();
        let overview = DeviceOverview::new(&device);

        assert_eq!(overview.device_id, "device-test-001");
        assert_eq!(overview.device_name, "测试设备");
        assert_eq!(overview.health.connection, ConnectionStatus::Disconnected);
        assert_eq!(overview.uptime, Duration::from_secs(0));
    }

    #[test]
    fn test_device_overview_display_name_fallback() {
        let device = crate::dto::entity::Device {
            id: "dev-001".to_string(),
            name: "MyDevice".to_string(),
            display_name: None, // No display name
            device_type: None,
            address: None,
            description: None,
            position: None,
            driver_name: None,
            device_model: None,
            protocol_type: None,
            factory_name: None,
            linked_data: None,
            driver_options: None,
            state: None,
            parent_id: None,
            product_id: None,
            organization_id: None,
            created_at: None,
            updated_at: None,
            tags: None,
            properties: None,
            commands: None,
            is_online: false,
            last_heartbeat: None,
        };

        let overview = DeviceOverview::new(&device);
        // Should fall back to name when display_name is None
        assert_eq!(overview.device_name, "MyDevice");
    }

    #[test]
    fn test_device_overview_record_success() {
        let device = create_test_device();
        let mut overview = DeviceOverview::new(&device);

        overview.record_success(Duration::from_millis(50));

        assert_eq!(overview.health.connection, ConnectionStatus::Connected);
        assert_eq!(overview.health.consecutive_successes, 1);
        assert!(overview.uptime > Duration::from_secs(0));
    }

    #[test]
    fn test_device_overview_record_failure() {
        let device = create_test_device();
        let mut overview = DeviceOverview::new(&device);

        overview.record_failure();

        assert_eq!(overview.health.connection, ConnectionStatus::Offline);
        assert_eq!(overview.health.consecutive_failures, 1);
    }

    // ===== DeviceStatusManager tests =====

    #[test]
    fn test_device_status_manager_new() {
        let device = create_test_device();
        let manager = DeviceStatusManager::new(&device);

        assert_eq!(manager.get_connection_status(), ConnectionStatus::Disconnected);
        // A newly created manager with no recorded operations is not healthy
        // (Disconnected status means not healthy)
        assert!(!manager.is_healthy());
        assert!(!manager.is_online());
    }

    #[test]
    fn test_device_status_manager_set_connection_status() {
        let device = create_test_device();
        let mut manager = DeviceStatusManager::new(&device);

        manager.set_connection_status(ConnectionStatus::Connected);
        assert_eq!(manager.get_connection_status(), ConnectionStatus::Connected);
        assert!(manager.is_online());

        manager.set_connection_status(ConnectionStatus::Alarm);
        assert_eq!(manager.get_connection_status(), ConnectionStatus::Alarm);
        assert!(manager.is_online());

        manager.set_connection_status(ConnectionStatus::Offline);
        assert_eq!(manager.get_connection_status(), ConnectionStatus::Offline);
        assert!(!manager.is_online());
    }

    #[test]
    fn test_device_status_manager_record_operations() {
        let device = create_test_device();
        let mut manager = DeviceStatusManager::new(&device);

        // Record success
        manager.record_success(Duration::from_millis(100));
        assert!(manager.is_healthy());
        assert!(manager.is_online());

        // Record failure
        manager.record_failure();
        assert!(!manager.is_healthy());
    }

    #[test]
    fn test_device_status_manager_reset() {
        let device = create_test_device();
        let mut manager = DeviceStatusManager::new(&device);

        // Change state
        manager.set_connection_status(ConnectionStatus::Connected);
        manager.record_success(Duration::from_millis(100));

        // Reset
        manager.reset();

        assert_eq!(manager.get_connection_status(), ConnectionStatus::Disconnected);
        assert!(!manager.is_online());
    }

    #[test]
    fn test_device_status_manager_set_offline() {
        let device = create_test_device();
        let mut manager = DeviceStatusManager::new(&device);

        manager.record_success(Duration::from_millis(100));
        assert!(manager.is_online());

        manager.set_offline();

        assert_eq!(manager.get_connection_status(), ConnectionStatus::Offline);
        assert!(!manager.is_online());
        assert!(manager.get_statistics().health.last_failure_time.is_some());
    }

    #[test]
    fn test_device_status_manager_statistics() {
        let device = create_test_device();
        let mut manager = DeviceStatusManager::new(&device);

        manager.record_success(Duration::from_millis(50));
        manager.record_success(Duration::from_millis(100));
        manager.record_failure();

        let stats = manager.get_statistics();
        assert_eq!(stats.health.total_attempts, 3);
        assert_eq!(stats.health.total_successes, 2);
        assert_eq!(stats.health.total_failures, 1);
    }
}
