//! Alarm 持久化：报警记录（P-集中化 E2，自 alarm crate 迁入；Task 11 拆出 alarm_rule.rs）。
//!
//! 类型随 repo 住 db（方案 B）：Alarm 及嵌入枚举为 DB 行类型，
//! alarm crate 保留 DTO/规则评估/通知分发，经 re-export 兼容。
//! 规则侧类型经 `pub use crate::alarm_rule::*` 再导出，保持既有路径兼容。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::database::Db;
use crate::error::{DbError, Result};

// 规则侧类型再导出（Task 11 拆分后保持 `alarm::RuleType` 等既有路径可用）。
pub use crate::alarm_rule::*;

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

// ──────────────────────────────────────────────
// Repositories
// ──────────────────────────────────────────────

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

/// Parse a datetime string from the database, handling both RFC3339 and SQLite formats.
pub(crate) fn parse_db_datetime(s: &str) -> std::result::Result<DateTime<Utc>, String> {
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
// SQLite 查询函数（pub(crate) 自由函数 + Db 门面委托）
// ============================================================================

fn row_to_alarm(row: sqlx::sqlite::SqliteRow) -> Result<Alarm> {
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

    let alarm_time = parse_db_datetime(&alarm_time_str).unwrap_or_else(|e| {
        tracing::warn!(alarm_id = %id, alarm_time = %alarm_time_str, error = %e, "Parse alarm_time failed, using now");
        Utc::now()
    });

    let created_at = parse_db_datetime(&created_at_str).unwrap_or_else(|e| {
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

pub(crate) async fn insert_alarm(pool: &SqlitePool, alarm: &Alarm) -> Result<()> {
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
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn update_alarm(pool: &SqlitePool, alarm: &Alarm) -> Result<()> {
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
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn find_alarm_by_id(pool: &SqlitePool, id: &str, workspace_id: Option<&str>) -> Result<Option<Alarm>> {
    let query = if workspace_id.is_some() {
        "SELECT * FROM device_alarms WHERE id = ? AND workspace_id = ?"
    } else {
        "SELECT * FROM device_alarms WHERE id = ?"
    };
    let mut sqlx_query = sqlx::query(query).bind(id);
    if let Some(ws) = workspace_id {
        sqlx_query = sqlx_query.bind(ws);
    }
    let row = sqlx_query.fetch_optional(pool).await?;
    if let Some(row) = row {
        Ok(Some(row_to_alarm(row)?))
    } else {
        Ok(None)
    }
}

pub(crate) async fn find_alarms_by_criteria(pool: &SqlitePool, criteria: &AlarmQueryCriteria) -> Result<Vec<Alarm>> {
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
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("Query failed: {}", e)))?;

    let mut alarms = Vec::new();
    for row in rows {
        alarms.push(row_to_alarm(row)?);
    }

    Ok(alarms)
}

pub(crate) async fn list_active_alarms(pool: &SqlitePool, device_id: Option<&str>) -> Result<Vec<Alarm>> {
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
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("find_active query failed: {}", e)))?;

    let mut alarms = Vec::new();
    for row in rows {
        alarms.push(row_to_alarm(row)?);
    }

    Ok(alarms)
}

pub(crate) async fn list_unacknowledged_alarms(pool: &SqlitePool, device_id: Option<&str>) -> Result<Vec<Alarm>> {
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
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(format!("find_unacknowledged query failed: {}", e)))?;

    let mut alarms = Vec::new();
    for row in rows {
        alarms.push(row_to_alarm(row)?);
    }

    Ok(alarms)
}

pub(crate) async fn count_alarms_by_criteria(pool: &SqlitePool, criteria: &AlarmQueryCriteria) -> Result<u64> {
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
        .fetch_one(pool)
        .await
        .map_err(|e| DbError::Internal(format!("Count query failed: {}", e)))?;

    use sqlx::Row;
    let count: i64 = row.get("count");
    Ok(count as u64)
}

pub(crate) async fn batch_update_alarm_status(
    pool: &SqlitePool,
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
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(format!("batch_update_status failed: {}", e)))?;

    Ok(result.rows_affected() as usize)
}

pub(crate) async fn delete_old_alarms(pool: &SqlitePool, before: DateTime<Utc>) -> Result<usize> {
    let query = "DELETE FROM device_alarms WHERE created_at < ? AND is_resolved = true";
    let result = sqlx::query(query).bind(before.to_rfc3339()).execute(pool).await?;
    Ok(result.rows_affected() as usize)
}

pub(crate) async fn count_active_alarms_by_device(pool: &SqlitePool, device_id: &str) -> Result<u32> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND is_resolved = 0")
        .bind(device_id)
        .fetch_one(pool)
        .await?;
    Ok(count as u32)
}

pub(crate) async fn count_all_active_alarms(pool: &SqlitePool) -> Result<u32> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE is_resolved = 0")
        .fetch_one(pool)
        .await?;
    Ok(count as u32)
}

pub(crate) async fn count_offline_alarms(pool: &SqlitePool, device_id: &str, days: u32) -> Result<u32> {
    let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND alarm_message LIKE '%离线%' AND alarm_time > datetime('now', ?)",
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
    Ok(count as u32)
}

// ──────────────────────────────────────────────
// cloud handler SQL 吸收（Task 11）
// ──────────────────────────────────────────────

/// `/alarms/recent` 查询行：(id, device_id, device_name, alarm_level,
/// alarm_message, alarm_time, is_acknowledged, is_resolved)。
pub type RecentAlarmRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    chrono::NaiveDateTime,
    bool,
    bool,
);

/// 最近告警列表（LEFT JOIN devices 取设备名；自 cloud alarm handler 吸收，SQL 逐字）。
pub(crate) async fn list_recent_alarms(
    pool: &SqlitePool,
    limit: i32,
    workspace_id: Option<&str>,
) -> std::result::Result<Vec<RecentAlarmRow>, sqlx::Error> {
    let alarms: Vec<RecentAlarmRow> = if let Some(wid) = workspace_id {
        sqlx::query_as(
            r#"
            SELECT
                da.id,
                da.device_id,
                d.name,
                da.alarm_level,
                da.alarm_message,
                da.alarm_time,
                da.is_acknowledged,
                da.is_resolved
            FROM device_alarms da
            LEFT JOIN devices d ON da.device_id = d.id
            WHERE da.workspace_id = ?
            ORDER BY da.alarm_time DESC
            LIMIT ?"#,
        )
        .bind(wid)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT
                da.id,
                da.device_id,
                d.name,
                da.alarm_level,
                da.alarm_message,
                da.alarm_time,
                da.is_acknowledged,
                da.is_resolved
            FROM device_alarms da
            LEFT JOIN devices d ON da.device_id = d.id
            ORDER BY da.alarm_time DESC
            LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(alarms)
}

/// 批量加载告警设备名（display_name 优先；自 cloud alarm handler 吸收，SQL 逐字）。
pub(crate) async fn load_alarm_device_names(
    pool: &SqlitePool,
    alarms: &[Alarm],
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if alarms.is_empty() {
        return map;
    }
    let placeholders = vec!["?"; alarms.len()].join(",");
    let query = format!(
        "SELECT id, display_name, name FROM devices WHERE id IN ({})",
        placeholders
    );
    let mut q = sqlx::query(sqlx::AssertSqlSafe(query));
    for a in alarms {
        q = q.bind(&a.device_id);
    }
    let rows = q.fetch_all(pool).await.unwrap_or_else(|e| {
        tracing::error!("Failed to load device names for alarm list: {}", e);
        Vec::new()
    });
    for row in rows {
        use sqlx::Row;
        let id: String = row.get("id");
        let display: Option<String> = row.get("display_name");
        let name: String = row.get("name");
        map.insert(id, display.unwrap_or(name));
    }
    map
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 插入告警记录。
    pub async fn insert_alarm(&self, alarm: &Alarm) -> Result<()> {
        insert_alarm(self.pool(), alarm).await
    }

    /// 更新告警确认/解决状态。
    pub async fn update_alarm(&self, alarm: &Alarm) -> Result<()> {
        update_alarm(self.pool(), alarm).await
    }

    /// 按 id 查询告警（可选 workspace 过滤）。
    pub async fn find_alarm_by_id(&self, id: &str, workspace_id: Option<&str>) -> Result<Option<Alarm>> {
        find_alarm_by_id(self.pool(), id, workspace_id).await
    }

    /// 按条件查询告警。
    pub async fn find_alarms_by_criteria(&self, criteria: &AlarmQueryCriteria) -> Result<Vec<Alarm>> {
        find_alarms_by_criteria(self.pool(), criteria).await
    }

    /// 查询未解决告警（可选按设备过滤）。
    pub async fn list_active_alarms(&self, device_id: Option<&str>) -> Result<Vec<Alarm>> {
        list_active_alarms(self.pool(), device_id).await
    }

    /// 查询未确认且未解决告警（可选按设备过滤）。
    pub async fn list_unacknowledged_alarms(&self, device_id: Option<&str>) -> Result<Vec<Alarm>> {
        list_unacknowledged_alarms(self.pool(), device_id).await
    }

    /// 按条件统计告警数。
    pub async fn count_alarms_by_criteria(&self, criteria: &AlarmQueryCriteria) -> Result<u64> {
        count_alarms_by_criteria(self.pool(), criteria).await
    }

    /// 批量更新告警状态（自动解决时写入 resolution 元数据）。
    pub async fn batch_update_alarm_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
        workspace_id: &str,
    ) -> Result<usize> {
        batch_update_alarm_status(self.pool(), alarm_ids, status, workspace_id).await
    }

    /// 删除指定时间之前已解决的告警。
    pub async fn delete_old_alarms(&self, before: DateTime<Utc>) -> Result<usize> {
        delete_old_alarms(self.pool(), before).await
    }

    /// 统计设备未解决告警数。
    pub async fn count_active_alarms_by_device(&self, device_id: &str) -> Result<u32> {
        count_active_alarms_by_device(self.pool(), device_id).await
    }

    /// 统计全部未解决告警数。
    pub async fn count_all_active_alarms(&self) -> Result<u32> {
        count_all_active_alarms(self.pool()).await
    }

    /// 统计设备近 `days` 天离线告警数。
    pub async fn count_offline_alarms(&self, device_id: &str, days: u32) -> Result<u32> {
        count_offline_alarms(self.pool(), device_id, days).await
    }

    /// 最近告警列表（含设备名）。
    pub async fn list_recent_alarms(
        &self,
        limit: i32,
        workspace_id: Option<&str>,
    ) -> std::result::Result<Vec<RecentAlarmRow>, sqlx::Error> {
        list_recent_alarms(self.pool(), limit, workspace_id).await
    }

    /// 批量加载告警设备名（display_name 优先）。
    pub async fn load_alarm_device_names(&self, alarms: &[Alarm]) -> std::collections::HashMap<String, String> {
        load_alarm_device_names(self.pool(), alarms).await
    }
}
