use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use tinyiothub_core::models::device_command::DeviceCommand as DeviceCommandEntity;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entity_with_params(params: Option<String>) -> DeviceCommandEntity {
        DeviceCommandEntity {
            id: "cmd-1".to_string(),
            device_id: "dev-1".to_string(),
            name: "toggle".to_string(),
            display_name: Some("Toggle Switch".to_string()),
            description: Some("Toggle the device".to_string()),
            parameters: params,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_from_entity_with_valid_json_params() {
        let entity = test_entity_with_params(Some(
            r#"{"interval": 30, "unit": "seconds"}"#.to_string(),
        ));
        let response = DeviceCommandResponse::from(entity);

        assert_eq!(response.id, "cmd-1");
        assert_eq!(response.device_id, "dev-1");
        assert_eq!(response.name, "toggle");
        assert_eq!(response.display_name, Some("Toggle Switch".to_string()));
        assert_eq!(response.parameters.len(), 2);
        assert_eq!(response.parameters["interval"], 30);
        assert_eq!(response.parameters["unit"], "seconds");
    }

    #[test]
    fn test_from_entity_with_invalid_json_params() {
        let entity = test_entity_with_params(Some("not valid json".to_string()));
        let response = DeviceCommandResponse::from(entity);

        assert!(response.parameters.is_empty());
    }

    #[test]
    fn test_from_entity_with_none_params() {
        let entity = test_entity_with_params(None);
        let response = DeviceCommandResponse::from(entity);

        assert!(response.parameters.is_empty());
    }

    #[test]
    fn test_from_entities() {
        let entities = vec![
            test_entity_with_params(None),
            test_entity_with_params(Some(r#"{"key": "value"}"#.to_string())),
        ];
        let responses = DeviceCommandResponse::from_entities(entities);

        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].id, "cmd-1");
        assert!(responses[0].parameters.is_empty());
        assert_eq!(responses[1].parameters["key"], "value");
    }
}
