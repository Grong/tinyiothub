use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::{
    domain::event::{
        repositories::{EventCriteria, SortBy, SortOrder},
        value_objects::EventType,
    },
    dto::{
        entity::{device::Device, device_property::DeviceProperty},
        response::{builder::ApiResponseBuilder, ApiResponse, DeviceCommandResponse},
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

/// 设备完整配置文件
#[derive(Debug, Serialize)]
pub struct DeviceProfile {
    /// 设备基本信息
    pub device: Device,
    /// 设备在线状态
    pub is_online: bool,
    /// 设备属性列表
    pub properties: Vec<DeviceProperty>,
    /// 设备指令列表
    pub commands: Vec<DeviceCommandResponse>,
    /// 设备最近事件列表（最近 10 条）
    pub recent_events: Vec<DeviceEventSummary>,
    /// 设备概述信息
    pub overview: DeviceProfileOverview,
    /// 配置文件生成时间
    pub generated_at: String,
}

/// 设备事件摘要
#[derive(Debug, Serialize)]
pub struct DeviceEventSummary {
    /// 事件 ID
    pub id: String,
    /// 事件类型（如 "Connection", "Property", "Command"）
    pub event_type: String,
    /// 事件级别（如 "Info", "Warning", "Error", "Critical"）
    pub level: String,
    /// 事件标题
    pub title: String,
    /// 事件简要描述
    pub message: String,
    /// 事件时间
    pub timestamp: String,
    /// 额外的元数据（如命令状态、属性变化等）
    pub metadata: Option<serde_json::Value>,
}

/// 设备配置文件概述信息
#[derive(Debug, Serialize)]
pub struct DeviceProfileOverview {
    /// 属性总数
    pub total_properties: u32,
    /// 在线属性数
    pub online_properties: u32,
    /// 离线属性数
    pub offline_properties: u32,
    /// 只读属性数
    pub readonly_properties: u32,
    /// 可写属性数
    pub writable_properties: u32,
    /// 指令总数
    pub total_commands: u32,
    /// 事件总数（最近 24 小时）
    pub recent_event_count: u32,
    /// 严重事件数（最近 24 小时）
    pub critical_event_count: u32,
    /// 错误事件数（最近 24 小时）
    pub error_event_count: u32,
    /// 最后事件时间
    pub last_event_time: Option<String>,
    /// 最后更新时间
    pub updated_at: Option<String>,
}

pub fn create_router() -> Router<AppState> {
    Router::new().route("/{id}/profile", get(get_device_profile))
}

/// 获取设备完整配置文件
async fn get_device_profile(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<DeviceProfile>> {
    tracing::debug!("Getting complete profile for device: {}", device_id);

    // 从DataContext缓存获取设备信息（包含实时数据）
    let mut device = match state.get_device(&device_id) {
        Some(device) => device,
        None => {
            // 如果内存中没有，尝试从数据库加载并加入缓存
            match state.device_service.get_device_by_id(&device_id).await {
                Ok(Some(mut device)) => {
                    // 加载设备属性和指令
                    match state.device_service.get_device_properties(&device_id).await {
                        Ok(properties) => device.properties = Some(properties),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load properties for device {}: {}",
                                device_id,
                                e
                            );
                            device.properties = Some(Vec::new());
                        }
                    }

                    match state.device_service.get_device_commands(&device_id).await {
                        Ok(commands) => device.commands = Some(commands),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load commands for device {}: {}",
                                device_id,
                                e
                            );
                            device.commands = Some(Vec::new());
                        }
                    }

                    // 初始化运行时状态
                    device.is_online = false;
                    device.last_heartbeat = None;

                    // 将设备加载到内存缓存中
                    state.set_device(device.clone());
                    device
                }
                Ok(None) => {
                    return ApiResponseBuilder::error("设备不存在");
                }
                Err(e) => {
                    tracing::error!("Failed to find device {}: {}", device_id, e);
                    return ApiResponseBuilder::error("查询设备失败");
                }
            }
        }
    };

    // 加载设备标签
    match state.tag_service.find_tags_by_target_id(&device_id).await {
        Ok(tags) => device.tags = Some(tags),
        Err(e) => tracing::warn!("Failed to load tags for device {}: {}", device_id, e),
    }

    // 从缓存的设备对象中获取属性和指令（已包含实时数据）
    let properties = device.properties.clone().unwrap_or_default();
    let commands = device.commands.clone().unwrap_or_default();
    let commands_response = DeviceCommandResponse::from_entities(commands);

    // 查询设备最近的 10 条事件
    let recent_events = fetch_recent_device_events(&state, &device_id).await;

    // 计算概述信息（包含事件统计）
    let overview = calculate_device_overview(
        &state,
        &device_id,
        &properties,
        &commands_response,
        &recent_events,
    )
    .await;

    // 设备在线状态直接从缓存获取
    let is_online = device.is_online;

    // 生成配置文件
    let profile = DeviceProfile {
        device,
        is_online,
        properties,
        commands: commands_response,
        recent_events,
        overview,
        generated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    ApiResponseBuilder::success(profile)
}

/// 获取设备最近的事件
async fn fetch_recent_device_events(state: &AppState, device_id: &str) -> Vec<DeviceEventSummary> {
    // 构建查询条件：查询该设备最近 10 条事件
    let criteria = EventCriteria::builder()
        .device_ids(vec![device_id.to_string()])
        .sort_by(SortBy::Timestamp)
        .sort_order(SortOrder::Descending)
        .limit(10)
        .build();

    // 查询事件
    match state.event_repository.find_by_criteria(&criteria).await {
        Ok(events) => {
            events
                .into_iter()
                .map(|event| {
                    // 提取事件类型字符串
                    let event_type_str = match event.event_type() {
                        EventType::Device(device_type) => device_type.display_name(),
                        EventType::System(_) => "System",
                    };

                    // 提取事件级别字符串
                    let level_str = match event.level() {
                        crate::domain::event::value_objects::EventLevel::Debug => "Debug",
                        crate::domain::event::value_objects::EventLevel::Info => "Info",
                        crate::domain::event::value_objects::EventLevel::Warning => "Warning",
                        crate::domain::event::value_objects::EventLevel::Error => "Error",
                        crate::domain::event::value_objects::EventLevel::Critical => "Critical",
                    };

                    // 提取内容
                    let content = event.content();
                    let title = content.title().to_string();

                    // 生成简要描述（从内容元素中提取第一个文本）
                    let message = content
                        .elements()
                        .iter()
                        .find_map(|element| {
                            if let crate::domain::event::value_objects::ContentElement::Text {
                                content,
                                ..
                            } = element
                            {
                                Some(content.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| title.clone());

                    // 提取元数据
                    let metadata = if content.metadata().is_empty() {
                        None
                    } else {
                        Some(
                            serde_json::to_value(content.metadata())
                                .unwrap_or(serde_json::Value::Null),
                        )
                    };

                    DeviceEventSummary {
                        id: event.id().to_string(),
                        event_type: event_type_str.to_string(),
                        level: level_str.to_string(),
                        title,
                        message,
                        timestamp: event.timestamp().format("%Y-%m-%d %H:%M:%S").to_string(),
                        metadata,
                    }
                })
                .collect()
        }
        Err(e) => {
            tracing::warn!("Failed to fetch events for device {}: {}", device_id, e);
            Vec::new()
        }
    }
}

/// 计算设备概述信息
async fn calculate_device_overview(
    state: &AppState,
    device_id: &str,
    properties: &[DeviceProperty],
    commands: &[DeviceCommandResponse],
    recent_events: &[DeviceEventSummary],
) -> DeviceProfileOverview {
    let total_properties = properties.len() as u32;

    // 计算只读属性数量
    let readonly_properties = properties.iter().filter(|p| p.is_read_only == 1).count() as u32;
    let writable_properties = total_properties - readonly_properties;

    // 计算在线属性数量（基于最后更新时间）
    let now = chrono::Utc::now();
    let online_threshold = now - chrono::Duration::minutes(5); // 5分钟内更新的认为是在线

    let online_properties = properties
        .iter()
        .filter(|p| {
            if let Some(last_update) = &p.updated_at {
                // 尝试解析时间字符串
                if let Ok(update_time) = chrono::DateTime::parse_from_str(
                    &format!("{} +00:00", last_update),
                    "%Y-%m-%d %H:%M:%S %z",
                ) {
                    update_time.with_timezone(&chrono::Utc) > online_threshold
                } else {
                    false // 如果时间解析失败，认为是离线
                }
            } else {
                false // 没有更新时间的认为是离线
            }
        })
        .count() as u32;

    let offline_properties = total_properties - online_properties;

    let total_commands = commands.len() as u32;

    // 获取最后更新时间（所有属性中最新的）
    let updated_at = properties.iter().filter_map(|p| p.updated_at.as_ref()).max().cloned();

    // 查询最近 24 小时的事件统计
    let twenty_four_hours_ago = now - chrono::Duration::hours(24);
    let criteria = EventCriteria::builder()
        .device_ids(vec![device_id.to_string()])
        .start_time(twenty_four_hours_ago)
        .build();

    let (recent_event_count, critical_event_count, error_event_count) = match state
        .event_repository
        .find_by_criteria(&criteria)
        .await
    {
        Ok(events) => {
            let total = events.len() as u32;
            let critical = events
                .iter()
                .filter(|e| {
                    matches!(e.level(), crate::domain::event::value_objects::EventLevel::Critical)
                })
                .count() as u32;
            let error = events
                .iter()
                .filter(|e| {
                    matches!(e.level(), crate::domain::event::value_objects::EventLevel::Error)
                })
                .count() as u32;
            (total, critical, error)
        }
        Err(e) => {
            tracing::warn!("Failed to fetch event statistics for device {}: {}", device_id, e);
            (0, 0, 0)
        }
    };

    // 获取最后事件时间
    let last_event_time = recent_events.first().map(|e| e.timestamp.clone());

    DeviceProfileOverview {
        total_properties,
        online_properties,
        offline_properties,
        readonly_properties,
        writable_properties,
        total_commands,
        recent_event_count,
        critical_event_count,
        error_event_count,
        last_event_time,
        updated_at,
    }
}
