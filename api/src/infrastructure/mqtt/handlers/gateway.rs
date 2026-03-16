//! Gateway MQTT Message Handler
//! 处理网关相关的 MQTT 消息

use serde::{Deserialize, Serialize};
use crate::dto::entity::gateway::{Gateway, GatewayDevice, DeviceListReport, CreateGatewayRequest, UpdateGatewayRequest};
use crate::dto::entity::device::{Device, CreateDeviceRequest};

/// MQTT 消息通用格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttMessage {
    pub timestamp: String,
    pub message_id: String,
    pub payload: serde_json::Value,
}

/// 注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayRegisterPayload {
    pub api_key: String,
    pub gateway_name: String,
    pub gateway_type: Option<String>,
    pub firmware_version: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

/// 注册响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayRegisterResponse {
    pub success: bool,
    pub token: Option<String>,
    pub expires_at: Option<String>,
    pub gateway_id: Option<String>,
    pub error: Option<String>,
}

/// 网关状态上报
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayStatusPayload {
    pub status: String,
    pub uptime: Option<i64>,
    pub memory_usage: Option<i32>,
    pub cpu_usage: Option<i32>,
    pub wifi_signal: Option<i32>,
    pub connected_devices: Option<i32>,
}

/// 设备数据上报
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDataPayload {
    pub device_id: String,
    pub properties: Vec<PropertyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyValue {
    pub name: String,
    pub value: serde_json::Value,
    pub property_type: Option<String>,
    pub unit: Option<String>,
}

/// 处理网关注册
pub async fn handle_register(db: &crate::infrastructure::persistence::database::Database, payload: &GatewayRegisterPayload) -> GatewayRegisterResponse {
    // 1. 验证 API Key
    let gateway = match Gateway::find_by_api_key(db, &payload.api_key).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            return GatewayRegisterResponse {
                success: false,
                token: None,
                expires_at: None,
                gateway_id: None,
                error: Some("Invalid API key".to_string()),
            };
        }
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return GatewayRegisterResponse {
                success: false,
                token: None,
                expires_at: None,
                gateway_id: None,
                error: Some("Database error".to_string()),
            };
        }
    };

    // 2. 生成 Token
    let token = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    let expires_at_str = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // 3. 更新网关 Token
    if let Err(e) = Gateway::update_token(db, &gateway.id, &token, &expires_at_str).await {
        tracing::error!("Failed to update token: {}", e);
        return GatewayRegisterResponse {
            success: false,
            token: None,
            expires_at: None,
            gateway_id: None,
            error: Some("Failed to generate token".to_string()),
        };
    }

    // 4. 更新网关名称（如果提供）
    if let Some(name) = Some(&payload.gateway_name) {
        let _ = Gateway::update(db, &gateway.id, &crate::dto::entity::gateway::UpdateGatewayRequest {
            name: Some(name.clone()),
            status: None,
            gateway_type: None,
        }).await;
    }

    GatewayRegisterResponse {
        success: true,
        token: Some(token),
        expires_at: Some(expires_at_str),
        gateway_id: Some(gateway.id),
        error: None,
    }
}

/// 处理设备列表上报
pub async fn handle_device_list(
    db: &crate::infrastructure::persistence::database::Database,
    gateway_id: &str,
    payload: &DeviceListReport,
) -> Result<(), String> {
    // 1. 验证网关存在
    let gateway = Gateway::find_by_id(db, gateway_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Gateway not found")?;

    // 2. 更新网关在线状态
    let _ = Gateway::update_status(db, &gateway.id, "online").await;

    // 3. 处理每个设备
    for device_info in &payload.devices {
        // 查找或创建设备
        let existing_device = crate::dto::entity::device::Device::find_by_id(db, &device_info.device_id).await
            .map_err(|e| e.to_string())?;

        if existing_device.is_none() {
            // 创建新设备
            let create_req = CreateDeviceRequest {
                name: device_info.name.clone(),
                device_type: device_info.device_type.clone(),
                driver_name: device_info.protocol.clone(),
                description: Some(format!("From gateway {}", gateway.name)),
                metadata: device_info.properties.clone(),
                tags: None,
                parent_id: None,
            };

            // 创建设备（这里简化处理，实际需要调用创建方法）
            tracing::info!("Would create device: {}", device_info.device_id);
        }

        // 绑定设备到网关
        let _ = GatewayDevice::bind_device(db, &gateway.id, &device_info.device_id).await;

        // 更新设备属性
        if let Some(props) = &device_info.properties {
            for (key, value) in props.as_object().unwrap_or(&serde_json::Map::new()) {
                tracing::debug!("Device {} property {} = {}", device_info.device_id, key, value);
            }
        }
    }

    Ok(())
}

/// 处理设备数据上报
pub async fn handle_device_data(
    db: &crate::infrastructure::persistence::database::Database,
    gateway_id: &str,
    payload: &DeviceDataPayload,
) -> Result<(), String> {
    // 1. 验证网关
    let _ = Gateway::find_by_id(db, gateway_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Gateway not found")?;

    // 2. 更新网关在线状态
    let _ = Gateway::update_status(db, gateway_id, "online").await;

    // 3. 存储设备数据
    for prop in &payload.properties {
        let data_req = crate::dto::entity::device_data::CreateDeviceDataRequest {
            device_id: payload.device_id.clone(),
            property_name: prop.name.clone(),
            property_value: prop.value.to_string(),
            property_type: prop.property_type.clone(),
            unit: prop.unit.clone(),
            quality: Some("good".to_string()),
            timestamp: None,
        };

        if let Err(e) = crate::dto::entity::device_data::DeviceData::create(db, &data_req).await {
            tracing::warn!("Failed to save device data: {}", e);
        }
    }

    Ok(())
}

/// 处理网关状态上报
pub async fn handle_gateway_status(
    db: &crate::infrastructure::persistence::database::Database,
    gateway_id: &str,
    payload: &GatewayStatusPayload,
) -> Result<(), String> {
    let status = if payload.status == "online" { "online" } else { "offline" };
    
    Gateway::update_status(db, gateway_id, status)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
