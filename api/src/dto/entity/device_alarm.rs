use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};

use crate::infrastructure::persistence::database::Database;

/// Device alarm entity - 设备告警实体
///
/// 使用 SQLx 最佳实践:
/// - 使用 snake_case 字段名映射到 PascalCase 数据库列
/// - 使用类型安全的查询构建
/// - 使用事务确保数据一致性
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarm {
    pub id: String,
    pub device_id: String,
    pub property_id: String,
    pub rule_id: String,
    pub alarm_level: String, // "info", "warning", "error", "critical"
    pub alarm_message: String,
    pub alarm_value: Option<String>, // The value that triggered the alarm
    pub threshold_value: Option<String>, // The threshold that was exceeded
    pub alarm_time: String,
    pub is_acknowledged: i32, // SQLite uses INTEGER for boolean
    pub acknowledged_by: Option<String>,
    pub acknowledged_time: Option<String>,
    pub acknowledged_note: Option<String>,
    pub is_resolved: i32, // SQLite uses INTEGER for boolean
    pub resolved_time: Option<String>,
    pub created_at: String,
}

/// Query parameters for device alarm search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmQueryParams {
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_id: Option<String>,
    pub alarm_level: Option<String>,
    pub is_acknowledged: Option<bool>,
    pub is_resolved: Option<bool>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new device alarm
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceAlarmRequest {
    pub device_id: String,
    pub property_id: String,
    pub rule_id: String,
    pub alarm_level: String,
    pub alarm_message: String,
    pub alarm_value: Option<String>,
    pub threshold_value: Option<String>,
}

/// Request for acknowledging an alarm
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcknowledgeAlarmRequest {
    pub acknowledged_by: String,
    pub acknowledged_note: Option<String>,
}

/// Request for resolving an alarm
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolveAlarmRequest {
    pub resolved_by: String,
    pub resolution_note: Option<String>,
}

/// Device alarm statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmStatistics {
    pub total_alarms: i64,
    pub active_alarms: i64,
    pub acknowledged_alarms: i64,
    pub resolved_alarms: i64,
    pub critical_alarms: i64,
    pub warning_alarms: i64,
    pub info_alarms: i64,
}

impl DeviceAlarm {
    /// Find a device alarm by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<DeviceAlarm>, sqlx::Error> {
        let alarm = sqlx::query_as::<_, DeviceAlarm>(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(alarm)
    }

    /// Create a new device alarm
    pub async fn create(
        db: &Database,
        request: &CreateDeviceAlarmRequest,
    ) -> Result<DeviceAlarm, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Use transaction for data consistency
        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO DeviceAlarms (
                id, device_id, property_id, rule_id, alarm_level, alarm_message,
                alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                is_resolved, ResolvedTime, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.device_id)
        .bind(&request.property_id)
        .bind(&request.rule_id)
        .bind(&request.alarm_level)
        .bind(&request.alarm_message)
        .bind(&request.alarm_value)
        .bind(&request.threshold_value)
        .bind(&now)
        .bind(0) // is_acknowledged = false
        .bind(None::<String>) // acknowledged_by
        .bind(None::<String>) // acknowledged_time
        .bind(None::<String>) // acknowledged_note
        .bind(0) // is_resolved = false
        .bind(None::<String>) // resolved_time
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Return the created alarm
        Self::find_by_id(db, &id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Acknowledge an alarm
    pub async fn acknowledge(
        db: &Database,
        id: &str,
        request: &AcknowledgeAlarmRequest,
    ) -> Result<DeviceAlarm, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            UPDATE DeviceAlarms 
            SET is_acknowledged = 1, acknowledged_by = ?, AcknowledgedTime = ?, AcknowledgedNote = ?
            WHERE id = ?
            "#,
        )
        .bind(&request.acknowledged_by)
        .bind(&now)
        .bind(&request.acknowledged_note)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Return the updated alarm
        Self::find_by_id(db, id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Resolve an alarm
    pub async fn resolve(
        db: &Database,
        id: &str,
        request: &ResolveAlarmRequest,
    ) -> Result<DeviceAlarm, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = db.pool().begin().await?;

        // If not acknowledged, acknowledge it first
        sqlx::query(
            r#"
            UPDATE DeviceAlarms 
            SET is_resolved = 1, ResolvedTime = ?,
                is_acknowledged = CASE WHEN is_acknowledged = 0 THEN 1 ELSE is_acknowledged END,
                acknowledged_by = CASE WHEN acknowledged_by IS NULL THEN ? ELSE acknowledged_by END,
                AcknowledgedTime = CASE WHEN AcknowledgedTime IS NULL THEN ? ELSE AcknowledgedTime END,
                AcknowledgedNote = CASE WHEN AcknowledgedNote IS NULL THEN ? ELSE AcknowledgedNote END
            WHERE id = ?
            "#
        )
        .bind(&now)
        .bind(&request.resolved_by)
        .bind(&now)
        .bind(&request.resolution_note)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Return the updated alarm
        Self::find_by_id(db, id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Delete an alarm
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM DeviceAlarms WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Find all device alarms with optional filtering
    pub async fn find_all(
        db: &Database,
        params: &DeviceAlarmQueryParams,
    ) -> Result<Vec<DeviceAlarm>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms WHERE 1=1
            "#,
        );

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(property_id) = &params.property_id {
            query_builder.push(" AND property_id = ").push_bind(property_id);
        }

        if let Some(rule_id) = &params.rule_id {
            query_builder.push(" AND rule_id = ").push_bind(rule_id);
        }

        if let Some(alarm_level) = &params.alarm_level {
            query_builder.push(" AND alarm_level = ").push_bind(alarm_level);
        }

        if let Some(is_acknowledged) = params.is_acknowledged {
            let ack_value = if is_acknowledged { 1 } else { 0 };
            query_builder.push(" AND is_acknowledged = ").push_bind(ack_value);
        }

        if let Some(is_resolved) = params.is_resolved {
            let resolved_value = if is_resolved { 1 } else { 0 };
            query_builder.push(" AND is_resolved = ").push_bind(resolved_value);
        }

        if let Some(start_time) = &params.start_time {
            query_builder.push(" AND alarm_time >= ").push_bind(start_time);
        }

        if let Some(end_time) = &params.end_time {
            query_builder.push(" AND alarm_time <= ").push_bind(end_time);
        }

        query_builder.push(" ORDER BY alarm_time DESC");

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let alarms = query_builder.build_query_as::<DeviceAlarm>().fetch_all(db.pool()).await?;

        Ok(alarms)
    }

    /// Count device alarms with optional filtering
    pub async fn count(db: &Database, params: &DeviceAlarmQueryParams) -> Result<i64, sqlx::Error> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM DeviceAlarms WHERE 1=1");

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(property_id) = &params.property_id {
            query_builder.push(" AND property_id = ").push_bind(property_id);
        }

        if let Some(rule_id) = &params.rule_id {
            query_builder.push(" AND rule_id = ").push_bind(rule_id);
        }

        if let Some(alarm_level) = &params.alarm_level {
            query_builder.push(" AND alarm_level = ").push_bind(alarm_level);
        }

        if let Some(is_acknowledged) = params.is_acknowledged {
            let ack_value = if is_acknowledged { 1 } else { 0 };
            query_builder.push(" AND is_acknowledged = ").push_bind(ack_value);
        }

        if let Some(is_resolved) = params.is_resolved {
            let resolved_value = if is_resolved { 1 } else { 0 };
            query_builder.push(" AND is_resolved = ").push_bind(resolved_value);
        }

        if let Some(start_time) = &params.start_time {
            query_builder.push(" AND alarm_time >= ").push_bind(start_time);
        }

        if let Some(end_time) = &params.end_time {
            query_builder.push(" AND alarm_time <= ").push_bind(end_time);
        }

        let row = query_builder.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }

    /// Find alarms by device ID
    pub async fn find_by_device_id(
        db: &Database,
        device_id: &str,
    ) -> Result<Vec<DeviceAlarm>, sqlx::Error> {
        let alarms = sqlx::query_as::<_, DeviceAlarm>(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms WHERE device_id = ?
            ORDER BY alarm_time DESC
            "#,
        )
        .bind(device_id)
        .fetch_all(db.pool())
        .await?;

        Ok(alarms)
    }

    /// Find active (unresolved) alarms
    pub async fn find_active_alarms(db: &Database) -> Result<Vec<DeviceAlarm>, sqlx::Error> {
        let alarms = sqlx::query_as::<_, DeviceAlarm>(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms WHERE is_resolved = 0
            ORDER BY alarm_time DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(alarms)
    }

    /// Find critical alarms that need attention
    pub async fn find_critical_unacknowledged(
        db: &Database,
    ) -> Result<Vec<DeviceAlarm>, sqlx::Error> {
        let alarms = sqlx::query_as::<_, DeviceAlarm>(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms 
            WHERE alarm_level = 'critical' AND is_acknowledged = 0 AND is_resolved = 0
            ORDER BY alarm_time DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(alarms)
    }

    /// Get alarm statistics
    pub async fn get_statistics(db: &Database) -> Result<DeviceAlarmStatistics, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_alarms,
                COUNT(CASE WHEN is_resolved = 0 THEN 1 END) as active_alarms,
                COUNT(CASE WHEN is_acknowledged = 1 THEN 1 END) as acknowledged_alarms,
                COUNT(CASE WHEN is_resolved = 1 THEN 1 END) as resolved_alarms,
                COUNT(CASE WHEN alarm_level = 'critical' THEN 1 END) as critical_alarms,
                COUNT(CASE WHEN alarm_level = 'warning' THEN 1 END) as warning_alarms,
                COUNT(CASE WHEN alarm_level = 'info' THEN 1 END) as info_alarms
            FROM DeviceAlarms
            "#,
        )
        .fetch_one(db.pool())
        .await?;

        Ok(DeviceAlarmStatistics {
            total_alarms: row.get("total_alarms"),
            active_alarms: row.get("active_alarms"),
            acknowledged_alarms: row.get("acknowledged_alarms"),
            resolved_alarms: row.get("resolved_alarms"),
            critical_alarms: row.get("critical_alarms"),
            warning_alarms: row.get("warning_alarms"),
            info_alarms: row.get("info_alarms"),
        })
    }

    /// Delete all alarms for a device
    pub async fn delete_by_device_id(db: &Database, device_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM DeviceAlarms WHERE device_id = ?")
            .bind(device_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Batch acknowledge alarms
    pub async fn batch_acknowledge(
        db: &Database,
        alarm_ids: &[String],
        request: &AcknowledgeAlarmRequest,
    ) -> Result<u64, sqlx::Error> {
        if alarm_ids.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut tx = db.pool().begin().await?;

        let mut query_builder = QueryBuilder::<Sqlite>::new(
            "UPDATE DeviceAlarms SET is_acknowledged = 1, acknowledged_by = ",
        );
        query_builder.push_bind(&request.acknowledged_by);
        query_builder.push(", AcknowledgedTime = ");
        query_builder.push_bind(&now);
        query_builder.push(", AcknowledgedNote = ");
        query_builder.push_bind(&request.acknowledged_note);
        query_builder.push(" WHERE id IN (");

        let mut separated = query_builder.separated(", ");
        for id in alarm_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let result = query_builder.build().execute(&mut *tx).await?;
        tx.commit().await?;

        Ok(result.rows_affected())
    }

    /// Find alarms with pagination and sorting
    pub async fn find_paginated(
        db: &Database,
        params: &DeviceAlarmQueryParams,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
    ) -> Result<(Vec<DeviceAlarm>, i64), sqlx::Error> {
        // Get total count first
        let total_count = Self::count(db, params).await?;

        // Build the main query
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
                   alarm_value, ThresholdValue, alarm_time, is_acknowledged,
                   acknowledged_by, AcknowledgedTime, AcknowledgedNote,
                   is_resolved, ResolvedTime, created_at
            FROM DeviceAlarms WHERE 1=1
            "#,
        );

        if let Some(device_id) = &params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }

        if let Some(property_id) = &params.property_id {
            query_builder.push(" AND property_id = ").push_bind(property_id);
        }

        if let Some(rule_id) = &params.rule_id {
            query_builder.push(" AND rule_id = ").push_bind(rule_id);
        }

        if let Some(alarm_level) = &params.alarm_level {
            query_builder.push(" AND alarm_level = ").push_bind(alarm_level);
        }

        if let Some(is_acknowledged) = params.is_acknowledged {
            let ack_value = if is_acknowledged { 1 } else { 0 };
            query_builder.push(" AND is_acknowledged = ").push_bind(ack_value);
        }

        if let Some(is_resolved) = params.is_resolved {
            let resolved_value = if is_resolved { 1 } else { 0 };
            query_builder.push(" AND is_resolved = ").push_bind(resolved_value);
        }

        if let Some(start_time) = &params.start_time {
            query_builder.push(" AND alarm_time >= ").push_bind(start_time);
        }

        if let Some(end_time) = &params.end_time {
            query_builder.push(" AND alarm_time <= ").push_bind(end_time);
        }

        // Add sorting
        let sort_column = match sort_by {
            Some("alarmLevel") => "AlarmLevel",
            Some("alarmTime") => "AlarmTime",
            Some("deviceId") => "DeviceId",
            Some("createdAt") => "CreatedAt",
            _ => "AlarmTime",
        };

        let sort_direction = match sort_order {
            Some("asc") => "ASC",
            _ => "DESC",
        };

        query_builder.push(format!(" ORDER BY {} {}", sort_column, sort_direction));

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let alarms = query_builder.build_query_as::<DeviceAlarm>().fetch_all(db.pool()).await?;

        Ok((alarms, total_count))
    }

    // Helper methods for business logic

    /// Check if alarm is critical
    pub fn is_critical(&self) -> bool {
        self.alarm_level == "critical"
    }

    /// Check if alarm is active (not resolved)
    pub fn is_active(&self) -> bool {
        self.is_resolved == 0
    }

    /// Check if alarm is acknowledged
    pub fn is_acknowledged(&self) -> bool {
        self.is_acknowledged == 1
    }

    /// Get alarm priority (higher number = higher priority)
    pub fn get_alarm_priority(&self) -> u8 {
        match self.alarm_level.as_str() {
            "info" => 1,
            "warning" => 2,
            "error" => 3,
            "critical" => 4,
            _ => 0,
        }
    }

    /// Get alarm age in seconds
    pub fn get_alarm_age_seconds(&self) -> i64 {
        let now = chrono::Utc::now();
        if let Ok(alarm_time) =
            chrono::NaiveDateTime::parse_from_str(&self.alarm_time, "%Y-%m-%d %H:%M:%S")
        {
            let alarm_time_utc =
                chrono::DateTime::from_naive_utc_and_offset(alarm_time, chrono::Utc);
            (now - alarm_time_utc).num_seconds()
        } else {
            0
        }
    }

    /// Check if alarm needs attention (critical and not acknowledged)
    pub fn needs_attention(&self) -> bool {
        self.is_critical() && !self.is_acknowledged() && self.is_active()
    }
}

// Backward compatibility
pub type DeviceAlarmDto = DeviceAlarm;
pub type DeviceAlarmQuery = DeviceAlarmQueryParams;
