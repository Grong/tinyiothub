// Alarm module types — entities, errors, value objects

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// ============================================================================
// Errors
// ============================================================================
use thiserror::Error;

use crate::modules::event::aggregates::NotificationChannelType;

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

impl From<serde_json::Error> for AlarmError {
    fn from(err: serde_json::Error) -> Self {
        AlarmError::SerializationError(err.to_string())
    }
}

pub type AlarmResult<T> = Result<T, AlarmError>;

// ============================================================================
// Value Objects
// ============================================================================

/// 报警级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlarmLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlarmLevel::Info => "info",
            AlarmLevel::Warning => "warning",
            AlarmLevel::Error => "error",
            AlarmLevel::Critical => "critical",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(AlarmLevel::Info),
            "warning" => Some(AlarmLevel::Warning),
            "error" => Some(AlarmLevel::Error),
            "critical" => Some(AlarmLevel::Critical),
            _ => None,
        }
    }

    pub fn to_event_level(&self) -> crate::modules::event::value_objects::EventLevel {
        match self {
            AlarmLevel::Info => crate::modules::event::value_objects::EventLevel::Info,
            AlarmLevel::Warning => crate::modules::event::value_objects::EventLevel::Warning,
            AlarmLevel::Error => crate::modules::event::value_objects::EventLevel::Error,
            AlarmLevel::Critical => crate::modules::event::value_objects::EventLevel::Critical,
        }
    }

    pub fn from_event_level(level: &crate::modules::event::value_objects::EventLevel) -> Self {
        match level {
            crate::modules::event::value_objects::EventLevel::Debug => AlarmLevel::Info,
            crate::modules::event::value_objects::EventLevel::Info => AlarmLevel::Info,
            crate::modules::event::value_objects::EventLevel::Warning => AlarmLevel::Warning,
            crate::modules::event::value_objects::EventLevel::Error => AlarmLevel::Error,
            crate::modules::event::value_objects::EventLevel::Critical => AlarmLevel::Critical,
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            AlarmLevel::Info => 1,
            AlarmLevel::Warning => 2,
            AlarmLevel::Error => 3,
            AlarmLevel::Critical => 4,
        }
    }
}

impl std::fmt::Display for AlarmLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 报警状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmStatus {
    Active,
    Acknowledged,
    Resolved,
    Suppressed,
}

impl AlarmStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlarmStatus::Active => "active",
            AlarmStatus::Acknowledged => "acknowledged",
            AlarmStatus::Resolved => "resolved",
            AlarmStatus::Suppressed => "suppressed",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "active" => Some(AlarmStatus::Active),
            "acknowledged" => Some(AlarmStatus::Acknowledged),
            "resolved" => Some(AlarmStatus::Resolved),
            "suppressed" => Some(AlarmStatus::Suppressed),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, AlarmStatus::Resolved)
    }
}

impl std::fmt::Display for AlarmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 报警类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmType {
    DeviceOffline,
    DeviceError,
    PropertyThreshold,
    PropertyAnomaly,
    CommandFailed,
    Custom { name: String },
}

impl AlarmType {
    pub fn as_str(&self) -> String {
        match self {
            AlarmType::DeviceOffline => "device_offline".to_string(),
            AlarmType::DeviceError => "device_error".to_string(),
            AlarmType::PropertyThreshold => "property_threshold".to_string(),
            AlarmType::PropertyAnomaly => "property_anomaly".to_string(),
            AlarmType::CommandFailed => "command_failed".to_string(),
            AlarmType::Custom { name } => format!("custom_{}", name),
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "device_offline" => AlarmType::DeviceOffline,
            "device_error" => AlarmType::DeviceError,
            "property_threshold" => AlarmType::PropertyThreshold,
            "property_anomaly" => AlarmType::PropertyAnomaly,
            "command_failed" => AlarmType::CommandFailed,
            s if s.starts_with("custom_") => {
                AlarmType::Custom { name: s.strip_prefix("custom_").unwrap_or(s).to_string() }
            }
            _ => AlarmType::Custom { name: s.to_string() },
        }
    }
}

impl std::fmt::Display for AlarmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 报警条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmCondition {
    Threshold {
        operator: ComparisonOperator,
        value: f64,
        /// 恢复阈值（迟滞）。当设置了该值时，恢复条件使用此阈值而非原始阈值。
        /// 例如：触发条件 `> 80`，恢复阈值 `75`，恢复需要值 `< 75`。
        #[serde(default)]
        recovery_threshold: Option<f64>,
    },
    Range {
        min: Option<f64>,
        max: Option<f64>,
        inclusive: bool,
    },
    Change {
        change_type: ChangeType,
        threshold: f64,
        #[serde(with = "duration_serde")]
        time_window: Duration,
    },
    Duration {
        condition: Box<AlarmCondition>,
        #[serde(with = "duration_serde")]
        duration: Duration,
    },
    Composite {
        operator: LogicalOperator,
        conditions: Vec<AlarmCondition>,
    },
}

/// 比较运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

impl ComparisonOperator {
    pub fn evaluate(&self, left: f64, right: f64) -> bool {
        match self {
            ComparisonOperator::GreaterThan => left > right,
            ComparisonOperator::LessThan => left < right,
            ComparisonOperator::GreaterThanOrEqual => left >= right,
            ComparisonOperator::LessThanOrEqual => left <= right,
            ComparisonOperator::Equal => (left - right).abs() < f64::EPSILON,
            ComparisonOperator::NotEqual => (left - right).abs() >= f64::EPSILON,
        }
    }
}

/// 变化类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Increase,
    Decrease,
    Any,
}

/// 逻辑运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}

/// 确认信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acknowledgement {
    pub acknowledged_by: String,
    pub acknowledged_at: DateTime<Utc>,
    pub note: Option<String>,
}

impl Acknowledgement {
    pub fn new(user_id: String, note: Option<String>) -> Self {
        Self { acknowledged_by: user_id, acknowledged_at: Utc::now(), note }
    }
}

/// 解决信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub resolved_by: String,
    pub resolved_at: DateTime<Utc>,
    pub note: Option<String>,
    pub resolution_type: ResolutionType,
}

impl Resolution {
    pub fn new(user_id: String, resolution_type: ResolutionType, note: Option<String>) -> Self {
        Self { resolved_by: user_id, resolved_at: Utc::now(), note, resolution_type }
    }
}

/// 解决方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionType {
    Fixed,
    FalseAlarm,
    Ignored,
    AutoResolved,
}

impl ResolutionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResolutionType::Fixed => "fixed",
            ResolutionType::FalseAlarm => "false_alarm",
            ResolutionType::Ignored => "ignored",
            ResolutionType::AutoResolved => "auto_resolved",
        }
    }
}

impl std::str::FromStr for ResolutionType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Fixed" => Ok(ResolutionType::Fixed),
            "FalseAlarm" => Ok(ResolutionType::FalseAlarm),
            "Ignored" => Ok(ResolutionType::Ignored),
            "AutoResolved" => Ok(ResolutionType::AutoResolved),
            _ => Err(format!("invalid resolution type: {}", s)),
        }
    }
}

/// 通知配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub channels: Vec<NotificationChannelType>,
    pub recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", with = "optional_duration_serde", default)]
    pub suppress_duration: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none", with = "optional_duration_serde", default)]
    pub repeat_interval: Option<Duration>,
    /// 触发去抖动时长：条件必须持续满足该时长后才触发告警。
    /// None = 立即触发（保持现有行为）。
    #[serde(skip_serializing_if = "Option::is_none", with = "optional_duration_serde", default)]
    pub trigger_duration_secs: Option<Duration>,
    /// 恢复去抖动时长：恢复条件必须持续满足该时长后才自动恢复告警。
    /// None = 立即恢复（保持现有行为）。
    #[serde(skip_serializing_if = "Option::is_none", with = "optional_duration_serde", default)]
    pub recovery_duration_secs: Option<Duration>,
}

// Duration 序列化辅助模块
mod duration_serde {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

mod optional_duration_serde {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => serializer.serialize_some(&d.as_secs()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<u64> = Option::deserialize(deserializer)?;
        Ok(opt.map(Duration::from_secs))
    }
}

// ============================================================================
// Entities
// ============================================================================

/// 报警实例实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub device_id: String,
    pub property_id: Option<String>,
    pub rule_id: Option<String>,
    pub alarm_type: AlarmType,
    pub alarm_level: AlarmLevel,
    pub message: String,
    pub alarm_value: Option<String>,
    pub threshold_value: Option<String>,
    pub alarm_time: DateTime<Utc>,
    pub status: AlarmStatus,
    pub acknowledgement: Option<Acknowledgement>,
    pub resolution: Option<Resolution>,
    pub workspace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Alarm {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device_id: String,
        property_id: Option<String>,
        rule_id: Option<String>,
        alarm_type: AlarmType,
        alarm_level: AlarmLevel,
        message: String,
        alarm_value: Option<String>,
        threshold_value: Option<String>,
        workspace_id: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id,
            property_id,
            rule_id,
            alarm_type,
            alarm_level,
            message,
            alarm_value,
            threshold_value,
            alarm_time: now,
            status: AlarmStatus::Active,
            acknowledgement: None,
            resolution: None,
            workspace_id,
            created_at: now,
        }
    }

    pub fn acknowledge(&mut self, user_id: String, note: Option<String>) -> AlarmResult<()> {
        if self.status != AlarmStatus::Active {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "acknowledged".to_string(),
            });
        }
        self.acknowledgement = Some(Acknowledgement::new(user_id, note));
        self.status = AlarmStatus::Acknowledged;
        Ok(())
    }

    pub fn resolve(
        &mut self,
        user_id: String,
        resolution_type: ResolutionType,
        note: Option<String>,
    ) -> AlarmResult<()> {
        if !matches!(self.status, AlarmStatus::Active | AlarmStatus::Acknowledged) {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "resolved".to_string(),
            });
        }
        self.resolution = Some(Resolution::new(user_id, resolution_type, note));
        self.status = AlarmStatus::Resolved;
        Ok(())
    }

    pub fn suppress(&mut self) -> AlarmResult<()> {
        if self.status != AlarmStatus::Active {
            return Err(AlarmError::InvalidStatusTransition {
                from: self.status.as_str().to_string(),
                to: "suppressed".to_string(),
            });
        }
        self.status = AlarmStatus::Suppressed;
        Ok(())
    }

    pub fn can_acknowledge(&self) -> bool {
        self.status == AlarmStatus::Active
    }

    pub fn can_resolve(&self) -> bool {
        matches!(self.status, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }

    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 报警规则实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_type: RuleType,
    pub condition: AlarmCondition,
    pub alarm_level: AlarmLevel,
    pub is_enabled: bool,
    pub notification_config: NotificationConfig,
    pub workspace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AlarmRule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        description: Option<String>,
        device_id: Option<String>,
        property_id: Option<String>,
        rule_type: RuleType,
        condition: AlarmCondition,
        alarm_level: AlarmLevel,
        notification_config: NotificationConfig,
        workspace_id: String,
    ) -> AlarmResult<Self> {
        Self::validate_config(&name, &condition, &notification_config)?;

        let now = Utc::now();
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description,
            device_id,
            property_id,
            rule_type,
            condition,
            alarm_level,
            is_enabled: true,
            notification_config,
            workspace_id: Some(workspace_id),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update(
        &mut self,
        name: Option<String>,
        description: Option<String>,
        condition: Option<AlarmCondition>,
        alarm_level: Option<AlarmLevel>,
        notification_config: Option<NotificationConfig>,
    ) -> AlarmResult<()> {
        if let Some(n) = name {
            if n.is_empty() {
                return Err(AlarmError::InvalidRuleConfig("规则名称不能为空".to_string()));
            }
            self.name = n;
        }
        if let Some(d) = description {
            self.description = Some(d);
        }
        if let Some(c) = condition {
            self.condition = c;
        }
        if let Some(l) = alarm_level {
            self.alarm_level = l;
        }
        if let Some(nc) = notification_config {
            self.notification_config = nc;
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn enable(&mut self) {
        self.is_enabled = true;
        self.updated_at = Utc::now();
    }

    pub fn disable(&mut self) {
        self.is_enabled = false;
        self.updated_at = Utc::now();
    }

    fn validate_config(
        name: &str,
        _condition: &AlarmCondition,
        notification_config: &NotificationConfig,
    ) -> AlarmResult<()> {
        if name.is_empty() {
            return Err(AlarmError::InvalidRuleConfig("规则名称不能为空".to_string()));
        }
        if notification_config.enabled && notification_config.channels.is_empty() {
            return Err(AlarmError::InvalidRuleConfig(
                "启用通知时至少需要配置一个通知渠道".to_string(),
            ));
        }
        if notification_config.enabled {
            let needs_recipients = notification_config.channels.iter().any(|ch| {
                matches!(ch, NotificationChannelType::Email | NotificationChannelType::Sms)
            });
            if needs_recipients && notification_config.recipients.is_empty() {
                return Err(AlarmError::InvalidRuleConfig(
                    "使用邮件或短信通知时需要配置接收人".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// 规则类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Threshold,
    Range,
    Change,
    Duration,
    Composite,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::Threshold => "threshold",
            RuleType::Range => "range",
            RuleType::Change => "change",
            RuleType::Duration => "duration",
            RuleType::Composite => "composite",
        }
    }
}

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

impl From<crate::modules::alarm::Alarm> for AlarmDto {
    fn from(alarm: crate::modules::alarm::Alarm) -> Self {
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

impl From<crate::modules::alarm::AlarmRule> for AlarmRuleDto {
    fn from(rule: crate::modules::alarm::AlarmRule) -> Self {
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
#[serde(rename_all = "camelCase")]
pub struct AlarmStatisticsDto {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}

impl From<crate::modules::alarm::AlarmStatistics> for AlarmStatisticsDto {
    fn from(stats: crate::modules::alarm::AlarmStatistics) -> Self {
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
    Ok(raw.map(|s| s.split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect()))
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
    pub rule_type: crate::modules::alarm::RuleType,
    pub condition: serde_json::Value,
    pub alarm_level: String,
    pub notification_config: serde_json::Value,
}

/// 更新报警规则请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAlarmRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
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
        assert_eq!(AlarmType::Custom { name: "special".to_string() }.as_str(), "custom_special");
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
            AlarmType::Custom { name: "foo".to_string() }
        );
        assert_eq!(AlarmType::parse_str("other"), AlarmType::Custom { name: "other".to_string() });
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
        assert!(alarm.acknowledge("user-1".to_string(), Some("ack note".to_string())).is_ok());
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
        alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None).unwrap();
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
        alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None).unwrap();
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
        alarm.resolve("user-1".to_string(), ResolutionType::Fixed, None).unwrap();
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

        let result = rule.update(Some("New Name".to_string()), None, None, None, None);
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

        let result = rule.update(Some("".to_string()), None, None, None, None);
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
        use crate::modules::alarm::AlarmStatistics;
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
            AlarmCondition::Threshold { operator, value, recovery_threshold } => {
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
        let json = r#"{"enabled":false,"channels":[],"recipients":[],"trigger_duration_secs":30,"recovery_duration_secs":60}"#;
        let config: NotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trigger_duration_secs, Some(Duration::from_secs(30)));
        assert_eq!(config.recovery_duration_secs, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_notification_config_deser_no_new_duration_fields() {
        // Backward compat: old JSON without new fields should deserialize
        let json = r#"{"enabled":false,"channels":[],"recipients":[]}"#;
        let config: NotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trigger_duration_secs, None);
        assert_eq!(config.recovery_duration_secs, None);
    }

    #[test]
    fn test_notification_config_default_has_none_duration_fields() {
        let config = NotificationConfig::default();
        assert_eq!(config.trigger_duration_secs, None);
        assert_eq!(config.recovery_duration_secs, None);
    }
}
