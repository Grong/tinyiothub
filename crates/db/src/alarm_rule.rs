//! Alarm rule 持久化：报警规则（Task 11 自 alarm.rs 拆分）。
//!
//! 规则侧行类型（AlarmRule/AlarmCondition/NotificationConfig/RuleType/EventRuleRow）
//! 与查询函数同住本文件；告警记录侧见 alarm.rs。

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use tinyiothub_core::notification_types::NotificationChannelType;

use crate::alarm::{AlarmLevel, parse_db_datetime};
use crate::database::Db;
use crate::error::{DbError, Result};

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
// Repository 行类型
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

// ============================================================================
// SQLite 查询函数（pub(crate) 自由函数 + Db 门面委托）
// ============================================================================

fn row_to_alarm_rule(row: sqlx::sqlite::SqliteRow) -> Result<AlarmRule> {
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

    let created_at = parse_db_datetime(&created_at_str).unwrap_or_else(|e| {
        tracing::warn!(rule_id = %id, created_at = %created_at_str, error = %e, "Failed to parse created_at, using now");
        Utc::now()
    });
    let updated_at = parse_db_datetime(&updated_at_str).unwrap_or_else(|e| {
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

pub(crate) async fn create_alarm_rule(pool: &SqlitePool, rule: &AlarmRule) -> Result<()> {
    let condition_json =
        serde_json::to_string(&rule.condition).map_err(|e| DbError::Internal(format!("序列化条件配置失败: {}", e)))?;

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
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(format!("创建规则失败: {}", e)))?;

    Ok(())
}

pub(crate) async fn update_alarm_rule(pool: &SqlitePool, rule: &AlarmRule, workspace_id: Option<&str>) -> Result<()> {
    let condition_json =
        serde_json::to_string(&rule.condition).map_err(|e| DbError::Internal(format!("序列化条件配置失败: {}", e)))?;
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
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(format!("更新规则失败: {}", e)))?;

    Ok(())
}

pub(crate) async fn delete_alarm_rule(pool: &SqlitePool, id: &str, workspace_id: Option<&str>) -> Result<()> {
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
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(format!("删除规则失败: {}", e)))?;
    Ok(())
}

pub(crate) async fn find_alarm_rule_by_id(pool: &SqlitePool, id: &str) -> Result<Option<AlarmRule>> {
    let query = "SELECT * FROM device_alarm_rules WHERE id = ?";
    let row = sqlx::query(query)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(format!("查询规则失败: {}", e)))?;

    if let Some(row) = row {
        Ok(Some(row_to_alarm_rule(row)?))
    } else {
        Ok(None)
    }
}

pub(crate) async fn find_enabled_alarm_rules(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<Vec<AlarmRule>> {
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
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("查询启用规则失败: {}", e)))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row_to_alarm_rule(row)?);
    }
    Ok(rules)
}

pub(crate) async fn find_alarm_rules_by_device(
    pool: &SqlitePool,
    device_id: &str,
    workspace_id: Option<&str>,
) -> Result<Vec<AlarmRule>> {
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
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("查询设备规则失败: {}", e)))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row_to_alarm_rule(row)?);
    }
    Ok(rules)
}

pub(crate) async fn find_alarm_rules_by_property(
    pool: &SqlitePool,
    device_id: &str,
    property_id: &str,
) -> Result<Vec<AlarmRule>> {
    let query = "SELECT * FROM device_alarm_rules WHERE device_id = ? AND property_id = ? ORDER BY created_at DESC";
    let rows = sqlx::query(query)
        .bind(device_id)
        .bind(property_id)
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("查询属性规则失败: {}", e)))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row_to_alarm_rule(row)?);
    }
    Ok(rules)
}

pub(crate) async fn find_global_alarm_rules(pool: &SqlitePool) -> Result<Vec<AlarmRule>> {
    let query = "SELECT * FROM device_alarm_rules WHERE device_id IS NULL ORDER BY created_at DESC";
    let rows = sqlx::query(query)
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("查询全局规则失败: {}", e)))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row_to_alarm_rule(row)?);
    }
    Ok(rules)
}

pub(crate) async fn set_alarm_rule_enabled(
    pool: &SqlitePool,
    id: &str,
    enabled: bool,
    workspace_id: Option<&str>,
) -> Result<()> {
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
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(format!("更新规则状态失败: {}", e)))?;
    Ok(())
}

pub(crate) async fn find_event_alarm_rules(
    pool: &SqlitePool,
    workspace_id: &str,
    device_id: Option<&str>,
) -> Result<Vec<EventRuleRow>> {
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
        .fetch_all(pool)
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

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 创建报警规则。
    pub async fn create_alarm_rule(&self, rule: &AlarmRule) -> Result<()> {
        create_alarm_rule(self.pool(), rule).await
    }

    /// 更新报警规则（可选 workspace 限定）。
    pub async fn update_alarm_rule(&self, rule: &AlarmRule, workspace_id: Option<&str>) -> Result<()> {
        update_alarm_rule(self.pool(), rule, workspace_id).await
    }

    /// 删除报警规则（可选 workspace 限定）。
    pub async fn delete_alarm_rule(&self, id: &str, workspace_id: Option<&str>) -> Result<()> {
        delete_alarm_rule(self.pool(), id, workspace_id).await
    }

    /// 按 id 查询报警规则。
    pub async fn find_alarm_rule_by_id(&self, id: &str) -> Result<Option<AlarmRule>> {
        find_alarm_rule_by_id(self.pool(), id).await
    }

    /// 查询启用的报警规则（可选 workspace 过滤）。
    pub async fn find_enabled_alarm_rules(&self, workspace_id: Option<&str>) -> Result<Vec<AlarmRule>> {
        find_enabled_alarm_rules(self.pool(), workspace_id).await
    }

    /// 查询设备的报警规则（可选 workspace 过滤）。
    pub async fn find_alarm_rules_by_device(
        &self,
        device_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<Vec<AlarmRule>> {
        find_alarm_rules_by_device(self.pool(), device_id, workspace_id).await
    }

    /// 查询设备属性的报警规则。
    pub async fn find_alarm_rules_by_property(&self, device_id: &str, property_id: &str) -> Result<Vec<AlarmRule>> {
        find_alarm_rules_by_property(self.pool(), device_id, property_id).await
    }

    /// 查询全局报警规则（device_id IS NULL）。
    pub async fn find_global_alarm_rules(&self) -> Result<Vec<AlarmRule>> {
        find_global_alarm_rules(self.pool()).await
    }

    /// 启用/禁用报警规则（可选 workspace 限定）。
    pub async fn set_alarm_rule_enabled(&self, id: &str, enabled: bool, workspace_id: Option<&str>) -> Result<()> {
        set_alarm_rule_enabled(self.pool(), id, enabled, workspace_id).await
    }

    /// 查询事件型报警规则原始行（rule_type='event' 且启用）。
    pub async fn find_event_alarm_rules(
        &self,
        workspace_id: &str,
        device_id: Option<&str>,
    ) -> Result<Vec<EventRuleRow>> {
        find_event_alarm_rules(self.pool(), workspace_id, device_id).await
    }
}
