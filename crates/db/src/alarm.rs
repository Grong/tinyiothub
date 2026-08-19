//! Alarm 持久化：报警与报警规则（P-集中化 E2，自 alarm crate 迁入）。
//!
//! 类型随 repo 住 db（方案 B）：Alarm/AlarmRule 及嵌入枚举为 DB 行类型，
//! alarm crate 保留 DTO/规则评估/通知分发，经 re-export 兼容。

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tinyiothub_core::notification_types::NotificationChannelType;

use crate::database::Db;
use crate::error::{DbError, Result};

// ──────────────────────────────────────────────
// 持久化类型（DB 行）— 自 alarm/types.rs 迁入
// ──────────────────────────────────────────────

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

    pub fn to_event_level(&self) -> tinyiothub_core::models::event::EventLevel {
        match self {
            AlarmLevel::Info => tinyiothub_core::models::event::EventLevel::Info,
            AlarmLevel::Warning => tinyiothub_core::models::event::EventLevel::Warning,
            AlarmLevel::Error => tinyiothub_core::models::event::EventLevel::Error,
            AlarmLevel::Critical => tinyiothub_core::models::event::EventLevel::Critical,
        }
    }

    pub fn from_event_level(level: &tinyiothub_core::models::event::EventLevel) -> Self {
        match level {
            tinyiothub_core::models::event::EventLevel::Debug => AlarmLevel::Info,
            tinyiothub_core::models::event::EventLevel::Info => AlarmLevel::Info,
            tinyiothub_core::models::event::EventLevel::Warning => AlarmLevel::Warning,
            tinyiothub_core::models::event::EventLevel::Error => AlarmLevel::Error,
            tinyiothub_core::models::event::EventLevel::Critical => AlarmLevel::Critical,
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
            s if s.starts_with("custom_") => AlarmType::Custom {
                name: s.strip_prefix("custom_").unwrap_or(s).to_string(),
            },
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
        Self {
            acknowledged_by: user_id,
            acknowledged_at: Utc::now(),
            note,
        }
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
        Self {
            resolved_by: user_id,
            resolved_at: Utc::now(),
            note,
            resolution_type,
        }
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
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 默认 30 秒，防止边界振荡导致的瞬间报警恢复。
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "optional_duration_serde",
        default = "default_recovery_duration"
    )]
    pub recovery_duration_secs: Option<Duration>,
}

fn default_recovery_duration() -> Option<Duration> {
    Some(Duration::from_secs(30))
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: Vec::new(),
            recipients: Vec::new(),
            suppress_duration: None,
            repeat_interval: None,
            trigger_duration_secs: None,
            // Default 30s recovery debounce to prevent single-tick
            // boundary oscillation from immediately auto-resolving.
            recovery_duration_secs: Some(std::time::Duration::from_secs(30)),
        }
    }
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

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
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

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Option<Duration>, D::Error>
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

    pub fn acknowledge(&mut self, user_id: String, note: Option<String>) -> Result<()> {
        if self.status != AlarmStatus::Active {
            return Err(DbError::Validation {
                message: format!(
                    "无效的报警状态转换: 从 {} 到 {}",
                    self.status.as_str().to_string(),
                    "acknowledged".to_string()
                ),
            });
        }
        self.acknowledgement = Some(Acknowledgement::new(user_id, note));
        self.status = AlarmStatus::Acknowledged;
        Ok(())
    }

    pub fn resolve(&mut self, user_id: String, resolution_type: ResolutionType, note: Option<String>) -> Result<()> {
        if !matches!(self.status, AlarmStatus::Active | AlarmStatus::Acknowledged) {
            return Err(DbError::Validation {
                message: format!(
                    "无效的报警状态转换: 从 {} 到 {}",
                    self.status.as_str().to_string(),
                    "resolved".to_string()
                ),
            });
        }
        self.resolution = Some(Resolution::new(user_id, resolution_type, note));
        self.status = AlarmStatus::Resolved;
        Ok(())
    }

    pub fn suppress(&mut self) -> Result<()> {
        if self.status != AlarmStatus::Active {
            return Err(DbError::Validation {
                message: format!(
                    "无效的报警状态转换: 从 {} 到 {}",
                    self.status.as_str().to_string(),
                    "suppressed".to_string()
                ),
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
    ) -> Result<Self> {
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
        property_id: Option<String>,
        condition: Option<AlarmCondition>,
        alarm_level: Option<AlarmLevel>,
        notification_config: Option<NotificationConfig>,
    ) -> Result<()> {
        if let Some(n) = name {
            if n.is_empty() {
                return Err(DbError::Validation {
                    message: "规则名称不能为空".to_string(),
                });
            }
            self.name = n;
        }
        if let Some(d) = description {
            self.description = Some(d);
        }
        if property_id.is_some() {
            self.property_id = property_id;
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
    ) -> Result<()> {
        if name.is_empty() {
            return Err(DbError::Validation {
                message: "规则名称不能为空".to_string(),
            });
        }
        if notification_config.enabled && notification_config.channels.is_empty() {
            return Err(DbError::Validation {
                message: "启用通知时至少需要配置一个通知渠道".to_string(),
            });
        }
        if notification_config.enabled {
            let needs_recipients = notification_config
                .channels
                .iter()
                .any(|ch| matches!(ch, NotificationChannelType::Email | NotificationChannelType::Sms));
            if needs_recipients && notification_config.recipients.is_empty() {
                return Err(DbError::Validation {
                    message: "使用邮件或短信通知时需要配置接收人".to_string(),
                });
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
    /// Event-based alarm rule: triggered when a thing event matches event_name + min_level
    Event,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::Threshold => "threshold",
            RuleType::Range => "range",
            RuleType::Change => "change",
            RuleType::Duration => "duration",
            RuleType::Composite => "composite",
            RuleType::Event => "event",
        }
    }
}

// ──────────────────────────────────────────────
// Repositories
// ──────────────────────────────────────────────

/// Raw row for an event-type alarm rule (rule_type='event').
///
/// The `condition_config` is JSON matching `EventAlarmCondition`,
/// not the usual `AlarmCondition` enum — so it requires separate deserialization.
#[derive(Debug, Clone)]
pub struct EventRuleRow {
    pub id: String,
    pub rule_name: String,
    pub condition_config: String,
    pub alarm_level: String,
    pub workspace_id: Option<String>,
    pub notification_config_json: Option<String>,
}

/// 报警查询条件
#[derive(Debug, Clone, Default)]
pub struct AlarmQueryCriteria {
    pub workspace_id: Option<String>,
    pub device_ids: Option<Vec<String>>,
    pub property_ids: Option<Vec<String>>,
    pub alarm_levels: Option<Vec<AlarmLevel>>,
    pub alarm_types: Option<Vec<AlarmType>>,
    pub statuses: Option<Vec<AlarmStatus>>,
    pub time_range: Option<TimeRange>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// 时间范围
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// 排序顺序
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Parse legacy condition format: {"operator": "gt", "value": 85} → AlarmCondition::Threshold
fn parse_legacy_condition(json: &str) -> std::result::Result<AlarmCondition, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("legacy parse: {}", e))?;
    let op_str = v
        .get("operator")
        .and_then(|o| o.as_str())
        .ok_or_else(|| "legacy: missing operator".to_string())?;
    let val = v
        .get("value")
        .and_then(|n| n.as_f64())
        .ok_or_else(|| "legacy: missing value".to_string())?;
    let op = match op_str {
        "gt" => ComparisonOperator::GreaterThan,
        "lt" => ComparisonOperator::LessThan,
        "gte" => ComparisonOperator::GreaterThanOrEqual,
        "lte" => ComparisonOperator::LessThanOrEqual,
        "eq" => ComparisonOperator::Equal,
        "neq" => ComparisonOperator::NotEqual,
        _ => return Err(format!("legacy: unknown operator '{}'", op_str)),
    };
    Ok(AlarmCondition::Threshold {
        operator: op,
        value: val,
        recovery_threshold: None,
    })
}

/// Parse a datetime string from the database, handling both RFC3339 and SQLite formats.
fn parse_db_datetime(s: &str) -> std::result::Result<DateTime<Utc>, String> {
    // Try RFC3339 first (format used by new code)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt.and_utc());
    }
    // Try ISO 8601 with 'T' separator but no timezone
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt.and_utc());
    }
    Err(format!("unrecognized datetime format: {}", s))
}

// ============================================================================
// SQLite Implementations
// ============================================================================

/// 报警仓储实现
pub struct AlarmRepository {
    db: Arc<Db>,
}

impl AlarmRepository {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    fn row_to_alarm(&self, row: sqlx::sqlite::SqliteRow) -> Result<Alarm> {
        use sqlx::Row;

        let id: String = row.get("id");
        let device_id: String = row.get("device_id");
        let property_id: Option<String> = row.get("property_id");
        let rule_id: Option<String> = row.get("rule_id");
        let alarm_level_str: String = row.get("alarm_level");
        let message: String = row.get("alarm_message");
        let alarm_value: Option<String> = row.get("alarm_value");
        let threshold_value: Option<String> = row.get("threshold_value");
        let alarm_time_str: String = row.get("alarm_time");
        let is_acknowledged: bool = row.get("is_acknowledged");
        let acknowledged_by: Option<String> = row.get("acknowledged_by");
        let acknowledged_at_str: Option<String> = row.get("acknowledged_at");
        let acknowledged_note: Option<String> = row.get("acknowledged_note");
        let is_resolved: bool = row.get("is_resolved");
        let resolved_by: Option<String> = row.get("resolved_by");
        let resolved_at_str: Option<String> = row.get("resolved_at");
        let resolved_note: Option<String> = row.get("resolved_note");
        let created_at_str: String = row.get("created_at");
        let workspace_id: Option<String> = row.get("workspace_id");

        let alarm_level = AlarmLevel::parse_str(&alarm_level_str).ok_or_else(|| DbError::Validation {
            message: format!("Unknown alarm level: {}", alarm_level_str),
        })?;

        let alarm_type = AlarmType::PropertyThreshold;

        let alarm_time = parse_db_datetime(&alarm_time_str)
            .unwrap_or_else(|e| {
                tracing::warn!(alarm_id = %id, alarm_time = %alarm_time_str, error = %e, "Parse alarm_time failed, using now");
                Utc::now()
            });

        let created_at = parse_db_datetime(&created_at_str)
            .unwrap_or_else(|e| {
                tracing::warn!(alarm_id = %id, created_at = %created_at_str, error = %e, "Parse created_at failed, using now");
                Utc::now()
            });

        let acknowledgement = if is_acknowledged {
            let acknowledged_at = acknowledged_at_str.as_ref().and_then(|s| parse_db_datetime(s).ok());

            Some(Acknowledgement {
                acknowledged_by: acknowledged_by.unwrap_or_default(),
                acknowledged_at: acknowledged_at.unwrap_or_else(Utc::now),
                note: acknowledged_note,
            })
        } else {
            None
        };

        let resolution_type_str: Option<String> = row.get("resolution_type");

        let resolution = if is_resolved {
            let resolved_at = resolved_at_str.as_ref().and_then(|s| parse_db_datetime(s).ok());

            let resolution_type = resolution_type_str
                .and_then(|s| match s.as_str() {
                    "fixed" => Some(ResolutionType::Fixed),
                    "false_alarm" => Some(ResolutionType::FalseAlarm),
                    "ignored" => Some(ResolutionType::Ignored),
                    "auto_resolved" => Some(ResolutionType::AutoResolved),
                    _ => None,
                })
                .unwrap_or(ResolutionType::Fixed);

            Some(Resolution {
                resolved_by: resolved_by.unwrap_or_default(),
                resolved_at: resolved_at.unwrap_or_else(Utc::now),
                note: resolved_note,
                resolution_type,
            })
        } else {
            None
        };

        let status = if is_resolved {
            AlarmStatus::Resolved
        } else if is_acknowledged {
            AlarmStatus::Acknowledged
        } else {
            AlarmStatus::Active
        };

        Ok(Alarm {
            id,
            device_id,
            property_id,
            rule_id,
            alarm_type,
            alarm_level,
            message,
            alarm_value,
            threshold_value,
            alarm_time,
            status,
            acknowledgement,
            resolution,
            workspace_id,
            created_at,
        })
    }
}

impl AlarmRepository {
    pub async fn create(&self, alarm: &Alarm) -> Result<()> {
        let query = r#"
            INSERT INTO device_alarms (
                id, device_id, property_id, rule_id, alarm_level,
                alarm_message, alarm_value, threshold_value, alarm_time,
                is_acknowledged, acknowledged_by, acknowledged_at, acknowledged_note,
                is_resolved, resolved_by, resolved_at, resolved_note, resolution_type,
                workspace_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        sqlx::query(query)
            .bind(&alarm.id)
            .bind(&alarm.device_id)
            .bind(&alarm.property_id)
            .bind(&alarm.rule_id)
            .bind(alarm.alarm_level.as_str())
            .bind(&alarm.message)
            .bind(&alarm.alarm_value)
            .bind(&alarm.threshold_value)
            .bind(alarm.alarm_time.to_rfc3339())
            .bind(alarm.acknowledgement.is_some())
            .bind(alarm.acknowledgement.as_ref().map(|a| &a.acknowledged_by))
            .bind(alarm.acknowledgement.as_ref().map(|a| a.acknowledged_at.to_rfc3339()))
            .bind(alarm.acknowledgement.as_ref().and_then(|a| a.note.as_ref()))
            .bind(alarm.resolution.is_some())
            .bind(alarm.resolution.as_ref().map(|r| &r.resolved_by))
            .bind(alarm.resolution.as_ref().map(|r| r.resolved_at.to_rfc3339()))
            .bind(alarm.resolution.as_ref().and_then(|r| r.note.as_ref()))
            .bind(alarm.resolution.as_ref().map(|r| r.resolution_type.as_str()))
            .bind(&alarm.workspace_id)
            .bind(alarm.created_at.to_rfc3339())
            .execute(self.db.pool())
            .await?;

        Ok(())
    }

    pub async fn update(&self, alarm: &Alarm) -> Result<()> {
        let query = r#"
            UPDATE device_alarms SET
                is_acknowledged = ?,
                acknowledged_by = ?,
                acknowledged_at = ?,
                acknowledged_note = ?,
                is_resolved = ?,
                resolved_by = ?,
                resolved_at = ?,
                resolved_note = ?,
                resolution_type = ?
            WHERE id = ?
        "#;

        sqlx::query(query)
            .bind(alarm.acknowledgement.is_some())
            .bind(alarm.acknowledgement.as_ref().map(|a| &a.acknowledged_by))
            .bind(alarm.acknowledgement.as_ref().map(|a| a.acknowledged_at.to_rfc3339()))
            .bind(alarm.acknowledgement.as_ref().and_then(|a| a.note.as_ref()))
            .bind(alarm.resolution.is_some())
            .bind(alarm.resolution.as_ref().map(|r| &r.resolved_by))
            .bind(alarm.resolution.as_ref().map(|r| r.resolved_at.to_rfc3339()))
            .bind(alarm.resolution.as_ref().and_then(|r| r.note.as_ref()))
            .bind(alarm.resolution.as_ref().map(|r| r.resolution_type.as_str()))
            .bind(&alarm.id)
            .execute(self.db.pool())
            .await?;

        Ok(())
    }

    pub async fn find_by_id(&self, id: &str, workspace_id: Option<&str>) -> Result<Option<Alarm>> {
        let query = if workspace_id.is_some() {
            "SELECT * FROM device_alarms WHERE id = ? AND workspace_id = ?"
        } else {
            "SELECT * FROM device_alarms WHERE id = ?"
        };
        let mut sqlx_query = sqlx::query(query).bind(id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        let row = sqlx_query.fetch_optional(self.db.pool()).await?;
        if let Some(row) = row {
            Ok(Some(self.row_to_alarm(row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_by_criteria(&self, criteria: &AlarmQueryCriteria) -> Result<Vec<Alarm>> {
        let mut query = String::from("SELECT * FROM device_alarms WHERE 1=1");
        let mut bindings: Vec<String> = Vec::new();

        if let Some(ref workspace_id) = criteria.workspace_id {
            query.push_str(" AND device_id IN (SELECT id FROM devices WHERE workspace_id = ?)");
            bindings.push(workspace_id.clone());
        }

        if let Some(device_ids) = &criteria.device_ids
            && !device_ids.is_empty()
        {
            let placeholders = vec!["?"; device_ids.len()].join(",");
            query.push_str(&format!(" AND device_id IN ({})", placeholders));
            for id in device_ids {
                bindings.push(id.clone());
            }
        }

        if let Some(levels) = &criteria.alarm_levels
            && !levels.is_empty()
        {
            let placeholders = vec!["?"; levels.len()].join(",");
            query.push_str(&format!(" AND alarm_level IN ({})", placeholders));
            for level in levels {
                bindings.push(level.clone().to_string());
            }
        }

        if let Some(statuses) = &criteria.statuses
            && !statuses.is_empty()
        {
            let mut status_conditions: Vec<&str> = Vec::new();
            for status in statuses {
                match status {
                    AlarmStatus::Active => {
                        status_conditions.push("(is_resolved = false AND is_acknowledged = false)");
                    }
                    AlarmStatus::Acknowledged => {
                        status_conditions.push("(is_resolved = false AND is_acknowledged = true)");
                    }
                    AlarmStatus::Resolved => {
                        status_conditions.push("is_resolved = true");
                    }
                    AlarmStatus::Suppressed => {}
                }
            }
            if !status_conditions.is_empty() {
                query.push_str(&format!(" AND ({})", status_conditions.join(" OR ")));
            }
        }

        if let Some(time_range) = &criteria.time_range {
            query.push_str(" AND alarm_time >= ? AND alarm_time <= ?");
            bindings.push(time_range.start.to_rfc3339());
            bindings.push(time_range.end.to_rfc3339());
        }

        query.push_str(" ORDER BY alarm_time DESC");

        if let Some(limit) = criteria.limit {
            query.push_str(" LIMIT ?");
            bindings.push(limit.to_string());
        }

        if let Some(offset) = criteria.offset {
            query.push_str(" OFFSET ?");
            bindings.push(offset.to_string());
        }

        let mut sqlx_query = sqlx::query(sqlx::AssertSqlSafe(query));
        for binding in &bindings {
            sqlx_query = sqlx_query.bind(binding);
        }

        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("Query failed: {}", e)))?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    pub async fn find_active(&self, device_id: Option<&str>) -> Result<Vec<Alarm>> {
        let query = if device_id.is_some() {
            "SELECT * FROM device_alarms WHERE is_resolved = false AND device_id = ? ORDER BY alarm_time DESC"
        } else {
            "SELECT * FROM device_alarms WHERE is_resolved = false ORDER BY alarm_time DESC"
        };

        let mut sqlx_query = sqlx::query(query);
        if let Some(id) = device_id {
            sqlx_query = sqlx_query.bind(id);
        }

        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("find_active query failed: {}", e)))?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    pub async fn find_unacknowledged(&self, device_id: Option<&str>) -> Result<Vec<Alarm>> {
        let query = if device_id.is_some() {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false AND device_id = ? ORDER BY alarm_time DESC"
        } else {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false ORDER BY alarm_time DESC"
        };

        let mut sqlx_query = sqlx::query(query);
        if let Some(id) = device_id {
            sqlx_query = sqlx_query.bind(id);
        }

        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("find_unacknowledged query failed: {}", e)))?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    pub async fn count_by_criteria(&self, criteria: &AlarmQueryCriteria) -> Result<u64> {
        let mut query = String::from("SELECT COUNT(*) as count FROM device_alarms WHERE 1=1");
        let mut bindings: Vec<String> = Vec::new();

        if let Some(ref workspace_id) = criteria.workspace_id {
            query.push_str(" AND device_id IN (SELECT id FROM devices WHERE workspace_id = ?)");
            bindings.push(workspace_id.clone());
        }

        if let Some(device_ids) = &criteria.device_ids
            && !device_ids.is_empty()
        {
            let placeholders = vec!["?"; device_ids.len()].join(",");
            query.push_str(&format!(" AND device_id IN ({})", placeholders));
            for id in device_ids {
                bindings.push(id.clone());
            }
        }

        if let Some(levels) = &criteria.alarm_levels
            && !levels.is_empty()
        {
            let placeholders = vec!["?"; levels.len()].join(",");
            query.push_str(&format!(" AND alarm_level IN ({})", placeholders));
            for level in levels {
                bindings.push(level.clone().to_string());
            }
        }

        if let Some(statuses) = &criteria.statuses
            && !statuses.is_empty()
        {
            let mut status_conditions: Vec<&str> = Vec::new();
            for status in statuses {
                match status {
                    AlarmStatus::Active => {
                        status_conditions.push("(is_resolved = false AND is_acknowledged = false)");
                    }
                    AlarmStatus::Acknowledged => {
                        status_conditions.push("(is_resolved = false AND is_acknowledged = true)");
                    }
                    AlarmStatus::Resolved => {
                        status_conditions.push("is_resolved = true");
                    }
                    AlarmStatus::Suppressed => {}
                }
            }
            if !status_conditions.is_empty() {
                query.push_str(&format!(" AND ({})", status_conditions.join(" OR ")));
            }
        }

        if let Some(time_range) = &criteria.time_range {
            query.push_str(" AND alarm_time >= ? AND alarm_time <= ?");
            bindings.push(time_range.start.to_rfc3339());
            bindings.push(time_range.end.to_rfc3339());
        }

        let mut sqlx_query = sqlx::query(sqlx::AssertSqlSafe(query));
        for binding in &bindings {
            sqlx_query = sqlx_query.bind(binding);
        }

        let row = sqlx_query
            .fetch_one(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("Count query failed: {}", e)))?;

        use sqlx::Row;
        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    pub async fn batch_update_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
        workspace_id: &str,
    ) -> Result<usize> {
        if alarm_ids.is_empty() {
            return Ok(0);
        }

        let (is_resolved, is_acknowledged) = match status {
            AlarmStatus::Active => (false, false),
            AlarmStatus::Acknowledged => (false, true),
            AlarmStatus::Resolved => (true, true),
            AlarmStatus::Suppressed => return Ok(0),
        };

        // When auto-resolving, also set resolution metadata.
        // resolved_by is NULL because auto-resolve has no human actor;
        // resolution_type = 'auto_resolved' marks it as system-resolved.
        let (resolved_by, resolved_at, resolution_type) = if is_resolved {
            (
                None::<&str>,
                Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
                Some("auto_resolved"),
            )
        } else {
            (None, None, None)
        };

        let placeholders = vec!["?"; alarm_ids.len()].join(",");
        // Filter by is_resolved = 0 to avoid re-resolving already-resolved alarms
        let query = if workspace_id.is_empty() {
            format!(
                "UPDATE device_alarms SET is_resolved = ?, is_acknowledged = ?, resolved_by = ?, resolved_at = ?, resolution_type = ? WHERE is_resolved = 0 AND id IN ({})",
                placeholders
            )
        } else {
            format!(
                "UPDATE device_alarms SET is_resolved = ?, is_acknowledged = ?, resolved_by = ?, resolved_at = ?, resolution_type = ? WHERE is_resolved = 0 AND id IN ({}) AND device_id IN (SELECT id FROM devices WHERE workspace_id = ?)",
                placeholders
            )
        };

        let mut sqlx_query = sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(is_resolved)
            .bind(is_acknowledged)
            .bind(resolved_by)
            .bind(resolved_at)
            .bind(resolution_type);
        for id in alarm_ids {
            sqlx_query = sqlx_query.bind(id);
        }
        if !workspace_id.is_empty() {
            sqlx_query = sqlx_query.bind(workspace_id);
        }

        let result = sqlx_query
            .execute(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("batch_update_status failed: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }

    pub async fn delete_old_alarms(&self, before: DateTime<Utc>) -> Result<usize> {
        let query = "DELETE FROM device_alarms WHERE created_at < ? AND is_resolved = true";
        let result = sqlx::query(query)
            .bind(before.to_rfc3339())
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected() as usize)
    }

    pub async fn count_active_alarms_by_device(&self, device_id: &str) -> Result<u32> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND is_resolved = 0")
                .bind(device_id)
                .fetch_one(self.db.pool())
                .await?;
        Ok(count as u32)
    }

    pub async fn count_all_active_alarms(&self) -> Result<u32> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE is_resolved = 0")
            .fetch_one(self.db.pool())
            .await?;
        Ok(count as u32)
    }

    pub async fn count_offline_alarms(&self, device_id: &str, days: u32) -> Result<u32> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND alarm_message LIKE '%离线%' AND alarm_time > datetime('now', ?)",
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(self.db.pool())
        .await?
        .unwrap_or(0);
        Ok(count as u32)
    }
}

// ============================================================================
// Alarm Rule Repository SQLite Implementation
// ============================================================================

/// 报警规则仓储实现
pub struct AlarmRuleRepository {
    db: Arc<Db>,
}

impl AlarmRuleRepository {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    fn row_to_alarm_rule(&self, row: sqlx::sqlite::SqliteRow) -> Result<AlarmRule> {
        use sqlx::Row;

        let id: String = row.get("id");
        let name: String = row.get("rule_name");
        let description: Option<String> = row.get("description");
        let device_id: Option<String> = row.get("device_id");
        let property_id: Option<String> = row.get("property_id");
        let rule_type_str: String = row.get("rule_type");
        let condition_json: String = row.get("condition_config");
        let alarm_level_str: String = row.get("alarm_level");
        let is_enabled: bool = row.get("is_enabled");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let rule_type = match rule_type_str.as_str() {
            "threshold" => RuleType::Threshold,
            "range" => RuleType::Range,
            "change" => RuleType::Change,
            "duration" => RuleType::Duration,
            "composite" => RuleType::Composite,
            "event" => RuleType::Event,
            _ => {
                return Err(DbError::Validation {
                    message: format!("未知的规则类型: {}", rule_type_str),
                });
            }
        };

        let condition: AlarmCondition = serde_json::from_str(&condition_json)
            .or_else(|_| parse_legacy_condition(&condition_json))
            .unwrap_or_else(|e| {
                tracing::warn!(
                    rule_id = %id,
                    condition_json = %condition_json,
                    error = %e,
                    "Failed to parse stored condition, falling back to default"
                );
                AlarmCondition::Threshold {
                    operator: ComparisonOperator::GreaterThan,
                    value: 0.0,
                    recovery_threshold: None,
                }
            });

        let alarm_level = AlarmLevel::parse_str(&alarm_level_str).ok_or_else(|| DbError::Validation {
            message: format!("未知的告警级别: {}", alarm_level_str),
        })?;

        let created_at = parse_db_datetime(&created_at_str)
            .unwrap_or_else(|e| {
                tracing::warn!(rule_id = %id, created_at = %created_at_str, error = %e, "Failed to parse created_at, using now");
                Utc::now()
            });
        let updated_at = parse_db_datetime(&updated_at_str)
            .unwrap_or_else(|e| {
                tracing::warn!(rule_id = %id, updated_at = %updated_at_str, error = %e, "Failed to parse updated_at, using now");
                Utc::now()
            });

        let notification_config_json: Option<String> = row.get("notification_config");
        let notification_config = notification_config_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let workspace_id: Option<String> = row.get("workspace_id");

        Ok(AlarmRule {
            id,
            name,
            description,
            device_id,
            property_id,
            rule_type,
            condition,
            alarm_level,
            is_enabled,
            notification_config,
            workspace_id,
            created_at,
            updated_at,
        })
    }
}

impl AlarmRuleRepository {
    pub async fn create(&self, rule: &AlarmRule) -> Result<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| DbError::Internal(format!("序列化条件配置失败: {}", e)))?;

        let device_id = rule.device_id.as_ref().filter(|s| !s.is_empty());
        let property_id = rule.property_id.as_ref().filter(|s| !s.is_empty());

        let notification_config_json = serde_json::to_string(&rule.notification_config).ok();

        let query = r#"
            INSERT INTO device_alarm_rules (
                id, device_id, property_id, rule_name, rule_type,
                condition_config, alarm_level, is_enabled, description,
                notification_config, workspace_id, created_by, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
        "#;

        sqlx::query(query)
            .bind(&rule.id)
            .bind(device_id)
            .bind(property_id)
            .bind(&rule.name)
            .bind(rule.rule_type.as_str())
            .bind(&condition_json)
            .bind(rule.alarm_level.as_str())
            .bind(rule.is_enabled)
            .bind(&rule.description)
            .bind(&notification_config_json)
            .bind(&rule.workspace_id)
            .bind(rule.created_at.to_rfc3339())
            .bind(rule.updated_at.to_rfc3339())
            .execute(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("创建规则失败: {}", e)))?;

        Ok(())
    }

    pub async fn update(&self, rule: &AlarmRule, workspace_id: Option<&str>) -> Result<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| DbError::Internal(format!("序列化条件配置失败: {}", e)))?;
        let notification_config_json = serde_json::to_string(&rule.notification_config).ok();

        let query = if workspace_id.is_some() {
            r#"
            UPDATE device_alarm_rules SET
                rule_name = ?,
                rule_type = ?,
                condition_config = ?,
                alarm_level = ?,
                is_enabled = ?,
                description = ?,
                notification_config = ?,
                updated_at = ?
            WHERE id = ? AND workspace_id = ?
            "#
        } else {
            r#"
            UPDATE device_alarm_rules SET
                rule_name = ?,
                rule_type = ?,
                condition_config = ?,
                alarm_level = ?,
                is_enabled = ?,
                description = ?,
                notification_config = ?,
                updated_at = ?
            WHERE id = ?
            "#
        };

        let mut sqlx_query = sqlx::query(query)
            .bind(&rule.name)
            .bind(rule.rule_type.as_str())
            .bind(&condition_json)
            .bind(rule.alarm_level.as_str())
            .bind(rule.is_enabled)
            .bind(&rule.description)
            .bind(&notification_config_json)
            .bind(rule.updated_at.to_rfc3339())
            .bind(&rule.id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        sqlx_query
            .execute(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("更新规则失败: {}", e)))?;

        Ok(())
    }

    pub async fn delete(&self, id: &str, workspace_id: Option<&str>) -> Result<()> {
        let query = if workspace_id.is_some() {
            "DELETE FROM device_alarm_rules WHERE id = ? AND workspace_id = ?"
        } else {
            "DELETE FROM device_alarm_rules WHERE id = ?"
        };
        let mut sqlx_query = sqlx::query(query).bind(id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        sqlx_query
            .execute(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("删除规则失败: {}", e)))?;
        Ok(())
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE id = ?";
        let row = sqlx::query(query)
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询规则失败: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(self.row_to_alarm_rule(row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_enabled(&self, workspace_id: Option<&str>) -> Result<Vec<AlarmRule>> {
        let (query, bind_val) = if let Some(ws) = workspace_id {
            (
                "SELECT * FROM device_alarm_rules WHERE is_enabled = true AND workspace_id = ? ORDER BY created_at DESC",
                Some(ws),
            )
        } else {
            (
                "SELECT * FROM device_alarm_rules WHERE is_enabled = true ORDER BY created_at DESC",
                None,
            )
        };
        let mut sqlx_query = sqlx::query(query);
        if let Some(ws) = bind_val {
            sqlx_query = sqlx_query.bind(ws);
        }
        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询启用规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    pub async fn find_by_device(&self, device_id: &str, workspace_id: Option<&str>) -> Result<Vec<AlarmRule>> {
        let (query, bind_ws) = if let Some(ws) = workspace_id {
            (
                "SELECT * FROM device_alarm_rules WHERE device_id = ? AND workspace_id = ? ORDER BY created_at DESC",
                Some(ws),
            )
        } else {
            (
                "SELECT * FROM device_alarm_rules WHERE device_id = ? ORDER BY created_at DESC",
                None,
            )
        };
        let mut sqlx_query = sqlx::query(query).bind(device_id);
        if let Some(ws) = bind_ws {
            sqlx_query = sqlx_query.bind(ws);
        }
        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询设备规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    pub async fn find_by_property(&self, device_id: &str, property_id: &str) -> Result<Vec<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE device_id = ? AND property_id = ? ORDER BY created_at DESC";
        let rows = sqlx::query(query)
            .bind(device_id)
            .bind(property_id)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询属性规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    pub async fn find_global_rules(&self) -> Result<Vec<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE device_id IS NULL ORDER BY created_at DESC";
        let rows = sqlx::query(query)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询全局规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool, workspace_id: Option<&str>) -> Result<()> {
        let query = if workspace_id.is_some() {
            "UPDATE device_alarm_rules SET is_enabled = ?, updated_at = ? WHERE id = ? AND workspace_id = ?"
        } else {
            "UPDATE device_alarm_rules SET is_enabled = ?, updated_at = ? WHERE id = ?"
        };
        let mut sqlx_query = sqlx::query(query).bind(enabled).bind(Utc::now().to_rfc3339()).bind(id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        sqlx_query
            .execute(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("更新规则状态失败: {}", e)))?;
        Ok(())
    }

    pub async fn find_event_rules(&self, workspace_id: &str, device_id: Option<&str>) -> Result<Vec<EventRuleRow>> {
        use sqlx::Row;

        let query = if device_id.is_some() {
            "SELECT id, rule_name, condition_config, alarm_level, workspace_id, notification_config
             FROM device_alarm_rules
             WHERE rule_type = 'event'
               AND is_enabled = 1
               AND workspace_id = ?
               AND (device_id = ? OR device_id IS NULL)
             ORDER BY created_at DESC"
        } else {
            "SELECT id, rule_name, condition_config, alarm_level, workspace_id, notification_config
             FROM device_alarm_rules
             WHERE rule_type = 'event'
               AND is_enabled = 1
               AND workspace_id = ?
               AND device_id IS NULL
             ORDER BY created_at DESC"
        };

        let mut sqlx_query = sqlx::query(query).bind(workspace_id);
        if let Some(did) = device_id {
            sqlx_query = sqlx_query.bind(did);
        }

        let rows = sqlx_query
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| DbError::Internal(format!("查询事件规则失败: {}", e)))?;

        let mut event_rules = Vec::new();
        for row in rows {
            event_rules.push(EventRuleRow {
                id: row.get("id"),
                rule_name: row.get("rule_name"),
                condition_config: row.get("condition_config"),
                alarm_level: row.get("alarm_level"),
                workspace_id: row.get("workspace_id"),
                notification_config_json: row.get("notification_config"),
            });
        }
        Ok(event_rules)
    }
}
