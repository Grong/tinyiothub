// Alarm MCP Tools Module
// MCP tools wrapping existing alarm REST APIs

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::modules::mcp::handlers::get_mcp_context;
use crate::modules::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::modules::alarm::{
    AlarmCondition, AlarmLevel, AlarmQueryCriteria, AlarmRule, AlarmStatus, NotificationConfig,
    TimeRange, SortOrder,
};

/// Tool input: List alarms
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct ListAlarmsInput {
    workspace_id: Option<String>,
    device_ids: Option<Vec<String>>,
    levels: Option<Vec<String>>,
    statuses: Option<Vec<String>>,
    start_time: Option<String>,
    end_time: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

/// Tool input: Get alarm statistics
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlarmStatisticsInput {
    workspace_id: Option<String>,
    start_time: String,
    end_time: String,
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
    device_id: Option<String>,
    property_id: Option<String>,
    rule_type: String,
    condition: Value,
    alarm_level: String,
    is_enabled: Option<bool>,
    notification_config: Option<Value>,
}

/// List alarms tool handler
pub struct AlarmListHandler;

#[async_trait]
impl ToolHandler for AlarmListHandler {
    fn name(&self) -> &str {
        "alarm_list"
    }

    fn description(&self) -> &str {
        "List alarms with optional filters (workspace, device, level, status, time range)."
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
            "deviceIds".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("Filter by device IDs".to_string()),
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

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::modules::mcp::get_app_state()
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
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        });

        let statuses = input.statuses.as_ref().and_then(|s| {
            let parsed: Vec<AlarmStatus> = s.iter().filter_map(|status| AlarmStatus::parse_str(status)).collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        });

        let criteria = AlarmQueryCriteria {
            workspace_id: Some(claims.workspace_id.clone()),
            device_ids: input.device_ids,
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

        let total = state
            .alarm_service
            .count_alarms(criteria)
            .await
            .unwrap_or(0) as u32;

        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

        let alarms: Vec<crate::modules::alarm::types::AlarmDto> =
            result.into_iter().map(|a| a.into()).collect();

        Ok(serde_json::json!({
            "data": alarms,
            "pagination": {
                "page": page,
                "page_size": page_size,
                "total_pages": total_pages,
                "total_count": total
            }
        }))
    }
}

/// Alarm statistics handler
pub struct AlarmStatisticsHandler;

#[async_trait]
impl ToolHandler for AlarmStatisticsHandler {
    fn name(&self) -> &str {
        "alarm_statistics"
    }

    fn description(&self) -> &str {
        "Get alarm statistics for a time range."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
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
        InputSchema::object(vec!["startTime".to_string(), "endTime".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: AlarmStatisticsInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        // SECURITY: Reject if user tries to specify a different workspace_id
        // The statistics should only be for the authenticated workspace
        if let Some(ref ws_id) = input.workspace_id
            && ws_id != &claims.workspace_id {
                return Err(ToolError::Forbidden(
                    "Access denied: cannot query statistics for other workspaces".to_string()
                ));
            }

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        let start_time = chrono::DateTime::parse_from_rfc3339(&input.start_time)
            .map_err(|_| ToolError::InvalidParams("Invalid start_time format".to_string()))?
            .with_timezone(&chrono::Utc);

        let end_time = chrono::DateTime::parse_from_rfc3339(&input.end_time)
            .map_err(|_| ToolError::InvalidParams("Invalid end_time format".to_string()))?
            .with_timezone(&chrono::Utc);

        let time_range = TimeRange { start: start_time, end: end_time };

        let stats = state
            .alarm_service
            .get_alarm_statistics(time_range)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to get alarm statistics: {}", e)))?;

        let dto: crate::modules::alarm::types::AlarmStatisticsDto = stats.into();
        Ok(serde_json::to_value(dto).unwrap())
    }
}

/// Acknowledge alarm tool handler
pub struct AlarmAcknowledgeHandler;

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

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        let state = crate::modules::mcp::get_app_state()
            .ok_or_else(|| ToolError::Internal("AppState not initialized".to_string()))?;

        // SECURITY: Verify alarm belongs to the authenticated workspace before acknowledging
        // 1. Fetch the alarm
        let alarm = state.alarm_service.get_alarm_by_id(&input.id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to fetch alarm: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("Alarm not found".to_string()))?;

        // 2. Get tenant-aware device service to verify workspace isolation
        // Using tenant_device_service ensures the device belongs to the authenticated workspace
        let tenant_device_service = state.tenant_device_service(&Some(claims.workspace_id.clone()));
        let _device = tenant_device_service.get_device_by_id(&alarm.device_id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to fetch device: {}", e)))?
            .ok_or_else(|| ToolError::NotFound("Device associated with alarm not found or does not belong to workspace".to_string()))?;

        // 3. Workspace isolation is now verified by tenant_device_service
        // If we get here, the device belongs to the authenticated workspace

        // 4. Now safe to acknowledge
        state
            .alarm_service
            .acknowledge_alarm(&input.id, claims.actor_identifier().to_string(), input.note.map(|s| s.to_string()))
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to acknowledge alarm: {}", e)))?;

        Ok(serde_json::json!({
            "success": true,
            "alarm_id": input.id,
            "acknowledged_by": claims.actor_identifier()
        }))
    }
}

/// Create alarm rule tool handler
pub struct AlarmRuleAddHandler;

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
            "deviceId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Target device ID (optional)".to_string()),
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

        let claims = get_mcp_context().ok_or_else(|| {
            ToolError::Unauthorized("MCP context not initialized".to_string())
        })?;

        // SECURITY: Verify workspace_id matches the authenticated context
        // Prevents creating rules in other workspaces
        if input.workspace_id != claims.workspace_id {
            return Err(ToolError::Forbidden(
                "Access denied: workspace_id does not match authenticated workspace".to_string()
            ));
        }

        let state = crate::modules::mcp::get_app_state()
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
            "threshold" => crate::modules::alarm::RuleType::Threshold,
            "range" => crate::modules::alarm::RuleType::Range,
            "change" => crate::modules::alarm::RuleType::Change,
            "duration" => crate::modules::alarm::RuleType::Duration,
            "composite" => crate::modules::alarm::RuleType::Composite,
            _ => return Err(ToolError::InvalidParams(format!("Invalid rule type: {}", input.rule_type))),
        };

        // Create the alarm rule
        let rule = AlarmRule::new(
            input.name,
            input.description,
            input.device_id,
            input.property_id,
            rule_type,
            condition,
            alarm_level,
            notification_config,
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
