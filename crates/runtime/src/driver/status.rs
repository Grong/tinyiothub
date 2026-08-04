//! 设备状态管理
//!
//! 统一管理设备连接状态、健康状态和告警状态。

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device::DeviceStatus;

/// 设备健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub connection: DeviceStatus,
    pub last_success_time: Option<SystemTime>,
    pub last_failure_time: Option<SystemTime>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub total_attempts: u64,
    pub total_successes: u64,
    pub total_failures: u64,
    pub average_response_time: Duration,
    pub last_response_time: Option<Duration>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            connection: DeviceStatus::default(),
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
    pub fn record_success(&mut self, response_time: Duration) {
        self.connection = DeviceStatus::Online;
        self.last_success_time = Some(SystemTime::now());
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;
        self.total_attempts += 1;
        self.total_successes += 1;
        self.last_response_time = Some(response_time);

        if self.total_successes == 1 {
            self.average_response_time = response_time;
        } else {
            let total_time = self.average_response_time.as_millis() as u64 * (self.total_successes - 1)
                + response_time.as_millis() as u64;
            self.average_response_time = Duration::from_millis(total_time / self.total_successes);
        }
    }

    pub fn record_failure(&mut self) {
        self.connection = DeviceStatus::Offline;
        self.last_failure_time = Some(SystemTime::now());
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_attempts += 1;
        self.total_failures += 1;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            0.0
        } else {
            self.total_successes as f64 / self.total_attempts as f64
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.connection.is_online() && self.consecutive_failures < 3
    }

    pub fn is_online(&self) -> bool {
        self.connection.is_online()
    }
}

/// 设备统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceOverview {
    pub device_id: String,
    pub device_name: String,
    pub health: HealthStatus,
    pub start_time: SystemTime,
    pub uptime: Duration,
    pub last_update: SystemTime,
}

impl DeviceOverview {
    pub fn new(device: &Device) -> Self {
        let now = SystemTime::now();
        Self {
            device_id: device.id.clone(),
            device_name: device.display_name.clone().unwrap_or_else(|| device.name.clone()),
            health: HealthStatus::default(),
            start_time: now,
            uptime: Duration::from_secs(0),
            last_update: now,
        }
    }

    pub fn update(&mut self) {
        let now = SystemTime::now();
        self.uptime = now.duration_since(self.start_time).unwrap_or_default();
        self.last_update = now;
    }

    pub fn record_success(&mut self, response_time: Duration) {
        self.health.record_success(response_time);
        self.update();
    }

    pub fn record_failure(&mut self) {
        self.health.record_failure();
        self.update();
    }
}

/// 设备状态管理器
#[derive(Debug)]
pub struct DeviceStatusManager {
    overview: DeviceOverview,
}

impl DeviceStatusManager {
    pub fn new(device: &Device) -> Self {
        Self {
            overview: DeviceOverview::new(device),
        }
    }

    pub fn get_connection_status(&self) -> DeviceStatus {
        self.overview.health.connection.clone()
    }

    pub fn set_connection_status(&mut self, status: DeviceStatus) {
        self.overview.health.connection = status;
        self.overview.update();
    }

    pub fn record_success(&mut self, response_time: Duration) {
        self.overview.record_success(response_time);
    }

    pub fn record_failure(&mut self) {
        self.overview.record_failure();
    }

    pub fn get_statistics(&self) -> &DeviceOverview {
        &self.overview
    }

    pub fn is_healthy(&self) -> bool {
        self.overview.health.is_healthy()
    }

    pub fn is_online(&self) -> bool {
        self.overview.health.is_online()
    }

    pub fn reset(&mut self) {
        self.overview.health = HealthStatus::default();
        self.overview.start_time = SystemTime::now();
        self.overview.update();
    }

    pub fn soft_reset(&mut self) {
        self.overview.health.connection = DeviceStatus::Offline;
        self.overview.health.consecutive_failures = 0;
        self.overview.health.consecutive_successes = 0;
        self.overview.health.last_failure_time = None;
        self.overview.health.last_success_time = None;
        self.overview.health.last_response_time = None;
        self.overview.update();
    }

    pub fn set_offline(&mut self) {
        self.overview.health.connection = DeviceStatus::Offline;
        self.overview.health.last_failure_time = Some(SystemTime::now());
        self.overview.update();
    }
}
