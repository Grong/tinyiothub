// Device monitoring service — migrated from domain/device/monitoring_service.rs

use std::sync::Arc;
use tinyiothub_storage::cache::DeviceCache;
use crate::shared::persistence::Database;
use crate::shared::persistence::repositories::AlarmRepositoryImpl;

pub struct DeviceMonitoringService {
    database: Arc<Database>,
    device_cache: Arc<DeviceCache>,
    alarm_repository: Arc<AlarmRepositoryImpl>,
}

impl DeviceMonitoringService {
    pub fn new(database: Arc<Database>, device_cache: Arc<DeviceCache>, alarm_repository: Arc<AlarmRepositoryImpl>) -> Self {
        Self { database, device_cache, alarm_repository }
    }

    pub fn is_device_online(&self, device_id: &str) -> bool {
        if let Some(device) = self.device_cache.get(device_id) {
            if let Some(state) = device.state && state == 0 {
                return false;
            }
            if !device.is_online {
                // For simulation drivers, skip this check
            }
            if let Some(last_heartbeat) = &device.last_heartbeat
                && let Ok(heartbeat_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_heartbeat), "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    let heartbeat_threshold = now - chrono::Duration::minutes(5);
                    if heartbeat_time.with_timezone(&chrono::Utc) < heartbeat_threshold {
                        return false;
                    }
                }
            if let Some(updated_at) = &device.updated_at
                && let Ok(update_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", updated_at), "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    let update_threshold = now - chrono::Duration::hours(24);
                    if update_time.with_timezone(&chrono::Utc) < update_threshold {
                        return false;
                    }
                }
            true
        } else {
            false
        }
    }

    pub fn get_device_connection_quality(&self, device_id: &str) -> Option<u8> {
        if let Some(device) = self.device_cache.get(device_id) {
            let mut score = 100u8;
            if let Some(last_heartbeat) = &device.last_heartbeat {
                if let Ok(heartbeat_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_heartbeat), "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    let minutes_since_heartbeat = (now - heartbeat_time.with_timezone(&chrono::Utc)).num_minutes();
                    match minutes_since_heartbeat {
                        0..=1 => {}
                        2..=3 => score = score.saturating_sub(10),
                        4..=5 => score = score.saturating_sub(30),
                        6..=10 => score = score.saturating_sub(50),
                        _ => score = 0,
                    }
                }
            } else {
                score = score.saturating_sub(20);
            }
            if let Some(properties) = &device.properties && !properties.is_empty() {
                let now = chrono::Utc::now();
                let active_count = properties.iter().filter(|p| {
                    if let Some(last_update) = &p.updated_at
                        && let Ok(update_time) = chrono::DateTime::parse_from_str(
                            &format!("{} +00:00", last_update), "%Y-%m-%d %H:%M:%S %z",
                        ) {
                            let minutes_since_update = (now - update_time.with_timezone(&chrono::Utc)).num_minutes();
                            return minutes_since_update <= 5;
                        }
                    false
                }).count();
                let activity_ratio = active_count as f64 / properties.len() as f64;
                if activity_ratio < 0.5 { score = score.saturating_sub(15); }
                else if activity_ratio < 0.8 { score = score.saturating_sub(5); }
            }
            Some(score)
        } else {
            None
        }
    }

    pub async fn get_device_metrics(&self, device_id: &str) -> Option<DeviceMetrics> {
        if let Some(_device) = self.device_cache.get(device_id) {
            let device_repository: Arc<dyn crate::modules::device::repository::DeviceRepository> =
                Arc::new(crate::shared::persistence::repositories::SqliteDeviceRepository::new(self.database.as_ref().clone()));
            let device_service = super::service::DeviceService::new(device_repository, self.database.clone());

            let properties = device_service.get_device_properties(device_id).await.unwrap_or_default();
            let commands = device_service.get_device_commands(device_id).await.unwrap_or_default();

            let total_properties = properties.len() as u32;
            let total_commands = commands.len() as u32;

            let now = chrono::Utc::now();
            let online_threshold = now - chrono::Duration::minutes(5);
            let online_properties = properties.iter().filter(|p| {
                if let Some(last_update) = &p.updated_at {
                    if let Ok(update_time) = chrono::DateTime::parse_from_str(
                        &format!("{} +00:00", last_update), "%Y-%m-%d %H:%M:%S %z",
                    ) {
                        return update_time.with_timezone(&chrono::Utc) > online_threshold;
                    }
                }
                false
            }).count() as u32;

            let offline_properties = total_properties - online_properties;
            let (total_events, active_alarms) = self.get_device_events_and_alarms(device_id).await;

            Some(DeviceMetrics { total_properties, online_properties, offline_properties, total_commands, total_events, active_alarms })
        } else {
            None
        }
    }

    async fn get_device_events_and_alarms(&self, device_id: &str) -> (u32, u32) {
        let total_events = 0u32;
        let active_alarms = self.alarm_repository.count_active_alarms_by_device(device_id).await.unwrap_or(0);
        (total_events, active_alarms)
    }

    pub async fn get_system_overview(&self) -> SystemOverview {
        let all_devices = self.device_cache.all();
        let total_devices = all_devices.len() as u32;
        let mut online_devices = 0u32;
        let mut offline_devices = 0u32;
        let mut total_properties = 0u32;
        let mut total_commands = 0u32;
        for device in &all_devices {
            if self.is_device_online(&device.id) { online_devices += 1; } else { offline_devices += 1; }
            if let Some(properties) = &device.properties { total_properties += properties.len() as u32; }
            if let Some(commands) = &device.commands { total_commands += commands.len() as u32; }
        }
        let total_alarms = self.alarm_repository.count_all_active_alarms().await.unwrap_or(0);
        SystemOverview { total_devices, online_devices, offline_devices, total_properties, total_commands, total_alarms }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceMetrics {
    pub total_properties: u32,
    pub online_properties: u32,
    pub offline_properties: u32,
    pub total_commands: u32,
    pub total_events: u32,
    pub active_alarms: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemOverview {
    pub total_devices: u32,
    pub online_devices: u32,
    pub offline_devices: u32,
    pub total_properties: u32,
    pub total_commands: u32,
    pub total_alarms: u32,
}
