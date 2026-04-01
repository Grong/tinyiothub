// Self-Heal Tools Module
// MCP tools for system self-healing and recovery

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::api::self_healing::get_self_healing_state;
use crate::domain::self_healing::{RecoveryActionType, SeverityLevel};
use crate::dto::entity::self_healing::{ExecuteSelfHealRequest, SelfHealingPolicyDto};

/// Get self-heal policy tool handler
pub struct GetSelfHealPolicyHandler;

#[async_trait]
impl ToolHandler for GetSelfHealPolicyHandler {
    fn name(&self) -> &str {
        "get_self_heal_policy"
    }

    fn description(&self) -> &str {
        "获取系统自愈策略配置，包括健康阈值和恢复动作"
    }

    fn input_schema(&self) -> InputSchema {
        InputSchema::object(vec![], HashMap::new())
    }

    async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
        let state = get_self_healing_state()
            .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

        let state_guard = state.read().await;
        let policy_dto = SelfHealingPolicyDto::from(&state_guard.policy);
        drop(state_guard);

        Ok(serde_json::to_value(policy_dto).unwrap_or_default())
    }
}

/// Execute self-heal action tool handler
pub struct ExecuteSelfHealActionHandler;

#[async_trait]
impl ToolHandler for ExecuteSelfHealActionHandler {
    fn name(&self) -> &str {
        "execute_self_heal_action"
    }

    fn description(&self) -> &str {
        "执行指定的自愈动作"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "level".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("严重级别: L0, L1, L2, L3".to_string()),
            },
        );
        props.insert(
            "actionType".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("自愈动作类型: log_only, restart_driver, rejoin_lora, reconnect_device, clean_logs, report_cloud, create_ticket".to_string()),
            },
        );
        props.insert(
            "target".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("目标设备或进程 ID".to_string()),
            },
        );
        InputSchema::object(vec!["level".to_string(), "actionType".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let request: ExecuteSelfHealRequest = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let state = get_self_healing_state()
            .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

        let state_guard = state.read().await;
        let executor = state_guard.executor.clone();
        let policy = state_guard.policy.clone();
        let repository = state_guard.repository.clone();
        drop(state_guard);

        let level = match request.level.to_uppercase().as_str() {
            "L0" => SeverityLevel::L0,
            "L1" => SeverityLevel::L1,
            "L2" => SeverityLevel::L2,
            "L3" => SeverityLevel::L3,
            _ => return Err(ToolError::InvalidParams("Invalid level: use L0, L1, L2, or L3".to_string())),
        };

        let action_type = match request.action_type.to_lowercase().as_str() {
            "log_only" => RecoveryActionType::LogOnly,
            "restart_driver" => RecoveryActionType::RestartDriver,
            "rejoin_lora" => RecoveryActionType::RejoinLora,
            "reconnect_device" => RecoveryActionType::ReconnectDevice,
            "clean_logs" => RecoveryActionType::CleanLogs,
            "report_cloud" => RecoveryActionType::ReportCloud,
            "create_ticket" => RecoveryActionType::CreateTicket,
            _ => return Err(ToolError::InvalidParams("Invalid action_type".to_string())),
        };

        let cooldown = policy.levels.get(&level)
            .map(|p| p.cooldown_secs)
            .unwrap_or(0);

        // Check require_approval flag — if set, reject via MCP
        if policy.levels.get(&level)
            .map(|p| p.require_approval)
            .unwrap_or(false)
        {
            return Err(ToolError::InvalidParams(
                "This action requires approval per policy — direct execution not allowed".to_string(),
            ));
        }

        let exec_result = executor.execute(level, action_type, request.target, cooldown)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        // Persist execution to database
        if let Err(e) = repository.save(&exec_result).await {
            tracing::error!("Failed to persist healing execution: {}", e);
        }

        Ok(serde_json::json!({
            "execution_id": exec_result.id,
            "executed": true,
            "result": format!("{:?}", exec_result.result),
            "logs": exec_result.logs
        }))
    }
}

/// Get recovery history tool handler
pub struct GetRecoveryHistoryHandler;

#[async_trait]
impl ToolHandler for GetRecoveryHistoryHandler {
    fn name(&self) -> &str {
        "get_recovery_history"
    }

    fn description(&self) -> &str {
        "获取系统恢复操作的历史记录"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "limit".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("返回记录数量限制".to_string()),
            },
        );
        props.insert(
            "offset".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("分页偏移".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct HistoryInput {
            limit: Option<u32>,
            offset: Option<u32>,
        }

        let input: HistoryInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let limit = input.limit.unwrap_or(20).min(100);
        let offset = input.offset.unwrap_or(0);

        let state = get_self_healing_state()
            .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

        let state_guard = state.read().await;
        let repository = state_guard.repository.clone();
        drop(state_guard);

        // Default tenant for MCP context (single-tenant or system context)
        let tenant_id = crate::api::mcp::handlers::get_mcp_context()
            .map(|c| c.tenant_id)
            .unwrap_or_else(|| "default".to_string());

        let executions = repository.get_recent(&tenant_id, limit, offset)
            .await
            .map_err(|e| ToolError::Internal(format!("DB error: {}", e)))?;

        let total = repository.count(&tenant_id)
            .await
            .unwrap_or(executions.len() as u32);

        let history: Vec<serde_json::Value> = executions.iter().map(|e| {
            serde_json::json!({
                "id": e.id,
                "timestamp": e.timestamp.to_rfc3339(),
                "level": format!("{:?}", e.level),
                "action_type": format!("{:?}", e.action_type),
                "target": e.target,
                "result": format!("{:?}", e.result),
                "logs": e.logs
            })
        }).collect();

        Ok(serde_json::json!({
            "executions": history,
            "limit": limit,
            "offset": offset,
            "total": total
        }))
    }
}

/// Register all self-heal tools to the registry
pub fn register_self_heal_tools(registry: &mut crate::api::mcp::tool_registry::HandlerRegistry) {
    registry.register(GetSelfHealPolicyHandler);
    registry.register(ExecuteSelfHealActionHandler);
    registry.register(GetRecoveryHistoryHandler);
}
