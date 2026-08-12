// Alarm module types — entities, errors, value objects

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// ============================================================================
// Errors
// ============================================================================
use thiserror::Error;

/// 报警模块错误类型
#[derive(Error, Debug)]
pub enum AlarmError {
    #[error("报警未找到: {0}")]
    NotFound(String),

    #[error("报警规则未找到: {0}")]
    RuleNotFound(String),

    #[error("无效的报警状态转换: 从 {from} 到 {to}")]
    InvalidStatusTransition { from: String, to: String },

    #[error("报警已被确认")]
    AlreadyAcknowledged,

    #[error("报警已被解决")]
    AlreadyResolved,

    #[error("无效的报警条件: {0}")]
    InvalidCondition(String),

    #[error("无效的规则配置: {0}")]
    InvalidRuleConfig(String),

    #[error("数据库错误: {0}")]
    DatabaseError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("规则评估错误: {0}")]
    EvaluationError(String),

    #[error("权限不足")]
    PermissionDenied,

    #[error("内部错误: {0}")]
    InternalError(String),
}

impl From<sqlx::Error> for AlarmError {
    fn from(err: sqlx::Error) -> Self {
        AlarmError::DatabaseError(err.to_string())
    }
}

impl From<tinyiothub_storage::DbError> for AlarmError {
    fn from(err: tinyiothub_storage::DbError) -> Self {
        AlarmError::DatabaseError(err.to_string())
    }
}

impl From<serde_json::Error> for AlarmError {
    fn from(err: serde_json::Error) -> Self {
        AlarmError::SerializationError(err.to_string())
    }
}

pub type AlarmResult<T> = Result<T, AlarmError>;

// Persisted row types live in the db crate (E2 集中化); re-exported for compatibility.
pub use tinyiothub_storage::alarm::*;

// ============================================================================
// Value Objects
// ============================================================================

/// 报警 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

impl From<crate::domains::alarm::Alarm> for AlarmDto {
    fn from(alarm: crate::domains::alarm::Alarm) -> Self {
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
            acknowledged_by: alarm.acknowledgement.as_ref().map(|a| a.acknowledged_by.clone()),
            acknowledged_at: alarm.acknowledgement.as_ref().map(|a| a.acknowledged_at.to_rfc3339()),
            acknowledged_note: alarm.acknowledgement.as_ref().and_then(|a| a.note.clone()),
            is_resolved: alarm.resolution.is_some(),
            resolved_by: alarm.resolution.as_ref().map(|r| r.resolved_by.clone()),
            resolved_at: alarm.resolution.as_ref().map(|r| r.resolved_at.to_rfc3339()),
            resolved_note: alarm.resolution.as_ref().and_then(|r| r.note.clone()),
            created_at: alarm.created_at.to_rfc3339(),
        }
    }
}

/// 报警规则 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

impl From<crate::domains::alarm::AlarmRule> for AlarmRuleDto {
    fn from(rule: crate::domains::alarm::AlarmRule) -> Self {
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
            notification_config: serde_json::to_value(&rule.notification_config).unwrap_or(serde_json::Value::Null),
            created_at: rule.created_at.to_rfc3339(),
            updated_at: rule.updated_at.to_rfc3339(),
        }
    }
}

/// 报警统计 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmStatisticsDto {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}

impl From<crate::domains::alarm::AlarmStatistics> for AlarmStatisticsDto {
    fn from(stats: crate::domains::alarm::AlarmStatistics) -> Self {
        Self {
            total_count: stats.total_count,
            active_count: stats.active_count,
            acknowledged_count: stats.acknowledged_count,
            resolved_count: stats.resolved_count,
        }
    }
}

/// 确认报警请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgeAlarmRequest {
    pub note: Option<String>,
}

/// 解决报警请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAlarmRequest {
    pub resolution_type: String,
    pub note: Option<String>,
}

/// 批量确认请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAcknowledgeRequest {
    pub alarm_ids: Vec<String>,
}

/// 批量解决请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResolveRequest {
    pub alarm_ids: Vec<String>,
    pub resolution_type: String,
}

fn deser_opt_csv<'de, D>(d: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(d)?;
    Ok(raw.map(|s| {
        s.split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect()
    }))
}

/// 报警查询参数
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AlarmQueryParams {
    #[serde(default, deserialize_with = "deser_opt_csv")]
    pub device_ids: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deser_opt_csv")]
    pub levels: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deser_opt_csv")]
    pub statuses: Option<Vec<String>>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建报警规则请求
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAlarmRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_type: tinyiothub_storage::alarm::RuleType,
    pub condition: serde_json::Value,
    pub alarm_level: String,
    pub notification_config: serde_json::Value,
}

/// 更新报警规则请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAlarmRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub property_id: Option<String>,
    pub condition: Option<serde_json::Value>,
    pub alarm_level: Option<String>,
    pub notification_config: Option<serde_json::Value>,
}

/// 切换规则状态请求
#[derive(Debug, Clone, Deserialize)]
pub struct ToggleRuleRequest {
    pub enabled: bool,
}

/// 统计查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct StatisticsQueryParams {
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    pub success_count: usize,
    pub total_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_alarm_level_as_str() {
        assert_eq!(AlarmLevel::Info.as_str(), "info");
        assert_eq!(AlarmLevel::Warning.as_str(), "warning");
        assert_eq!(AlarmLevel::Error.as_str(), "error");
        assert_eq!(AlarmLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn test_alarm_level_parse_str() {
        assert_eq!(AlarmLevel::parse_str("info"), Some(AlarmLevel::Info));
        assert_eq!(AlarmLevel::parse_str("warning"), Some(AlarmLevel::Warning));
        assert_eq!(AlarmLevel::parse_str("error"), Some(AlarmLevel::Error));
        assert_eq!(AlarmLevel::parse_str("critical"), Some(AlarmLevel::Critical));
        assert_eq!(AlarmLevel::parse_str("unknown"), None);
    }

    #[test]
    fn test_alarm_level_priority() {
        assert_eq!(AlarmLevel::Info.priority(), 1);
        assert_eq!(AlarmLevel::Warning.priority(), 2);
        assert_eq!(AlarmLevel::Error.priority(), 3);
        assert_eq!(AlarmLevel::Critical.priority(), 4);
    }

    #[test]
    fn test_alarm_level_display() {
        assert_eq!(format!("{}", AlarmLevel::Warning), "warning");
    }

    #[test]
    fn test_alarm_status_as_str() {
        assert_eq!(AlarmStatus::Active.as_str(), "active");
        assert_eq!(AlarmStatus::Acknowledged.as_str(), "acknowledged");
        assert_eq!(AlarmStatus::Resolved.as_str(), "resolved");
        assert_eq!(AlarmStatus::Suppressed.as_str(), "suppressed");
    }

    #[test]
    fn test_alarm_status_parse_str() {
        assert_eq!(AlarmStatus::parse_str("active"), Some(AlarmStatus::Active));
        assert_eq!(AlarmStatus::parse_str("acknowledged"), Some(AlarmStatus::Acknowledged));
        assert_eq!(AlarmStatus::parse_str("resolved"), Some(AlarmStatus::Resolved));
        assert_eq!(AlarmStatus::parse_str("suppressed"), Some(AlarmStatus::Suppressed));
        assert_eq!(AlarmStatus::parse_str("unknown"), None);
    }

    #[test]
    fn test_alarm_status_is_active() {
        assert!(AlarmStatus::Active.is_active());
        assert!(AlarmStatus::Acknowledged.is_active());
        assert!(!AlarmStatus::Resolved.is_active());
        assert!(!AlarmStatus::Suppressed.is_active());
    }

    #[test]
    fn test_alarm_status_is_resolved() {
        assert!(AlarmStatus::Resolved.is_resolved());
        assert!(!AlarmStatus::Active.is_resolved());
        assert!(!AlarmStatus::Acknowledged.is_resolved());
        assert!(!AlarmStatus::Suppressed.is_resolved());
    }

    #[test]
    fn test_alarm_type_as_str() {
        assert_eq!(AlarmType::DeviceOffline.as_str(), "device_offline");
        assert_eq!(AlarmType::DeviceError.as_str(), "device_error");
        assert_eq!(AlarmType::PropertyThreshold.as_str(), "property_threshold");
        assert_eq!(AlarmType::PropertyAnomaly.as_str(), "property_anomaly");
        assert_eq!(AlarmType::CommandFailed.as_str(), "command_failed");
        assert_eq!(
            AlarmType::Custom {
                name: "special".to_string()
            }
            .as_str(),
            "custom_special"
        );
    }

    #[test]
    fn test_alarm_type_parse_str() {
        assert_eq!(AlarmType::parse_str("device_offline"), AlarmType::DeviceOffline);
        assert_eq!(AlarmType::parse_str("device_error"), AlarmType::DeviceError);
        assert_eq!(AlarmType::parse_str("property_threshold"), AlarmType::PropertyThreshold);
        assert_eq!(AlarmType::parse_str("property_anomaly"), AlarmType::PropertyAnomaly);
        assert_eq!(AlarmType::parse_str("command_failed"), AlarmType::CommandFailed);
        assert_eq!(
            AlarmType::parse_str("custom_foo"),
            AlarmType::Custom {
                name: "foo".to_string()
            }
        );
        assert_eq!(
            AlarmType::parse_str("other"),
            AlarmType::Custom {
                name: "other".to_string()
            }
        );
    }

    #[test]
    fn test_comparison_operator_evaluate() {
        assert!(ComparisonOperator::GreaterThan.evaluate(5.0, 3.0));
        assert!(!ComparisonOperator::GreaterThan.evaluate(3.0, 5.0));

        assert!(ComparisonOperator::LessThan.evaluate(2.0, 5.0));
        assert!(!ComparisonOperator::LessThan.evaluate(5.0, 2.0));

        assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(5.0, 5.0));
        assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(6.0, 5.0));

        assert!(ComparisonOperator::LessThanOrEqual.evaluate(3.0, 5.0));
        assert!(ComparisonOperator::LessThanOrEqual.evaluate(5.0, 5.0));

        assert!(ComparisonOperator::Equal.evaluate(1.0, 1.0));
        assert!(!ComparisonOperator::Equal.evaluate(1.0, 2.0));

        assert!(ComparisonOperator::NotEqual.evaluate(1.0, 2.0));
        assert!(!ComparisonOperator::NotEqual.evaluate(1.0, 1.0));
    }

    #[test]
    fn test_resolution_type_as_str() {
        assert_eq!(ResolutionType::Fixed.as_str(), "fixed");
        assert_eq!(ResolutionType::FalseAlarm.as_str(), "false_alarm");
        assert_eq!(ResolutionType::Ignored.as_str(), "ignored");
        assert_eq!(ResolutionType::AutoResolved.as_str(), "auto_resolved");
    }

    #[test]
    fn test_acknowledgement_new() {
        let ack = Acknowledgement::new("user-1".to_string(), Some("noted".to_string()));
        assert_eq!(ack.acknowledged_by, "user-1");
        assert_eq!(ack.note, Some("noted".to_string()));
    }

    #[test]
    fn test_resolution_new() {
        let res = Resolution::new("user-1".to_string(), ResolutionType::Fixed, None);
        assert_eq!(res.resolved_by, "user-1");
        assert_eq!(res.resolution_type, ResolutionType::Fixed);
        assert!(res.note.is_none());
    }

    #[test]
    fn test_alarm_new() {
        let alarm = Alarm::new(
            "device-1".to_string(),
            Some("prop-1".to_string()),
            Some("rule-1".to_string()),
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "Device went offline".to_string(),
            None,
            None,
            None,
        );
        assert!(!alarm.id.is_empty());
        assert_eq!(alarm.device_id, "device-1");
        assert_eq!(alarm.status, AlarmStatus::Active);
        assert!(alarm.acknowledgement.is_none());
        assert!(alarm.resolution.is_none());
    }

    #[test]
    fn test_alarm_acknowledge_success() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(
            alarm
                .acknowledge("user-1".to_string(), Some("ack note".to_string()))
                .is_ok()
        );
        assert_eq!(alarm.status, AlarmStatus::Acknowledged);
        assert!(alarm.acknowledgement.is_some());
    }

    #[test]
    fn test_alarm_acknowledge_already_acknowledged() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        let result = alarm.acknowledge("user-2".to_string(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_alarm_resolve_success() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None).is_ok());
        assert_eq!(alarm.status, AlarmStatus::Resolved);
    }

    #[test]
    fn test_alarm_resolve_after_acknowledge() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        assert!(alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None).is_ok());
    }

    #[test]
    fn test_alarm_resolve_already_resolved_fails() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        alarm
            .resolve("user-1".to_string(), ResolutionType::Fixed, None)
            .unwrap();
        let result = alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_alarm_suppress_success() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(alarm.suppress().is_ok());
        assert_eq!(alarm.status, AlarmStatus::Suppressed);
    }

    #[test]
    fn test_alarm_suppress_non_active_fails() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        assert!(alarm.suppress().is_err());
    }

    #[test]
    fn test_alarm_can_acknowledge() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(alarm.can_acknowledge());
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        assert!(!alarm.can_acknowledge());
    }

    #[test]
    fn test_alarm_can_resolve() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(alarm.can_resolve());
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        assert!(alarm.can_resolve());
        alarm
            .resolve("user-1".to_string(), ResolutionType::Fixed, None)
            .unwrap();
        assert!(!alarm.can_resolve());
    }

    #[test]
    fn test_alarm_is_active() {
        let mut alarm = Alarm::new(
            "device-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "msg".to_string(),
            None,
            None,
            None,
        );
        assert!(alarm.is_active());
        alarm.acknowledge("user-1".to_string(), None).unwrap();
        assert!(alarm.is_active());
        alarm
            .resolve("user-1".to_string(), ResolutionType::Fixed, None)
            .unwrap();
        assert!(!alarm.is_active());
    }

    #[test]
    fn test_alarm_rule_new_success() {
        let config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            trigger_duration_secs: None,
            recovery_duration_secs: None,
            repeat_interval: None,
        };
        let rule = AlarmRule::new(
            "Test Rule".to_string(),
            None,
            Some("device-1".to_string()),
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        );
        assert!(rule.is_ok());
        let rule = rule.unwrap();
        assert_eq!(rule.name, "Test Rule");
        assert!(rule.is_enabled);
    }

    #[test]
    fn test_alarm_rule_new_empty_name_fails() {
        let config = NotificationConfig::default();
        let rule = AlarmRule::new(
            "".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        );
        assert!(rule.is_err());
    }

    #[test]
    fn test_alarm_rule_new_enabled_notification_no_channels_fails() {
        let config = NotificationConfig {
            enabled: true,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            trigger_duration_secs: None,
            recovery_duration_secs: None,
            repeat_interval: None,
        };
        let rule = AlarmRule::new(
            "Test".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        );
        assert!(rule.is_err());
    }

    #[test]
    fn test_alarm_rule_update_name() {
        let config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            trigger_duration_secs: None,
            recovery_duration_secs: None,
            repeat_interval: None,
        };
        let mut rule = AlarmRule::new(
            "Old Name".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        )
        .unwrap();

        let result = rule.update(Some("New Name".to_string()), None, None, None, None, None);
        assert!(result.is_ok());
        assert_eq!(rule.name, "New Name");
    }

    #[test]
    fn test_alarm_rule_update_empty_name_fails() {
        let config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            trigger_duration_secs: None,
            recovery_duration_secs: None,
            repeat_interval: None,
        };
        let mut rule = AlarmRule::new(
            "Name".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        )
        .unwrap();

        let result = rule.update(Some("".to_string()), None, None, None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_alarm_rule_enable_disable() {
        let config = NotificationConfig {
            enabled: false,
            channels: vec![],
            recipients: vec![],
            suppress_duration: None,
            trigger_duration_secs: None,
            recovery_duration_secs: None,
            repeat_interval: None,
        };
        let mut rule = AlarmRule::new(
            "Name".to_string(),
            None,
            None,
            None,
            RuleType::Threshold,
            AlarmCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 50.0,
                recovery_threshold: None,
            },
            AlarmLevel::Warning,
            config,
            "ws-1".to_string(),
        )
        .unwrap();

        rule.disable();
        assert!(!rule.is_enabled);
        rule.enable();
        assert!(rule.is_enabled);
    }

    #[test]
    fn test_alarm_dto_from_alarm() {
        let alarm = Alarm::new(
            "device-1".to_string(),
            Some("prop-1".to_string()),
            Some("rule-1".to_string()),
            AlarmType::DeviceOffline,
            AlarmLevel::Warning,
            "offline".to_string(),
            Some("0".to_string()),
            Some("1".to_string()),
            None,
        );
        let dto = AlarmDto::from(alarm.clone());
        assert_eq!(dto.device_id, "device-1");
        assert_eq!(dto.alarm_type, "device_offline");
        assert_eq!(dto.alarm_level, "warning");
        assert_eq!(dto.status, "active");
        assert!(!dto.is_acknowledged);
        assert!(!dto.is_resolved);
    }

    #[test]
    fn test_alarm_statistics_dto_from() {
        use crate::domains::alarm::AlarmStatistics;
        let stats = AlarmStatistics {
            total_count: 10,
            active_count: 3,
            acknowledged_count: 2,
            resolved_count: 5,
        };
        let dto = AlarmStatisticsDto::from(stats);
        assert_eq!(dto.total_count, 10);
        assert_eq!(dto.active_count, 3);
        assert_eq!(dto.acknowledged_count, 2);
        assert_eq!(dto.resolved_count, 5);
    }

    #[test]
    fn test_threshold_condition_deser_recovery_threshold() {
        let json = r#"{"type":"threshold","operator":"greater_than","value":80.0,"recovery_threshold":75.0}"#;
        let condition: AlarmCondition = serde_json::from_str(json).unwrap();
        match condition {
            AlarmCondition::Threshold {
                operator,
                value,
                recovery_threshold,
            } => {
                assert_eq!(operator, ComparisonOperator::GreaterThan);
                assert!((value - 80.0).abs() < f64::EPSILON);
                assert_eq!(recovery_threshold, Some(75.0));
            }
            _ => panic!("Expected Threshold condition"),
        }
    }

    #[test]
    fn test_threshold_condition_deser_no_recovery_threshold() {
        // Backward compat: old JSON without recovery_threshold should deserialize
        let json = r#"{"type":"threshold","operator":"greater_than","value":80.0}"#;
        let condition: AlarmCondition = serde_json::from_str(json).unwrap();
        match condition {
            AlarmCondition::Threshold { recovery_threshold, .. } => {
                assert_eq!(recovery_threshold, None);
            }
            _ => panic!("Expected Threshold condition"),
        }
    }

    #[test]
    fn test_notification_config_deser_new_duration_fields() {
        let json =
            r#"{"enabled":false,"channels":[],"recipients":[],"trigger_duration_secs":30,"recovery_duration_secs":60}"#;
        let config: NotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trigger_duration_secs, Some(Duration::from_secs(30)));
        assert_eq!(config.recovery_duration_secs, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_notification_config_deser_no_new_duration_fields() {
        // Backward compat: old JSON without recovery_duration_secs
        // deserializes with the default 30s recovery debounce.
        let json = r#"{"enabled":false,"channels":[],"recipients":[]}"#;
        let config: NotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trigger_duration_secs, None);
        assert_eq!(config.recovery_duration_secs, Some(std::time::Duration::from_secs(30)));
    }

    #[test]
    fn test_notification_config_default_has_none_duration_fields() {
        let config = NotificationConfig::default();
        assert_eq!(config.trigger_duration_secs, None);
        // recovery_duration_secs defaults to 30s to prevent single-tick auto-resolve
        assert_eq!(config.recovery_duration_secs, Some(std::time::Duration::from_secs(30)));
    }
}

/// 最新告警信息 (moved from `cloud::modules::monitoring::types` in P4-Task19 —
/// the alarm `/recent` HTTP handler was its only consumer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentAlarm {
    /// 告警ID
    pub id: String,
    /// 设备ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 告警级别
    pub level: String,
    /// 告警消息
    pub message: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 告警状态
    pub status: String,
}
