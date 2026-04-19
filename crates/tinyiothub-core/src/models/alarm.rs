use serde::{Deserialize, Serialize};

/// Alarm DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmDto {
    pub id: String,
    pub device_id: String,
    pub device_name: Option<String>,
    pub property_id: Option<String>,
    pub property_name: Option<String>,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub alarm_type: String,
    pub alarm_level: String,
    pub message: String,
    pub alarm_value: Option<String>,
    pub threshold_value: Option<String>,
    pub alarm_time: String,
    pub status: String,
    pub is_acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<String>,
    pub acknowledged_note: Option<String>,
    pub is_resolved: bool,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub resolved_note: Option<String>,
    pub created_at: String,
}

/// Alarm rule DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRuleDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub property_id: Option<String>,
    pub property_name: Option<String>,
    pub rule_type: String,
    pub condition: serde_json::Value,
    pub alarm_level: String,
    pub is_enabled: bool,
    pub notification_config: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Alarm statistics DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmStatisticsDto {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}
