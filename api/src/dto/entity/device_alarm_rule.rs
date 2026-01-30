use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Device alarm rule entity - 设备告警规则实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmRule {
    pub id: String,
    pub device_id: String,
    pub property_id: String,
    pub rule_name: String,
    pub rule_type: String,   // "threshold", "range", "change"
    pub condition: String,   // JSON string with condition details
    pub alarm_level: String, // "info", "warning", "error", "critical"
    pub is_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub description: Option<String>,
}

/// Query parameters for device alarm rule search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmRuleQuery {
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub alarm_level: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new device alarm rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceAlarmRuleRequest {
    pub device_id: String,
    pub property_id: String,
    pub rule_name: String,
    pub rule_type: String,
    pub condition: String,
    pub alarm_level: String,
    pub is_enabled: Option<bool>,
    pub description: Option<String>,
}

/// Request for updating a device alarm rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceAlarmRuleRequest {
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub condition: Option<String>,
    pub alarm_level: Option<String>,
    pub is_enabled: Option<bool>,
    pub description: Option<String>,
}

impl DeviceAlarmRule {
    /// Create a new device alarm rule
    pub fn new(request: CreateDeviceAlarmRuleRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: request.device_id,
            property_id: request.property_id,
            rule_name: request.rule_name,
            rule_type: request.rule_type,
            condition: request.condition,
            alarm_level: request.alarm_level,
            is_enabled: request.is_enabled.unwrap_or(true),
            created_at: now.clone(),
            updated_at: now,
            description: request.description,
        }
    }

    /// Check if the alarm rule is active
    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    /// Get alarm level priority (higher number = higher priority)
    pub fn get_alarm_priority(&self) -> u8 {
        match self.alarm_level.as_str() {
            "info" => 1,
            "warning" => 2,
            "error" => 3,
            "critical" => 4,
            _ => 0,
        }
    }

    /// Validate the condition JSON
    pub fn validate_condition(&self) -> Result<(), String> {
        match serde_json::from_str::<serde_json::Value>(&self.condition) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Invalid condition JSON: {}", e)),
        }
    }
}

// Backward compatibility
pub type DeviceAlarmRuleDto = DeviceAlarmRule;
pub type DeviceAlarmRuleQueryParams = DeviceAlarmRuleQuery;
