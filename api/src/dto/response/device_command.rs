use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::dto::entity::device_command::DeviceCommand as DeviceCommandEntity;

/// 设备指令响应DTO - 用于API返回
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCommandResponse {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub parameters: HashMap<String, serde_json::Value>, // 解析后的参数对象
    pub created_at: String,
}

impl From<DeviceCommandEntity> for DeviceCommandResponse {
    fn from(entity: DeviceCommandEntity) -> Self {
        // 解析参数JSON字符串为对象
        let parameters = if let Some(params_str) = &entity.parameters {
            match serde_json::from_str::<HashMap<String, serde_json::Value>>(params_str) {
                Ok(params) => params,
                Err(e) => {
                    tracing::warn!("Failed to parse command parameters '{}': {}", params_str, e);
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        Self {
            id: entity.id,
            device_id: entity.device_id,
            name: entity.name,
            display_name: entity.display_name,
            description: entity.description,
            parameters,
            created_at: entity.created_at,
        }
    }
}

impl DeviceCommandResponse {
    /// 从实体列表转换为响应DTO列表
    pub fn from_entities(entities: Vec<DeviceCommandEntity>) -> Vec<Self> {
        entities.into_iter().map(Self::from).collect()
    }
}
