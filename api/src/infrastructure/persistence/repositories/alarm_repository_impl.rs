use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    domain::alarm::{
        Alarm, AlarmError, AlarmQueryCriteria, AlarmRepository, AlarmResult, AlarmRule,
        AlarmRuleRepository, AlarmStatus,
    },
    infrastructure::persistence::Database,
};

/// 报警仓储实现
pub struct AlarmRepositoryImpl {
    database: Arc<Database>,
}

impl AlarmRepositoryImpl {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl AlarmRepository for AlarmRepositoryImpl {
    async fn create(&self, alarm: &Alarm) -> AlarmResult<()> {
        let query = r#"
            INSERT INTO device_alarms (
                id, device_id, property_id, rule_id, alarm_level, 
                alarm_message, alarm_value, threshold_value, alarm_time,
                is_acknowledged, acknowledged_by, acknowledged_at, acknowledged_note,
                is_resolved, resolved_by, resolved_at, resolved_note, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                resolved_note = ?
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
            .bind(&alarm.id)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> AlarmResult<Option<Alarm>> {
        let query = "SELECT * FROM device_alarms WHERE id = ?";

        let row = sqlx::query(query).bind(id).fetch_optional(self.database.pool()).await?;

        if let Some(row) = row {
            Ok(Some(self.row_to_alarm(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>> {
        let mut query = String::from("SELECT * FROM device_alarms WHERE 1=1");

        if let Some(device_ids) = &criteria.device_ids {
            if !device_ids.is_empty() {
                let placeholders = vec!["?"; device_ids.len()].join(",");
                query.push_str(&format!(" AND device_id IN ({})", placeholders));
            }
        }

        if let Some(levels) = &criteria.alarm_levels {
            if !levels.is_empty() {
                let placeholders = vec!["?"; levels.len()].join(",");
                query.push_str(&format!(" AND alarm_level IN ({})", placeholders));
            }
        }

        if let Some(statuses) = &criteria.statuses {
            if !statuses.is_empty() {
                let mut status_conditions: Vec<&str> = Vec::new();
                for status in statuses {
                    match status {
                        AlarmStatus::Active => {
                            status_conditions
                                .push("(is_resolved = false AND is_acknowledged = false)");
                        }
                        AlarmStatus::Acknowledged => {
                            status_conditions
                                .push("(is_resolved = false AND is_acknowledged = true)");
                        }
                        AlarmStatus::Resolved => {
                            status_conditions.push("is_resolved = true");
                        }
                        AlarmStatus::Suppressed => {
                            // 暂时不支持抑制状态查询
                        }
                    }
                }
                if !status_conditions.is_empty() {
                    query.push_str(&format!(" AND ({})", status_conditions.join(" OR ")));
                }
            }
        }

        if let Some(_time_range) = &criteria.time_range {
            query.push_str(" AND alarm_time >= ? AND alarm_time <= ?");
        }

        query.push_str(" ORDER BY alarm_time DESC");

        if let Some(limit) = criteria.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = criteria.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        // 由于 SQLx 的限制，这里简化返回空列表
        // 实际项目中需要使用动态查询构建器
        // TODO: find_by_criteria is not yet implemented - returning empty vector
        // This method needs proper implementation before production use
        todo!("find_by_criteria is not yet implemented - returning empty vector")
    }

    async fn find_active(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
        let _query = if device_id.is_some() {
            "SELECT * FROM device_alarms WHERE is_resolved = false AND device_id = ? ORDER BY alarm_time DESC"
        } else {
            "SELECT * FROM device_alarms WHERE is_resolved = false ORDER BY alarm_time DESC"
        };

        // TODO: find_active is not yet implemented - returning empty vector
        // This method needs proper implementation before production use
        todo!("find_active is not yet implemented - returning empty vector")
    }

    async fn find_unacknowledged(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
        let _query = if device_id.is_some() {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false AND device_id = ? ORDER BY alarm_time DESC"
        } else {
            "SELECT * FROM device_alarms WHERE is_acknowledged = false AND is_resolved = false ORDER BY alarm_time DESC"
        };

        // TODO: find_unacknowledged is not yet implemented - returning empty vector
        // This method needs proper implementation before production use
        todo!("find_unacknowledged is not yet implemented - returning empty vector")
    }

    async fn count_by_criteria(&self, criteria: &AlarmQueryCriteria) -> AlarmResult<u64> {
        let mut query = String::from("SELECT COUNT(*) as count FROM device_alarms WHERE 1=1");

        if let Some(device_ids) = &criteria.device_ids {
            if !device_ids.is_empty() {
                let placeholders = vec!["?"; device_ids.len()].join(",");
                query.push_str(&format!(" AND device_id IN ({})", placeholders));
            }
        }

        if let Some(_time_range) = &criteria.time_range {
            query.push_str(" AND alarm_time >= ? AND alarm_time <= ?");
        }

        // TODO: count_by_criteria is not yet implemented - returning 0
        // This method needs proper implementation before production use
        todo!("count_by_criteria is not yet implemented - returning 0")
    }

    async fn batch_update_status(
        &self,
        alarm_ids: &[String],
        status: AlarmStatus,
    ) -> AlarmResult<usize> {
        if alarm_ids.is_empty() {
            return Ok(0);
        }

        let (_is_resolved, _is_acknowledged) = match status {
            AlarmStatus::Active => (false, false),
            AlarmStatus::Acknowledged => (false, true),
            AlarmStatus::Resolved => (true, true),
            AlarmStatus::Suppressed => return Ok(0), // 暂不支持
        };

        let placeholders = vec!["?"; alarm_ids.len()].join(",");
        let _query = format!(
            "UPDATE device_alarms SET is_resolved = ?, is_acknowledged = ? WHERE id IN ({})",
            placeholders
        );

        // TODO: batch_update_status is not yet implemented - returning 0
        // This method needs proper implementation before production use
        todo!("batch_update_status is not yet implemented - returning 0")
    }

    async fn delete_old_alarms(&self, before: DateTime<Utc>) -> AlarmResult<usize> {
        let query = "DELETE FROM device_alarms WHERE created_at < ? AND is_resolved = true";

        let result =
            sqlx::query(query).bind(before.to_rfc3339()).execute(self.database.pool()).await?;

        Ok(result.rows_affected() as usize)
    }
}

impl AlarmRepositoryImpl {
    fn row_to_alarm(&self, _row: sqlx::sqlite::SqliteRow) -> AlarmResult<Alarm> {
        // 简化实现，需要完整的字段映射
        Err(AlarmError::InternalError("Not implemented".to_string()))
    }
}

/// 报警规则仓储实现
pub struct AlarmRuleRepositoryImpl {
    database: Arc<Database>,
}

impl AlarmRuleRepositoryImpl {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    fn row_to_alarm_rule(&self, row: sqlx::sqlite::SqliteRow) -> AlarmResult<AlarmRule> {
        use sqlx::Row;

        use crate::domain::alarm::{
            entity::RuleType,
            value_objects::{AlarmCondition, AlarmLevel, NotificationConfig},
        };

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

        // 解析 rule_type
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
                )))
            }
        };

        // 解析 condition
        let condition: AlarmCondition = serde_json::from_str(&condition_json)
            .map_err(|e| AlarmError::InvalidCondition(format!("解析条件配置失败: {}", e)))?;

        // 解析 alarm_level
        let alarm_level = AlarmLevel::from_str(&alarm_level_str).ok_or_else(|| {
            AlarmError::InvalidRuleConfig(format!("未知的告警级别: {}", alarm_level_str))
        })?;

        // 解析时间
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| AlarmError::InternalError(format!("解析创建时间失败: {}", e)))?
            .with_timezone(&Utc);
        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map_err(|e| AlarmError::InternalError(format!("解析更新时间失败: {}", e)))?
            .with_timezone(&Utc);

        // 从数据库读取通知配置（如果有单独的表）
        // 这里暂时使用默认配置，实际应该从关联表读取
        let notification_config = NotificationConfig::default();

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
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl AlarmRuleRepository for AlarmRuleRepositoryImpl {
    async fn create(&self, rule: &AlarmRule) -> AlarmResult<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| AlarmError::InternalError(format!("序列化条件配置失败: {}", e)))?;

        // 将空字符串转换为 None，避免外键约束失败
        let device_id = rule.device_id.as_ref().filter(|s| !s.is_empty());
        let property_id = rule.property_id.as_ref().filter(|s| !s.is_empty());

        let query = r#"
            INSERT INTO device_alarm_rules (
                id, device_id, property_id, rule_name, rule_type, 
                condition_config, alarm_level, is_enabled, description,
                created_by, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
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
            .bind(rule.created_at.to_rfc3339())
            .bind(rule.updated_at.to_rfc3339())
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("创建规则失败: {}", e)))?;

        Ok(())
    }

    async fn update(&self, rule: &AlarmRule) -> AlarmResult<()> {
        let condition_json = serde_json::to_string(&rule.condition)
            .map_err(|e| AlarmError::InternalError(format!("序列化条件配置失败: {}", e)))?;

        let query = r#"
            UPDATE device_alarm_rules SET
                rule_name = ?,
                rule_type = ?,
                condition_config = ?,
                alarm_level = ?,
                is_enabled = ?,
                description = ?,
                updated_at = ?
            WHERE id = ?
        "#;

        sqlx::query(query)
            .bind(&rule.name)
            .bind(rule.rule_type.as_str())
            .bind(&condition_json)
            .bind(rule.alarm_level.as_str())
            .bind(rule.is_enabled)
            .bind(&rule.description)
            .bind(rule.updated_at.to_rfc3339())
            .bind(&rule.id)
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("更新规则失败: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> AlarmResult<()> {
        let query = "DELETE FROM device_alarm_rules WHERE id = ?";

        sqlx::query(query)
            .bind(id)
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

        if let Some(row) = row {
            Ok(Some(self.row_to_alarm_rule(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_enabled(&self) -> AlarmResult<Vec<AlarmRule>> {
        let query =
            "SELECT * FROM device_alarm_rules WHERE is_enabled = true ORDER BY created_at DESC";

        let rows = sqlx::query(query)
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("查询启用规则失败: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(self.row_to_alarm_rule(row)?);
        }

        Ok(rules)
    }

    async fn find_by_device(&self, device_id: &str) -> AlarmResult<Vec<AlarmRule>> {
        let query = "SELECT * FROM device_alarm_rules WHERE device_id = ? ORDER BY created_at DESC";

        let rows = sqlx::query(query)
            .bind(device_id)
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

    async fn set_enabled(&self, id: &str, enabled: bool) -> AlarmResult<()> {
        let query = "UPDATE device_alarm_rules SET is_enabled = ?, updated_at = ? WHERE id = ?";

        sqlx::query(query)
            .bind(enabled)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::InternalError(format!("更新规则状态失败: {}", e)))?;

        Ok(())
    }
}
