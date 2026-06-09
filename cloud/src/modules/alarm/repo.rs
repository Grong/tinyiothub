// Alarm repository traits + SQLite implementations

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tinyiothub_storage::sqlite::Database;

use super::types::*;

/// 报警仓储接口
#[async_trait]
pub trait AlarmRepository: Send + Sync {
    async fn create(&self, alarm: &Alarm) -> AlarmResult<()>;
    async fn update(&self, alarm: &Alarm) -> AlarmResult<()>;
    async fn find_by_id(&self, id: &str, workspace_id: Option<&str>) -> AlarmResult<Option<Alarm>>;
    async fn find_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>>;
    async fn find_active(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>>;
    async fn find_unacknowledged(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>>;
    async fn count_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<u64>;
    async fn batch_update_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
    ) -> AlarmResult<usize>;
    async fn delete_old_alarms(&self, before: DateTime<Utc>) -> AlarmResult<usize>;
}

/// 报警规则仓储接口
#[async_trait]
pub trait AlarmRuleRepository: Send + Sync {
    async fn create(&self, rule: &AlarmRule) -> AlarmResult<()>;
    /// 更新规则；workspace_id 用于 WHERE 子句确保租户隔离
    async fn update(&self, rule: &AlarmRule, workspace_id: Option<&str>) -> AlarmResult<()>;
    /// 删除规则；workspace_id 用于 WHERE 子句确保租户隔离
    async fn delete(&self, id: &str, workspace_id: Option<&str>) -> AlarmResult<()>;
    async fn find_by_id(&self, id: &str) -> AlarmResult<Option<AlarmRule>>;
    async fn find_enabled(&self, workspace_id: Option<&str>) -> AlarmResult<Vec<AlarmRule>>;
    async fn find_by_device(
        &self,
        device_id: &str,
        workspace_id: Option<&str>,
    ) -> AlarmResult<Vec<AlarmRule>>;
    async fn find_by_property(
        &self,
        device_id: &str,
        property_id: &str,
    ) -> AlarmResult<Vec<AlarmRule>>;
    async fn find_global_rules(&self) -> AlarmResult<Vec<AlarmRule>>;
    /// 启用/禁用规则；workspace_id 用于 WHERE 子句确保租户隔离
    async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        workspace_id: Option<&str>,
    ) -> AlarmResult<()>;
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
fn parse_legacy_condition(json: &str) -> Result<AlarmCondition, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("legacy parse: {}", e))?;
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
    Ok(AlarmCondition::Threshold { operator: op, value: val })
}

/// Parse a datetime string from the database, handling both RFC3339 and SQLite formats.
fn parse_db_datetime(s: &str) -> Result<DateTime<Utc>, String> {
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
pub struct SqliteAlarmRepository {
    database: Arc<Database>,
}

impl SqliteAlarmRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    fn row_to_alarm(&self, row: sqlx::sqlite::SqliteRow) -> AlarmResult<Alarm> {
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

        let alarm_level = AlarmLevel::parse_str(&alarm_level_str).ok_or_else(|| {
            AlarmError::InvalidRuleConfig(format!("Unknown alarm level: {}", alarm_level_str))
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
            let acknowledged_at =
                acknowledged_at_str.as_ref().and_then(|s| parse_db_datetime(s).ok());

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
            created_at,
        })
    }

    /// 统计设备的活跃告警数量
    pub async fn count_active_alarms_by_device(&self, device_id: &str) -> Result<u32, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND is_resolved = 0",
        )
        .bind(device_id)
        .fetch_one(self.database.pool())
        .await?;
        Ok(count as u32)
    }

    /// 统计所有活跃告警数量
    pub async fn count_all_active_alarms(&self) -> Result<u32, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM device_alarms WHERE is_resolved = 0")
                .fetch_one(self.database.pool())
                .await?;
        Ok(count as u32)
    }

    /// 统计设备离线告警数量（最近N天）
    pub async fn count_offline_alarms(
        &self,
        device_id: &str,
        days: u32,
    ) -> Result<u32, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_alarms WHERE device_id = ? AND alarm_message LIKE '%离线%' AND alarm_time > datetime('now', ?)",
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await?
        .unwrap_or(0);
        Ok(count as u32)
    }
}

#[async_trait]
impl AlarmRepository for SqliteAlarmRepository {
    async fn create(&self, alarm: &Alarm) -> AlarmResult<()> {
        let query = r#"
            INSERT INTO device_alarms (
                id, device_id, property_id, rule_id, alarm_level,
                alarm_message, alarm_value, threshold_value, alarm_time,
                is_acknowledged, acknowledged_by, acknowledged_at, acknowledged_note,
                is_resolved, resolved_by, resolved_at, resolved_note, resolution_type, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(alarm.created_at.to_rfc3339())
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn update(&self, alarm: &Alarm) -> AlarmResult<()> {
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
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &str, workspace_id: Option<&str>) -> AlarmResult<Option<Alarm>> {
        let query = if workspace_id.is_some() {
            "SELECT * FROM device_alarms WHERE id = ? AND workspace_id = ?"
        } else {
            "SELECT * FROM device_alarms WHERE id = ?"
        };
        let mut sqlx_query = sqlx::query(query).bind(id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        let row = sqlx_query.fetch_optional(self.database.pool()).await?;
        if let Some(row) = row { Ok(Some(self.row_to_alarm(row)?)) } else { Ok(None) }
    }

    async fn find_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>> {
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
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("Query failed: {}", e)))?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    async fn find_active(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
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
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("find_active query failed: {}", e)))?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    async fn find_unacknowledged(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
        let query = if device_id.is_some() {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false AND device_id = ? ORDER BY alarm_time DESC"
        } else {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false ORDER BY alarm_time DESC"
        };

        let mut sqlx_query = sqlx::query(query);
        if let Some(id) = device_id {
            sqlx_query = sqlx_query.bind(id);
        }

        let rows = sqlx_query.fetch_all(self.database.pool()).await.map_err(|e| {
            AlarmError::InternalError(format!("find_unacknowledged query failed: {}", e))
        })?;

        let mut alarms = Vec::new();
        for row in rows {
            alarms.push(self.row_to_alarm(row)?);
        }

        Ok(alarms)
    }

    async fn count_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<u64> {
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
            .fetch_one(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("Count query failed: {}", e)))?;

        use sqlx::Row;
        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    async fn batch_update_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
    ) -> AlarmResult<usize> {
        if alarm_ids.is_empty() {
            return Ok(0);
        }

        let (is_resolved, is_acknowledged) = match status {
            AlarmStatus::Active => (false, false),
            AlarmStatus::Acknowledged => (false, true),
            AlarmStatus::Resolved => (true, true),
            AlarmStatus::Suppressed => return Ok(0),
        };

        let placeholders = vec!["?"; alarm_ids.len()].join(",");
        let query = format!(
            "UPDATE device_alarms SET is_resolved = ?, is_acknowledged = ? WHERE id IN ({})",
            placeholders
        );

        let mut sqlx_query =
            sqlx::query(sqlx::AssertSqlSafe(query.clone())).bind(is_resolved).bind(is_acknowledged);
        for id in alarm_ids {
            sqlx_query = sqlx_query.bind(id);
        }

        let result = sqlx_query
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("batch_update_status failed: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }

    async fn delete_old_alarms(&self, before: DateTime<Utc>) -> AlarmResult<usize> {
        let query = "DELETE FROM device_alarms WHERE created_at < ? AND is_resolved = true";
        let result =
            sqlx::query(query).bind(before.to_rfc3339()).execute(self.database.pool()).await?;
        Ok(result.rows_affected() as usize)
    }
}

// ============================================================================
// Alarm Rule Repository SQLite Implementation
// ============================================================================

/// 报警规则仓储实现
pub struct SqliteAlarmRuleRepository {
    database: Arc<Database>,
}

impl SqliteAlarmRuleRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    fn row_to_alarm_rule(&self, row: sqlx::sqlite::SqliteRow) -> AlarmResult<AlarmRule> {
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
            _ => {
                return Err(AlarmError::InvalidRuleConfig(format!(
                    "未知的规则类型: {}",
                    rule_type_str
                )));
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
                AlarmCondition::Threshold { operator: ComparisonOperator::GreaterThan, value: 0.0 }
            });

        let alarm_level = AlarmLevel::parse_str(&alarm_level_str).ok_or_else(|| {
            AlarmError::InvalidRuleConfig(format!("未知的告警级别: {}", alarm_level_str))
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

        let notification_config = NotificationConfig::default();
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

#[async_trait]
impl AlarmRuleRepository for SqliteAlarmRuleRepository {
    async fn create(&self, rule: &AlarmRule) -> AlarmResult<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| AlarmError::InternalError(format!("序列化条件配置失败: {}", e)))?;

        let device_id = rule.device_id.as_ref().filter(|s| !s.is_empty());
        let property_id = rule.property_id.as_ref().filter(|s| !s.is_empty());

        let query = r#"
            INSERT INTO device_alarm_rules (
                id, device_id, property_id, rule_name, rule_type,
                condition_config, alarm_level, is_enabled, description,
                workspace_id, created_by, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
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
            .bind(&rule.workspace_id)
            .bind(rule.created_at.to_rfc3339())
            .bind(rule.updated_at.to_rfc3339())
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("创建规则失败: {}", e)))?;

        Ok(())
    }

    async fn update(&self, rule: &AlarmRule, workspace_id: Option<&str>) -> AlarmResult<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| AlarmError::InternalError(format!("序列化条件配置失败: {}", e)))?;

        let query = if workspace_id.is_some() {
            r#"
            UPDATE device_alarm_rules SET
                rule_name = ?,
                rule_type = ?,
                condition_config = ?,
                alarm_level = ?,
                is_enabled = ?,
                description = ?,
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
            .bind(rule.updated_at.to_rfc3339())
            .bind(&rule.id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        sqlx_query
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("更新规则失败: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str, workspace_id: Option<&str>) -> AlarmResult<()> {
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
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("删除规则失败: {}", e)))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> AlarmResult<Option<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE id = ?";
        let row = sqlx::query(query)
            .bind(id)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询规则失败: {}", e)))?;

        if let Some(row) = row { Ok(Some(self.row_to_alarm_rule(row)?)) } else { Ok(None) }
    }

    async fn find_enabled(&self, workspace_id: Option<&str>) -> AlarmResult<Vec<AlarmRule>> {
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
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询启用规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    async fn find_by_device(
        &self,
        device_id: &str,
        workspace_id: Option<&str>,
    ) -> AlarmResult<Vec<AlarmRule>> {
        let (query, bind_ws) = if let Some(ws) = workspace_id {
            (
                "SELECT * FROM device_alarm_rules WHERE device_id = ? AND workspace_id = ? ORDER BY created_at DESC",
                Some(ws),
            )
        } else {
            ("SELECT * FROM device_alarm_rules WHERE device_id = ? ORDER BY created_at DESC", None)
        };
        let mut sqlx_query = sqlx::query(query).bind(device_id);
        if let Some(ws) = bind_ws {
            sqlx_query = sqlx_query.bind(ws);
        }
        let rows = sqlx_query
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询设备规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    async fn find_by_property(
        &self,
        device_id: &str,
        property_id: &str,
    ) -> AlarmResult<Vec<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE device_id = ? AND property_id = ? ORDER BY created_at DESC";
        let rows = sqlx::query(query)
            .bind(device_id)
            .bind(property_id)
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询属性规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    async fn find_global_rules(&self) -> AlarmResult<Vec<AlarmRule>> {
        let query =
            "SELECT * FROM device_alarm_rules WHERE device_id IS NULL ORDER BY created_at DESC";
        let rows = sqlx::query(query)
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询全局规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }
        Ok(rules)
    }

    async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        workspace_id: Option<&str>,
    ) -> AlarmResult<()> {
        let query = if workspace_id.is_some() {
            "UPDATE device_alarm_rules SET is_enabled = ?, updated_at = ? WHERE id = ? AND workspace_id = ?"
        } else {
            "UPDATE device_alarm_rules SET is_enabled = ?, updated_at = ? WHERE id = ?"
        };
        let mut sqlx_query =
            sqlx::query(query).bind(enabled).bind(Utc::now().to_rfc3339()).bind(id);
        if let Some(ws) = workspace_id {
            sqlx_query = sqlx_query.bind(ws);
        }
        sqlx_query
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("更新规则状态失败: {}", e)))?;
        Ok(())
    }
}
