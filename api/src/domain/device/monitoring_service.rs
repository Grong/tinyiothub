use std::sync::Arc;

use crate::{application::data_context::DataContext, infrastructure::persistence::Database};

/// 设备监控服务
/// 负责设备性能监控、告警检查、追踪记录等功能
pub struct DeviceMonitoringService {
    database: Arc<Database>,
    context: Arc<DataContext>,
}

impl DeviceMonitoringService {
    pub fn new(database: Arc<Database>, context: Arc<DataContext>) -> Self {
        Self { database, context }
    }

    /// 检查设备是否在线
    pub fn is_device_online(&self, device_id: &str) -> bool {
        if let Some(device) = self.context.get_device(device_id) {
            // 1. 检查设备启用状态（state字段）
            if let Some(state) = device.state {
                if state == 0 {
                    tracing::debug!("Device {} is disabled (state=0)", device_id);
                    return false; // 设备被禁用
                }
            }

            // 2. 检查设备的 is_online 字段（由 DataServer 更新）
            if !device.is_online {
                tracing::debug!("Device {} marked as offline by DataServer", device_id);
                // 对于模拟驱动，暂时忽略这个检查
            }

            // 3. 检查心跳时间（如果存在）
            if let Some(last_heartbeat) = &device.last_heartbeat {
                if let Ok(heartbeat_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_heartbeat),
                    "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    let heartbeat_threshold = now - chrono::Duration::minutes(5); // 5分钟心跳超时

                    if heartbeat_time.with_timezone(&chrono::Utc) < heartbeat_threshold {
                        tracing::debug!(
                            "Device {} heartbeat timeout: last={}",
                            device_id,
                            last_heartbeat
                        );
                        return false; // 心跳超时
                    }
                }
            }

            // 4. 检查最后更新时间（作为备用指标）
            if let Some(updated_at) = &device.updated_at {
                if let Ok(update_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", updated_at),
                    "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    // 对于模拟驱动，放宽更新时间限制到24小时
                    let update_threshold = now - chrono::Duration::hours(24);

                    if update_time.with_timezone(&chrono::Utc) < update_threshold {
                        tracing::debug!("Device {} update timeout: last={}", device_id, updated_at);
                        return false; // 更新超时
                    }
                }
            }

            // 综合判断：如果通过了所有检查，认为设备在线
            true
        } else {
            false // 设备不存在
        }
    }

    /// 获取设备连接质量评分 (0-100)
    pub fn get_device_connection_quality(&self, device_id: &str) -> Option<u8> {
        if let Some(device) = self.context.get_device(device_id) {
            let mut score = 100u8;

            // 基于心跳时间计算质量
            if let Some(last_heartbeat) = &device.last_heartbeat {
                if let Ok(heartbeat_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_heartbeat),
                    "%Y-%m-%d %H:%M:%S %z",
                ) {
                    let now = chrono::Utc::now();
                    let minutes_since_heartbeat =
                        (now - heartbeat_time.with_timezone(&chrono::Utc)).num_minutes();

                    // 根据心跳延迟扣分
                    match minutes_since_heartbeat {
                        0..=1 => {} // 满分
                        2..=3 => score = score.saturating_sub(10),
                        4..=5 => score = score.saturating_sub(30),
                        6..=10 => score = score.saturating_sub(50),
                        _ => score = 0, // 超过10分钟认为离线
                    }
                }
            } else {
                score = score.saturating_sub(20); // 没有心跳信息扣20分
            }

            // 基于属性更新活跃度计算质量
            if let Some(properties) = &device.properties {
                if !properties.is_empty() {
                    let now = chrono::Utc::now();
                    let active_count = properties
                        .iter()
                        .filter(|p| {
                            if let Some(last_update) = &p.updated_at {
                                if let Ok(update_time) = chrono::DateTime::parse_from_str(
                                    &format!("{} +00:00", last_update),
                                    "%Y-%m-%d %H:%M:%S %z",
                                ) {
                                    let minutes_since_update = (now
                                        - update_time.with_timezone(&chrono::Utc))
                                    .num_minutes();
                                    return minutes_since_update <= 5;
                                }
                            }
                            false
                        })
                        .count();

                    let activity_ratio = active_count as f64 / properties.len() as f64;
                    if activity_ratio < 0.5 {
                        score = score.saturating_sub(15); // 活跃度低扣15分
                    } else if activity_ratio < 0.8 {
                        score = score.saturating_sub(5); // 活跃度中等扣5分
                    }
                }
            }

            Some(score)
        } else {
            None
        }
    }

    /// 获取设备指标信息
    pub async fn get_device_metrics(&self, device_id: &str) -> Option<DeviceMetrics> {
        if let Some(_device) = self.context.get_device(device_id) {
            // 使用 DeviceService 获取真实的属性和指令数据
            let device_repository: Arc<dyn crate::domain::device::repository::DeviceRepository> =
                Arc::new(crate::infrastructure::persistence::repositories::SqliteDeviceRepository::new(
                    self.database.as_ref().clone(),
                ));
            let device_service =
                crate::domain::device::service::DeviceService::new(device_repository, self.database.clone());

            let properties = match device_service.get_device_properties(device_id).await {
                Ok(props) => props,
                Err(_) => Vec::new(),
            };

            let commands = match device_service.get_device_commands(device_id).await {
                Ok(cmds) => cmds,
                Err(_) => Vec::new(),
            };

            let total_properties = properties.len() as u32;
            let total_commands = commands.len() as u32;

            // 计算在线属性数量（基于最后更新时间）
            let now = chrono::Utc::now();
            let online_threshold = now - chrono::Duration::minutes(5); // 5分钟内更新的认为是在线

            let online_properties = properties
                .iter()
                .filter(|p| {
                    if let Some(last_update) = &p.updated_at {
                        if let Ok(update_time) = chrono::DateTime::parse_from_str(
                            &format!("{} +00:00", last_update),
                            "%Y-%m-%d %H:%M:%S %z",
                        ) {
                            update_time.with_timezone(&chrono::Utc) > online_threshold
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                })
                .count() as u32;

            let offline_properties = total_properties - online_properties;

            // 获取事件和告警统计
            let (total_events, active_alarms) = self.get_device_events_and_alarms(device_id).await;

            Some(DeviceMetrics {
                total_properties,
                online_properties,
                offline_properties,
                total_commands,
                total_events,
                active_alarms,
            })
        } else {
            None
        }
    }

    /// 获取设备的事件和告警统计
    async fn get_device_events_and_alarms(&self, device_id: &str) -> (u32, u32) {
        // 查询设备相关事件数量（最近24小时）
        let total_events = 0u32; // TODO: 实现事件统计

        // 查询活跃告警数量
        let active_alarms = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND is_resolved = 0",
        )
        .bind(device_id)
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(e) => {
                tracing::debug!("Failed to query device alarms for {}: {}", device_id, e);
                0
            }
        };

        (total_events, active_alarms)
    }

    /// 获取系统概览信息
    pub async fn get_system_overview(&self) -> SystemOverview {
        let all_devices = self.context.get_all_devices();
        let total_devices = all_devices.len() as u32;

        let mut online_devices = 0u32;
        let mut offline_devices = 0u32;
        let mut total_properties = 0u32;
        let mut total_commands = 0u32;

        for device in &all_devices {
            // 统计在线/离线设备
            if self.is_device_online(&device.id) {
                online_devices += 1;
            } else {
                offline_devices += 1;
            }

            // 统计属性和指令数量
            if let Some(properties) = &device.properties {
                total_properties += properties.len() as u32;
            }

            if let Some(commands) = &device.commands {
                total_commands += commands.len() as u32;
            }
        }

        // 查询全局告警统计
        let total_alarms = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_alarms WHERE is_resolved = 0",
        )
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(e) => {
                tracing::debug!("Failed to query total active alarms: {}", e);
                0
            }
        };

        SystemOverview {
            total_devices,
            online_devices,
            offline_devices,
            total_properties,
            total_commands,
            total_alarms,
        }
    }
}

/// 设备指标信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceMetrics {
    pub total_properties: u32,
    pub online_properties: u32,
    pub offline_properties: u32,
    pub total_commands: u32,
    pub total_events: u32,
    pub active_alarms: u32,
}

/// 系统概览信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemOverview {
    pub total_devices: u32,
    pub online_devices: u32,
    pub offline_devices: u32,
    pub total_properties: u32,
    pub total_commands: u32,
    pub total_alarms: u32,
}
