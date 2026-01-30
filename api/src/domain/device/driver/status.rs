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
