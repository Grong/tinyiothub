use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};

use crate::infrastructure::persistence::database::Database;

/// Device alarm rule entity - 设备告警规则实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmRule {
    pub id: String,
    pub device_id: String,
    pub property_id: String,
    pub rule_name: String,
    pub rule_type: String,   // "threshold", "range", "change"
    pub condition: String,   // JSON string with condition details
    pub alarm_level: String, // "info", "warning", "error", "critical"
    pub is_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub description: Option<String>,
}

/// Query parameters for device alarm rule search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAlarmRuleQuery {
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub alarm_level: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new device alarm rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceAlarmRuleRequest {
    pub device_id: String,
    pub property_id: String,
    pub rule_name: String,
    pub rule_type: String,
    pub condition: String,
    pub alarm_level: String,
    pub is_enabled: Option<bool>,
    pub description: Option<String>,
}

/// Request for updating a device alarm rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceAlarmRuleRequest {
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub condition: Option<String>,
    pub alarm_level: Option<String>,
    pub is_enabled: Option<bool>,
    pub description: Option<String>,
}

impl DeviceAlarmRule {
    /// Find all alarm rules with pagination
    pub async fn find_all(
        db: &Database,
        params: &DeviceAlarmRuleQuery,
    ) -> Result<Vec<DeviceAlarmRule>, sqlx::Error> {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT id, device_id, property_id, rule_name, rule_type, condition, alarm_level, is_enabled, created_at, updated_at, description FROM device_alarm_rules WHERE 1=1",
        );

        if let Some(ref device_id) = params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }
        if let Some(ref property_id) = params.property_id {
            query_builder.push(" AND property_id = ").push_bind(property_id);
        }
        if let Some(ref rule_name) = params.rule_name {
            query_builder.push(" AND rule_name LIKE ").push_bind(format!("%{}%", rule_name));
        }
        if let Some(ref rule_type) = params.rule_type {
            query_builder.push(" AND rule_type = ").push_bind(rule_type);
        }
        if let Some(ref alarm_level) = params.alarm_level {
            query_builder.push(" AND alarm_level = ").push_bind(alarm_level);
        }
        if let Some(is_enabled) = params.is_enabled {
            query_builder.push(" AND is_enabled = ").push_bind(if is_enabled { 1 } else { 0 });
        }

        query_builder.push(" ORDER BY created_at DESC");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size as i64);
            query_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let rules = query_builder.build_query_as::<DeviceAlarmRule>().fetch_all(db.pool()).await?;
        Ok(rules)
    }

    /// Count alarm rules with filters
    pub async fn count(
        db: &Database,
        params: &DeviceAlarmRuleQuery,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT COUNT(*) FROM device_alarm_rules WHERE 1=1",
        );

        if let Some(ref device_id) = params.device_id {
            query_builder.push(" AND device_id = ").push_bind(device_id);
        }
        if let Some(ref property_id) = params.property_id {
            query_builder.push(" AND property_id = ").push_bind(property_id);
        }
        if let Some(ref rule_name) = params.rule_name {
            query_builder.push(" AND rule_name LIKE ").push_bind(format!("%{}%", rule_name));
        }
        if let Some(ref rule_type) = params.rule_type {
            query_builder.push(" AND rule_type = ").push_bind(rule_type);
        }
        if let Some(ref alarm_level) = params.alarm_level {
            query_builder.push(" AND alarm_level = ").push_bind(alarm_level);
        }
        if let Some(is_enabled) = params.is_enabled {
            query_builder.push(" AND is_enabled = ").push_bind(if is_enabled { 1 } else { 0 });
        }

        let row = query_builder.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get(0);
        Ok(count)
    }

    /// Create a new device alarm rule
    pub fn new(request: CreateDeviceAlarmRuleRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: request.device_id,
            property_id: request.property_id,
            rule_name: request.rule_name,
            rule_type: request.rule_type,
            condition: request.condition,
            alarm_level: request.alarm_level,
            is_enabled: request.is_enabled.unwrap_or(true),
            created_at: now.clone(),
            updated_at: now,
            description: request.description,
        }
    }

    /// Check if the alarm rule is active
    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    /// Get alarm level priority (higher number = higher priority)
    pub fn get_alarm_priority(&self) -> u8 {
        match self.alarm_level.as_str() {
            "info" => 1,
            "warning" => 2,
            "error" => 3,
            "critical" => 4,
            _ => 0,
        }
    }

    /// Validate the condition JSON
    pub fn validate_condition(&self) -> Result<(), String> {
        match serde_json::from_str::<serde_json::Value>(&self.condition) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Invalid condition JSON: {}", e)),
        }
    }
}

// Backward compatibility
pub type DeviceAlarmRuleDto = DeviceAlarmRule;
pub type DeviceAlarmRuleQueryParams = DeviceAlarmRuleQuery;
