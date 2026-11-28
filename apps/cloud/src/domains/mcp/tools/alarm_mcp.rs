// Alarm MCP Tools Module
// MCP tools wrapping existing alarm REST APIs

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::domains::alarm::{
    AlarmCondition, AlarmLevel, AlarmQueryCriteria, AlarmRule, AlarmStatus, NotificationConfig, SortOrder, TimeRange,
};

use crate::domains::mcp::McpState;
use crate::domains::mcp::handlers::get_mcp_context;
use crate::domains::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};

/// Tool input: List alarms
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct ListAlarmsInput {
    workspace_id: Option<String>,
    #[serde(alias = "deviceIds")]
    thing_ids: Option<Vec<String>>,
    levels: Option<Vec<String>>,
    statuses: Option<Vec<String>>,
    start_time: Option<String>,
    end_time: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

/// Tool input: Acknowledge alarm
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcknowledgeAlarmInput {
    id: String,
    note: Option<String>,
}

/// Tool input: Create alarm rule
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct CreateAlarmRuleInput {
    workspace_id: String,
    name: String,
    description: Option<String>,
    #[serde(rename = "thingId", alias = "deviceId")]
    thing_id: Option<String>,
    property_id: Option<String>,
    rule_type: String,
    condition: Value,
    alarm_level: String,
    is_enabled: Option<bool>,
    notification_config: Option<Value>,
}

/// List alarms tool handler
pub struct AlarmListHandler {
    state: Option<Arc<McpState>>,
}

impl AlarmListHandler {
    pub fn new(state: Option<Arc<McpState>>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ToolHandler for AlarmListHandler {
    fn name(&self) -> &str {
        "alarm_list"
    }

    fn description(&self) -> &str {
        "List alarms with optional filters (workspace, device, level, status, time range). \
         One call returns the page (`data`), `pagination.total_count`, and `summary` — \
         alarm counts by level over the same device/time filters. Use `summary` for \
         distributions; do not issue one call per level or pageSize=1 probes to count."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "workspaceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID for filtering".to_string()),
            },
        );
        props.insert(
            "thingIds".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Filter by thing IDs".to_string()),
            },
        );
        props.insert(
            "levels".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Filter by alarm levels (info, warning, error, critical)".to_string()),
            },
        );
        props.insert(
            "statuses".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Filter by status (active, acknowledged, resolved)".to_string()),
            },
        );
        props.insert(
            "startTime".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Start time (RFC3339 format)".to_string()),
            },
        );
        props.insert(
            "endTime".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("End time (RFC3339 format)".to_string()),
            },
        );
        props.insert(
            "page".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Page number (default: 1)".to_string()),
            },
        );
        props.insert(
            "pageSize".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Page size (default: 20)".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ListAlarmsInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims =
            get_mcp_context().ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?;

        let state = self
            .state
            .as_ref()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let page = input.page.unwrap_or(1);
        let page_size = input.page_size.unwrap_or(20);

        let time_range = if input.start_time.is_some() || input.end_time.is_some() {
            let start = input
                .start_time
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));

            let end = input
                .end_time
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            Some(TimeRange { start, end })
        } else {
            None
        };

        let alarm_levels = input.levels.as_ref().and_then(|l| {
            let parsed: Vec<AlarmLevel> = l.iter().filter_map(|level| AlarmLevel::parse_str(level)).collect();
            if parsed.is_empty() { None } else { Some(parsed) }
        });

        let statuses = input
            .statuses
            .as_ref()
            .map(|s| {
                s.iter()
                    .filter_map(|status| AlarmStatus::parse_str(status))
                    .collect::<Vec<_>>()
            })
            .filter(|statuses| !statuses.is_empty())
            .or_else(|| Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]));

        let criteria = AlarmQueryCriteria {
            workspace_id: Some(claims.workspace_id.clone()),
            thing_ids: input.thing_ids,
            property_ids: None,
            alarm_levels,
            alarm_types: None,
            statuses,
            time_range,
            sort_by: Some("alarm_time".to_string()),
            sort_order: Some(SortOrder::Desc),
            limit: Some(page_size),
            offset: Some((page - 1) * page_size),
        };

        let result = state
            .alarm_service
            .get_alarm_history(criteria.clone())
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to query alarms: {}", e)))?;

        let total = state.alarm_service.count_alarms(criteria.clone()).await.unwrap_or(0) as u32;

        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

        let alarms: Vec<crate::domains::alarm::dto::AlarmDto> = result.into_iter().map(|a| a.into()).collect();

        let summary = level_summary(&state.alarm_service, &criteria).await;

        Ok(serde_json::json!({
            "data": alarms,
            "pagination": {
                "page": page,
                "page_size": page_size,
                "total_pages": total_pages,
                "total_count": total
            },
            "summary": summary
        }))
    }
}

/// 等级分布：沿用本次查询的设备/时间过滤，但忽略调用方传的 levels/statuses 与分页。
/// 一次调用即可回答"各等级有多少条"，避免模型按等级拆成多次查询。
async fn level_summary(
    alarm_service: &crate::domains::alarm::service::AlarmService,
    criteria: &AlarmQueryCriteria,
) -> Value {
    let base = AlarmQueryCriteria {
        workspace_id: criteria.workspace_id.clone(),
        thing_ids: criteria.thing_ids.clone(),
        property_ids: criteria.property_ids.clone(),
        alarm_levels: None,
        alarm_types: criteria.alarm_types.clone(),
        statuses: None,
        time_range: criteria.time_range.clone(),
        sort_by: None,
        sort_order: None,
        limit: None,
        offset: None,
    };
    let mut by_level = serde_json::Map::new();
    let mut total = 0u64;
    for level in [
        AlarmLevel::Critical,
        AlarmLevel::Error,
        AlarmLevel::Warning,
        AlarmLevel::Info,
    ] {
        let scoped = AlarmQueryCriteria {
            alarm_levels: Some(vec![level]),
            ..base.clone()
        };
        let count = alarm_service.count_alarms(scoped).await.unwrap_or(0);
        total = total.saturating_add(count);
        by_level.insert(level.as_str().to_string(), serde_json::json!(count));
    }
    serde_json::json!({
        "total": total,
        "by_level": by_level,
    })
}

/// Acknowledge alarm tool handler
pub struct AlarmAcknowledgeHandler {
    state: Option<Arc<McpState>>,
}

impl AlarmAcknowledgeHandler {
    pub fn new(state: Option<Arc<McpState>>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ToolHandler for AlarmAcknowledgeHandler {
    fn name(&self) -> &str {
        "alarm_acknowledge"
    }

    fn description(&self) -> &str {
        "Acknowledge an alarm."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "id".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Alarm ID to acknowledge".to_string()),
            },
        );
        props.insert(
            "note".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Optional acknowledgment note".to_string()),
            },
        );
        InputSchema::object(vec!["id".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: AcknowledgeAlarmInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims =
            get_mcp_context().ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?;

        let state = self
            .state
            .as_ref()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // SECURITY: Verify alarm belongs to the authenticated workspace before acknowledging
        // 1. Fetch the alarm
        let alarm = state
            .alarm_service
            .get_alarm_by_id(&input.id, None)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to fetch alarm: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("Alarm not found".to_string()))?;

        // 2. Get tenant-aware device service to verify workspace isolation
        // Using tenant_device_service ensures the device belongs to the authenticated workspace
        let tenant_device_service = state.tenant_device_service_str(&claims.workspace_id);
        let _device = tenant_device_service
            .get_thing_by_id(&alarm.thing_id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to fetch device: {}", e)))?
            .ok_or_else(|| {
                ToolError::NotFound("Thing associated with alarm not found or does not belong to workspace".to_string())
            })?;

        // 3. Workspace isolation is now verified by tenant_device_service
        // If we get here, the device belongs to the authenticated workspace

        // 4. Now safe to acknowledge
        state
            .alarm_service
            .acknowledge_alarm(
                &input.id,
                claims.actor_identifier().to_string(),
                &claims.workspace_id,
                input.note.map(|s| s.to_string()),
            )
            .await
            .map_err(|e| match e {
                crate::domains::alarm::AlarmError::NotFound(id) => {
                    ToolError::NotFound(format!("Alarm not found: {id}"))
                }
                crate::domains::alarm::AlarmError::InvalidStatusTransition { .. } => {
                    ToolError::InvalidParams(e.to_string())
                }
                _ => ToolError::Internal(format!("Failed to acknowledge alarm: {e}")),
            })?;

        Ok(serde_json::json!({
            "success": true,
            "alarm_id": input.id,
            "acknowledged_by": claims.actor_identifier()
        }))
    }
}

/// Create alarm rule tool handler
pub struct AlarmRuleAddHandler {
    state: Option<Arc<McpState>>,
}

impl AlarmRuleAddHandler {
    pub fn new(state: Option<Arc<McpState>>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ToolHandler for AlarmRuleAddHandler {
    fn name(&self) -> &str {
        "alarm_rule_add"
    }

    fn description(&self) -> &str {
        "Create a new alarm rule."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "workspaceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Workspace ID".to_string()),
            },
        );
        props.insert(
            "name".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Alarm rule name".to_string()),
            },
        );
        props.insert(
            "description".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Optional description".to_string()),
            },
        );
        props.insert(
            "thingId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Target thing ID (optional)".to_string()),
            },
        );
        props.insert(
            "propertyId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Target property ID (optional)".to_string()),
            },
        );
        props.insert(
            "ruleType".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Rule type: threshold, range, change_rate, offline".to_string()),
            },
        );
        props.insert(
            "condition".to_string(),
            PropertySchema {
                prop_type: "object".to_string(),
                description: Some("Condition as JSON object".to_string()),
            },
        );
        props.insert(
            "alarmLevel".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Alarm level: info, warning, error, critical".to_string()),
            },
        );
        props.insert(
            "isEnabled".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Whether the rule is enabled (default: true)".to_string()),
            },
        );
        InputSchema::object(
            vec![
                "workspaceId".to_string(),
                "name".to_string(),
                "ruleType".to_string(),
                "alarmLevel".to_string(),
            ],
            props,
        )
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: CreateAlarmRuleInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims =
            get_mcp_context().ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".to_string()))?;

        // SECURITY: Verify workspace_id matches the authenticated context
        // Prevents creating rules in other workspaces
        if input.workspace_id != claims.workspace_id {
            return Err(ToolError::Forbidden(
                "Access denied: workspace_id does not match authenticated workspace".to_string(),
            ));
        }

        let state = self
            .state
            .as_ref()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // Parse alarm level
        let alarm_level = AlarmLevel::parse_str(&input.alarm_level)
            .ok_or_else(|| ToolError::InvalidParams(format!("Invalid alarm level: {}", input.alarm_level)))?;

        // Parse condition
        let condition: AlarmCondition = serde_json::from_value(input.condition.clone())
            .map_err(|e| ToolError::InvalidParams(format!("Invalid condition: {}", e)))?;

        // Parse notification config
        let notification_config: NotificationConfig = match &input.notification_config {
            Some(nc) => serde_json::from_value(nc.clone())
                .map_err(|e| ToolError::InvalidParams(format!("Invalid notification config: {}", e)))?,
            None => NotificationConfig::default(),
        };

        let rule_type = match input.rule_type.as_str() {
            "threshold" => tinyiothub_storage::alarm_rule::RuleType::Threshold,
            "range" => tinyiothub_storage::alarm_rule::RuleType::Range,
            "change" => tinyiothub_storage::alarm_rule::RuleType::Change,
            "duration" => tinyiothub_storage::alarm_rule::RuleType::Duration,
            "composite" => tinyiothub_storage::alarm_rule::RuleType::Composite,
            _ => {
                return Err(ToolError::InvalidParams(format!(
                    "Invalid rule type: {}",
                    input.rule_type
                )));
            }
        };

        // Create the alarm rule
        let rule = AlarmRule::new(
            input.name,
            input.description,
            input.thing_id,
            input.property_id,
            rule_type,
            condition,
            alarm_level,
            notification_config,
            claims.workspace_id.clone(),
        )
        .map_err(|e| ToolError::Internal(format!("Failed to create alarm rule: {}", e)))?;

        let created_rule = state
            .alarm_service
            .create_rule(rule)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to create alarm rule: {}", e)))?;

        Ok(serde_json::to_value(created_rule).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::alarm::service::AlarmService;
    use tinyiothub_storage::alarm::{Alarm, AlarmType};

    fn alarm(level: AlarmLevel, status: AlarmStatus) -> Alarm {
        let mut alarm = Alarm::new(
            "thing-1".to_string(),
            None,
            None,
            AlarmType::DeviceOffline,
            level,
            format!("{level:?} alarm"),
            None,
            None,
            None,
        );
        alarm.status = status;
        alarm
    }

    async fn service() -> (Arc<tinyiothub_storage::Db>, Arc<AlarmService>) {
        let (pool, db) = tinyiothub_storage::test_helpers::fixture_pool_with_db().await;
        tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO things (id, name, workspace_id, created_at, updated_at) \
             VALUES ('thing-1','thing-1','ws-default-001','2026-01-01','2026-01-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        for item in [
            alarm(AlarmLevel::Critical, AlarmStatus::Active),
            alarm(AlarmLevel::Critical, AlarmStatus::Resolved),
            alarm(AlarmLevel::Warning, AlarmStatus::Active),
        ] {
            db.insert_alarm(&item).await.unwrap();
        }
        let db = Arc::new(db);
        (db.clone(), Arc::new(AlarmService::new(db)))
    }

    #[tokio::test]
    async fn level_summary_counts_every_level() {
        let (_db, service) = service().await;
        let criteria = AlarmQueryCriteria {
            statuses: Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]),
            limit: Some(20),
            ..Default::default()
        };

        let summary = level_summary(&service, &criteria).await;

        assert_eq!(summary["total"], 3, "summary ignores the caller's status filter");
        assert_eq!(summary["by_level"]["critical"], 2);
        assert_eq!(summary["by_level"]["warning"], 1);
        assert_eq!(summary["by_level"]["error"], 0);
        assert_eq!(summary["by_level"]["info"], 0);
    }

    #[tokio::test]
    async fn level_summary_ignores_caller_level_filter() {
        let (_db, service) = service().await;
        let criteria = AlarmQueryCriteria {
            alarm_levels: Some(vec![AlarmLevel::Critical]),
            ..Default::default()
        };

        let summary = level_summary(&service, &criteria).await;

        assert_eq!(
            summary["total"], 3,
            "a level-filtered query still needs the full distribution"
        );
        assert_eq!(summary["by_level"]["warning"], 1);
    }
}
