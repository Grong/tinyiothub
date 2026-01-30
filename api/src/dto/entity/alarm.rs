use serde::{Deserialize, Serialize};

/// 报警 DTO
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

impl From<crate::domain::alarm::Alarm> for AlarmDto {
    fn from(alarm: crate::domain::alarm::Alarm) -> Self {
        Self {
            id: alarm.id,
            device_id: alarm.device_id,
            device_name: None,
            property_id: alarm.property_id,
            property_name: None,
            rule_id: alarm.rule_id,
            rule_name: None,
            alarm_type: alarm.alarm_type.as_str(),
            alarm_level: alarm.alarm_level.as_str().to_string(),
            message: alarm.message,
            alarm_value: alarm.alarm_value,
            threshold_value: alarm.threshold_value,
            alarm_time: alarm.alarm_time.to_rfc3339(),
            status: alarm.status.as_str().to_string(),
            is_acknowledged: alarm.acknowledgement.is_some(),
            acknowledged_by: alarm
                .acknowledgement
                .as_ref()
                .map(|a| a.acknowledged_by.clone()),
            acknowledged_at: alarm
                .acknowledgement
                .as_ref()
                .map(|a| a.acknowledged_at.to_rfc3339()),
            acknowledged_note: alarm.acknowledgement.as_ref().and_then(|a| a.note.clone()),
            is_resolved: alarm.resolution.is_some(),
            resolved_by: alarm.resolution.as_ref().map(|r| r.resolved_by.clone()),
            resolved_at: alarm
                .resolution
                .as_ref()
                .map(|r| r.resolved_at.to_rfc3339()),
            resolved_note: alarm.resolution.as_ref().and_then(|r| r.note.clone()),
            created_at: alarm.created_at.to_rfc3339(),
        }
    }
}

/// 报警规则 DTO
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

impl From<crate::domain::alarm::AlarmRule> for AlarmRuleDto {
    fn from(rule: crate::domain::alarm::AlarmRule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            device_id: rule.device_id,
            device_name: None,
            property_id: rule.property_id,
            property_name: None,
            rule_type: rule.rule_type.as_str().to_string(),
            condition: serde_json::to_value(&rule.condition).unwrap_or(serde_json::Value::Null),
            alarm_level: rule.alarm_level.as_str().to_string(),
            is_enabled: rule.is_enabled,
            notification_config: serde_json::to_value(&rule.notification_config)
                .unwrap_or(serde_json::Value::Null),
            created_at: rule.created_at.to_rfc3339(),
            updated_at: rule.updated_at.to_rfc3339(),
        }
    }
}

/// 报警统计 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmStatisticsDto {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}

impl From<crate::domain::alarm::AlarmStatistics> for AlarmStatisticsDto {
    fn from(stats: crate::domain::alarm::AlarmStatistics) -> Self {
        Self {
            total_count: stats.total_count,
            active_count: stats.active_count,
            acknowledged_count: stats.acknowledged_count,
            resolved_count: stats.resolved_count,
        }
    }
}
