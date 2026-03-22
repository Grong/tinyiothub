use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};

use crate::infrastructure::persistence::database::Database;

/// Device event trigger entity - 设备事件触发器实体
///
/// 使用 SQLx 最佳实践:
/// - 使用 snake_case 字段名映射到 PascalCase 数据库列
/// - 使用类型安全的查询构建
/// - 使用事务确保数据一致性
/// - 支持复杂的触发器逻辑和条件
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceEventTrigger {
    pub id: String,
    pub trigger: String, // JSON string containing trigger conditions
    pub action_type: Option<i32>,
    pub target_id: Option<String>,
    pub args: Option<String>, // JSON string containing action arguments
    pub is_enable: i32,       // SQLite uses INTEGER for boolean
    pub action_level: Option<i32>,
    pub created_at: String,
}

/// Query parameters for device event trigger search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceEventTriggerQueryParams {
    pub action_type: Option<i32>,
    pub target_id: Option<String>,
    pub is_enable: Option<bool>,
    pub action_level: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new device event trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceEventTriggerRequest {
    pub trigger: String, // JSON string containing trigger conditions
    pub action_type: Option<i32>,
    pub target_id: Option<String>,
    pub args: Option<String>, // JSON string containing action arguments
    pub is_enable: Option<bool>,
    pub action_level: Option<i32>,
}

/// Request for updating a device event trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceEventTriggerRequest {
    pub trigger: Option<String>,
    pub action_type: Option<i32>,
    pub target_id: Option<String>,
    pub args: Option<String>,
    pub is_enable: Option<bool>,
    pub action_level: Option<i32>,
}

/// Device event trigger statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceEventTriggerStatistics {
    pub total_triggers: i64,
    pub enabled_triggers: i64,
    pub disabled_triggers: i64,
    pub triggers_by_action_type: Vec<ActionTypeCount>,
}

/// Action type count for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ActionTypeCount {
    pub action_type: i32,
    pub count: i64,
}

impl DeviceEventTrigger {
    /// Find a device event trigger by ID
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<DeviceEventTrigger>, sqlx::Error> {
        let trigger = sqlx::query_as::<_, DeviceEventTrigger>(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(trigger)
    }

    /// Create a new device event trigger
    pub async fn create(
        db: &Database,
        request: &CreateDeviceEventTriggerRequest,
    ) -> Result<DeviceEventTrigger, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let is_enable = if request.is_enable.unwrap_or(true) { 1 } else { 0 };

        // Use transaction for data consistency
        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO DeviceEventTriggers (id, trigger, action_type, target_id, args, is_enable, action_level, CreatedAt)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(&request.trigger)
        .bind(request.action_type)
        .bind(&request.target_id)
        .bind(&request.args)
        .bind(is_enable)
        .bind(request.action_level)
        .bind(&created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Return the created trigger
        Self::find_by_id(db, &id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Update a device event trigger
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateDeviceEventTriggerRequest,
    ) -> Result<DeviceEventTrigger, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new("UPDATE DeviceEventTriggers SET ");
        let mut has_updates = false;

        if let Some(trigger) = &request.trigger {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("trigger = ").push_bind(trigger);
            has_updates = true;
        }

        if let Some(action_type) = request.action_type {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("action_type = ").push_bind(action_type);
            has_updates = true;
        }

        if let Some(target_id) = &request.target_id {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("target_id = ").push_bind(target_id);
            has_updates = true;
        }

        if let Some(args) = &request.args {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("args = ").push_bind(args);
            has_updates = true;
        }

        if let Some(is_enable) = request.is_enable {
            if has_updates {
                query_builder.push(", ");
            }
            let enable_value = if is_enable { 1 } else { 0 };
            query_builder.push("is_enable = ").push_bind(enable_value);
            has_updates = true;
        }

        if let Some(action_level) = request.action_level {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("action_level = ").push_bind(action_level);
            has_updates = true;
        }

        if !has_updates {
            return Err(sqlx::Error::RowNotFound);
        }

        query_builder.push(" WHERE id = ").push_bind(id);

        let mut tx = db.pool().begin().await?;
        query_builder.build().execute(&mut *tx).await?;
        tx.commit().await?;

        // Return the updated trigger
        Self::find_by_id(db, id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Delete a device event trigger
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM DeviceEventTriggers WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Find all device event triggers with optional filtering
    pub async fn find_all(
        db: &Database,
        params: &DeviceEventTriggerQueryParams,
    ) -> Result<Vec<DeviceEventTrigger>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE 1=1
            "#,
        );

        if let Some(action_type) = params.action_type {
            query_builder.push(" AND action_type = ").push_bind(action_type);
        }

        if let Some(target_id) = &params.target_id {
            query_builder.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(is_enable) = params.is_enable {
            let enable_value = if is_enable { 1 } else { 0 };
            query_builder.push(" AND is_enable = ").push_bind(enable_value);
        }

        if let Some(action_level) = params.action_level {
            query_builder.push(" AND action_level = ").push_bind(action_level);
        }

        query_builder.push(" ORDER BY created_at DESC");

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let triggers =
            query_builder.build_query_as::<DeviceEventTrigger>().fetch_all(db.pool()).await?;

        Ok(triggers)
    }

    /// Count device event triggers with optional filtering
    pub async fn count(
        db: &Database,
        params: &DeviceEventTriggerQueryParams,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM DeviceEventTriggers WHERE 1=1");

        if let Some(action_type) = params.action_type {
            query_builder.push(" AND action_type = ").push_bind(action_type);
        }

        if let Some(target_id) = &params.target_id {
            query_builder.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(is_enable) = params.is_enable {
            let enable_value = if is_enable { 1 } else { 0 };
            query_builder.push(" AND is_enable = ").push_bind(enable_value);
        }

        if let Some(action_level) = params.action_level {
            query_builder.push(" AND action_level = ").push_bind(action_level);
        }

        let row = query_builder.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }

    /// Find enabled triggers
    pub async fn find_enabled(db: &Database) -> Result<Vec<DeviceEventTrigger>, sqlx::Error> {
        let triggers = sqlx::query_as::<_, DeviceEventTrigger>(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE is_enable = 1
            ORDER BY action_level ASC, created_at ASC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(triggers)
    }

    /// Find triggers by action type
    pub async fn find_by_action_type(
        db: &Database,
        action_type: i32,
    ) -> Result<Vec<DeviceEventTrigger>, sqlx::Error> {
        let triggers = sqlx::query_as::<_, DeviceEventTrigger>(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE action_type = ? AND is_enable = 1
            ORDER BY action_level ASC, created_at ASC
            "#,
        )
        .bind(action_type)
        .fetch_all(db.pool())
        .await?;

        Ok(triggers)
    }

    /// Find triggers by target ID
    pub async fn find_by_target_id(
        db: &Database,
        target_id: &str,
    ) -> Result<Vec<DeviceEventTrigger>, sqlx::Error> {
        let triggers = sqlx::query_as::<_, DeviceEventTrigger>(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE target_id = ?
            ORDER BY action_level ASC, created_at ASC
            "#,
        )
        .bind(target_id)
        .fetch_all(db.pool())
        .await?;

        Ok(triggers)
    }

    /// Enable/disable trigger
    pub async fn set_enable_status(
        db: &Database,
        id: &str,
        is_enable: bool,
    ) -> Result<DeviceEventTrigger, sqlx::Error> {
        let enable_value = if is_enable { 1 } else { 0 };

        let mut tx = db.pool().begin().await?;

        sqlx::query("UPDATE DeviceEventTriggers SET is_enable = ? WHERE id = ?")
            .bind(enable_value)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Self::find_by_id(db, id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Batch enable/disable triggers
    pub async fn batch_set_enable_status(
        db: &Database,
        ids: &[String],
        is_enable: bool,
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let enable_value = if is_enable { 1 } else { 0 };
        let mut tx = db.pool().begin().await?;

        let mut query_builder =
            QueryBuilder::<Sqlite>::new("UPDATE DeviceEventTriggers SET is_enable = ");
        query_builder.push_bind(enable_value);
        query_builder.push(" WHERE id IN (");

        let mut separated = query_builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let result = query_builder.build().execute(&mut *tx).await?;
        tx.commit().await?;

        Ok(result.rows_affected())
    }

    /// Delete triggers by target ID
    pub async fn delete_by_target_id(db: &Database, target_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM DeviceEventTriggers WHERE target_id = ?")
            .bind(target_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Get trigger statistics
    pub async fn get_statistics(
        db: &Database,
    ) -> Result<DeviceEventTriggerStatistics, sqlx::Error> {
        let total_row =
            sqlx::query("SELECT COUNT(*) FROM DeviceEventTriggers").fetch_one(db.pool()).await?;

        let enabled_row =
            sqlx::query("SELECT COUNT(*) FROM DeviceEventTriggers WHERE is_enable = 1")
                .fetch_one(db.pool())
                .await?;

        let disabled_row =
            sqlx::query("SELECT COUNT(*) FROM DeviceEventTriggers WHERE is_enable = 0")
                .fetch_one(db.pool())
                .await?;

        // Get counts by action type
        let action_type_rows = sqlx::query(
            "SELECT action_type, COUNT(*) as count FROM DeviceEventTriggers WHERE action_type IS NOT NULL GROUP BY ActionType"
        )
        .fetch_all(db.pool())
        .await?;

        let triggers_by_action_type = action_type_rows
            .into_iter()
            .map(|row| ActionTypeCount {
                action_type: row.get("ActionType"),
                count: row.get("count"),
            })
            .collect();

        Ok(DeviceEventTriggerStatistics {
            total_triggers: total_row.get(0),
            enabled_triggers: enabled_row.get(0),
            disabled_triggers: disabled_row.get(0),
            triggers_by_action_type,
        })
    }

    /// Find triggers with pagination and sorting
    pub async fn find_paginated(
        db: &Database,
        params: &DeviceEventTriggerQueryParams,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
    ) -> Result<(Vec<DeviceEventTrigger>, i64), sqlx::Error> {
        // Get total count first
        let total_count = Self::count(db, params).await?;

        // Build the main query
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, trigger, action_type, target_id, args, is_enable, action_level, created_at
            FROM DeviceEventTriggers WHERE 1=1
            "#,
        );

        if let Some(action_type) = params.action_type {
            query_builder.push(" AND action_type = ").push_bind(action_type);
        }

        if let Some(target_id) = &params.target_id {
            query_builder.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(is_enable) = params.is_enable {
            let enable_value = if is_enable { 1 } else { 0 };
            query_builder.push(" AND is_enable = ").push_bind(enable_value);
        }

        if let Some(action_level) = params.action_level {
            query_builder.push(" AND action_level = ").push_bind(action_level);
        }

        // Add sorting
        let sort_column = match sort_by {
            Some("actionType") => "ActionType",
            Some("actionLevel") => "ActionLevel",
            Some("createdAt") => "CreatedAt",
            Some("targetId") => "TargetId",
            _ => "CreatedAt",
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

        let triggers =
            query_builder.build_query_as::<DeviceEventTrigger>().fetch_all(db.pool()).await?;

        Ok((triggers, total_count))
    }

    /// Execute trigger (business logic method)
    pub async fn execute(&self, event_data: &str) -> Result<bool, String> {
        // Parse trigger conditions from JSON
        let trigger_conditions: serde_json::Value = serde_json::from_str(&self.trigger)
            .map_err(|e| format!("Failed to parse trigger conditions: {}", e))?;

        // Parse event data
        let event: serde_json::Value = serde_json::from_str(event_data)
            .map_err(|e| format!("Failed to parse event data: {}", e))?;

        // Evaluate trigger conditions against event data
        // This is a simplified implementation - in practice, you'd have more complex logic
        if let Some(condition_type) = trigger_conditions.get("type").and_then(|v| v.as_str()) {
            match condition_type {
                "property_change" => {
                    // Check if property value matches condition
                    if let (Some(property_name), Some(expected_value)) = (
                        trigger_conditions.get("property").and_then(|v| v.as_str()),
                        trigger_conditions.get("value"),
                    ) {
                        if let Some(actual_value) = event.get(property_name) {
                            return Ok(actual_value == expected_value);
                        }
                    }
                }
                "threshold" => {
                    // Check if numeric value exceeds threshold
                    if let (Some(property_name), Some(threshold)) = (
                        trigger_conditions.get("property").and_then(|v| v.as_str()),
                        trigger_conditions.get("threshold").and_then(|v| v.as_f64()),
                    ) {
                        if let Some(actual_value) =
                            event.get(property_name).and_then(|v| v.as_f64())
                        {
                            let operator = trigger_conditions
                                .get("operator")
                                .and_then(|v| v.as_str())
                                .unwrap_or("gt");
                            return Ok(match operator {
                                "gt" => actual_value > threshold,
                                "gte" => actual_value >= threshold,
                                "lt" => actual_value < threshold,
                                "lte" => actual_value <= threshold,
                                "eq" => (actual_value - threshold).abs() < f64::EPSILON,
                                _ => false,
                            });
                        }
                    }
                }
                _ => return Ok(false),
            }
        }

        Ok(false)
    }

    // Helper methods for business logic

    /// Check if trigger is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enable == 1
    }

    /// Get trigger priority (lower number = higher priority)
    pub fn get_priority(&self) -> i32 {
        self.action_level.unwrap_or(999)
    }

    /// Parse trigger conditions as JSON
    pub fn parse_trigger_conditions(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(&self.trigger)
    }

    /// Parse action arguments as JSON
    pub fn parse_action_args(&self) -> Result<Option<serde_json::Value>, serde_json::Error> {
        match &self.args {
            Some(args) => Ok(Some(serde_json::from_str(args)?)),
            None => Ok(None),
        }
    }

    /// Validate trigger configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate trigger JSON
        self.parse_trigger_conditions().map_err(|e| format!("Invalid trigger JSON: {}", e))?;

        // Validate args JSON if present
        if let Err(e) = self.parse_action_args() {
            return Err(format!("Invalid args JSON: {}", e));
        }

        // Validate action type
        if let Some(action_type) = self.action_type {
            if !(0..=100).contains(&action_type) {
                return Err("Action type must be between 0 and 100".to_string());
            }
        }

        // Validate action level
        if let Some(action_level) = self.action_level {
            if !(0..=10).contains(&action_level) {
                return Err("Action level must be between 0 and 10".to_string());
            }
        }

        Ok(())
    }
}
