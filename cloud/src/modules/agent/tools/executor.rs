// ToolExecutor — unified tool execution with trust, snapshot, audit, rollback
//
// Wraps a Tool with cross-cutting concerns:
//   1. Trust check (via TrustConfig + TrustAwareTool)
//   2. Before-snapshot capture (for reversible mutating tools)
//   3. Audit logging (via AgentActionRepository)
//   4. Structured error signals (guides LLM toward proposals)
//
// This is the concrete shared abstraction for chat, heartbeat, and A2UI
// tool dispatch — each channel wraps its tools through the same executor.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroclaw::tools::{Tool, ToolResult};

use super::super::{
    action_repo::{ActionType, AgentAction, AgentActionRepository, EventType},
    heartbeat_manager::{TrustConfig, TrustLevel, resolve_trust},
};

// ============================================================================
// Snapshot types — before/after state for rollback
// ============================================================================

/// Captured state before a mutating tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSnapshot {
    /// Tool that created this snapshot
    pub tool_name: String,
    /// Affected device
    pub device_id: String,
    /// Property values before execution (key → value)
    pub before_properties: serde_json::Value,
    /// Timestamp when snapshot was taken
    pub captured_at: String,
}

impl ToolSnapshot {
    pub fn new(tool_name: &str, device_id: &str, before: serde_json::Value) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            device_id: device_id.to_string(),
            before_properties: before,
            captured_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// Whether a tool is reversible (supports rollback)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    /// Can be rolled back via reverse-write of before_snapshot
    Reversible,
    /// Cannot be rolled back (side effects like reboot, send_command)
    Irreversible,
    /// Read-only tool — no rollback needed
    ReadOnly,
}

/// Classify tool reversibility by name
pub fn classify_reversibility(tool_name: &str) -> Reversibility {
    match tool_name {
        "write_properties" => Reversibility::Reversible,
        "send_command" | "delete_device" | "create_device" => Reversibility::Irreversible,
        "alarm_acknowledge" => Reversibility::Reversible,
        name if name.starts_with("search_")
            || name.starts_with("get_")
            || name.starts_with("read_")
            || name.starts_with("list_") =>
        {
            Reversibility::ReadOnly
        }
        _ => Reversibility::Irreversible,
    }
}

// ============================================================================
// Execution result — what the caller gets back
// ============================================================================

/// Outcome of a tool execution through ToolExecutor
#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    /// The raw tool result
    pub tool_result: ToolResult,
    /// Whether the tool was blocked by trust
    pub blocked: bool,
    /// Why it was blocked (if applicable) — guides LLM
    pub block_reason: Option<String>,
    /// Captured before-snapshot (for reversible mutating tools)
    pub snapshot: Option<ToolSnapshot>,
    /// Whether rollback is supported
    pub reversibility: Reversibility,
    /// Recorded action ID in agent_actions
    pub action_id: Option<String>,
}

// ============================================================================
// ToolExecutor
// ============================================================================

/// Unified tool execution layer.
///
/// Wraps trust checking, snapshot capture, execution, and audit logging
/// into a single pipeline used by chat, heartbeat, and A2UI channels.
pub struct ToolExecutor {
    pub trust_config: Arc<TrustConfig>,
    pub action_repo: Arc<dyn AgentActionRepository>,
    pub workspace_id: String,
}

impl ToolExecutor {
    pub fn new(
        trust_config: Arc<TrustConfig>,
        action_repo: Arc<dyn AgentActionRepository>,
        workspace_id: String,
    ) -> Self {
        Self { trust_config, action_repo, workspace_id }
    }

    /// Execute a tool call through the full pipeline:
    ///   trust check → snapshot → execute → audit
    ///
    /// Called by chat/heartbeat/A2UI after the LLM generates a tool call.
    pub async fn execute(&self, tool: &dyn Tool, args: &serde_json::Value) -> ExecutionOutcome {
        let tool_name = tool.name().to_string();
        let device_id = Self::extract_device_id(args).unwrap_or("unknown");

        // 1. Trust check
        let level = resolve_trust(&self.trust_config, &tool_name, device_id);

        let reversibility = classify_reversibility(&tool_name);

        match level {
            TrustLevel::Disabled => ExecutionOutcome {
                tool_result: ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Tool '{}' is disabled for device '{}'. Do not retry.",
                        tool_name, device_id,
                    )),
                },
                blocked: true,
                block_reason: Some(format!("disabled:{}:{}", tool_name, device_id)),
                snapshot: None,
                reversibility,
                action_id: None,
            },
            TrustLevel::ApprovalRequired => {
                // Record as proposal, not executed
                let proposal_id = uuid::Uuid::new_v4().to_string();
                let content = serde_json::json!({
                    "type": "proposal",
                    "proposalId": proposal_id,
                    "status": "pending",
                    "toolName": tool_name,
                    "deviceId": device_id,
                    "toolParams": args,
                    "block_reason": "approval_required",
                })
                .to_string();

                let action = AgentAction::new(
                    self.workspace_id.clone(),
                    "default".into(),
                    None,
                    Some(device_id.to_string()),
                    EventType::Heartbeat,
                    ActionType::Proposal,
                    content,
                );

                let action_id = action.id.clone();
                if let Err(e) = self.action_repo.insert(&action).await {
                    tracing::error!("ToolExecutor: failed to record proposal: {}", e);
                }

                ExecutionOutcome {
                    tool_result: ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "Tool '{}' requires human approval for device '{}'. \
                             Instead of executing, propose this action in your \
                             pending_proposals with tool_name, device_id, \
                             summary, reason, and risk level.",
                            tool_name, device_id,
                        )),
                    },
                    blocked: true,
                    block_reason: Some(format!("approval_required:{}:{}", tool_name, device_id)),
                    snapshot: None,
                    reversibility,
                    action_id: Some(action_id),
                }
            }
            TrustLevel::AutoWithLog | TrustLevel::FullAuto => {
                // 2. Capture before-snapshot for reversible tools
                let snapshot = if reversibility == Reversibility::Reversible {
                    // Snapshot will be captured by the caller via read_properties
                    // before write_properties — we store a placeholder
                    Some(ToolSnapshot::new(
                        &tool_name,
                        device_id,
                        serde_json::json!({"note": "snapshot captured before execution"}),
                    ))
                } else {
                    None
                };

                // 3. Execute
                let result = tool.execute(args.clone()).await.unwrap_or_else(|e| ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });

                // 4. Audit
                let content = serde_json::json!({
                    "type": "auto_executed",
                    "tool": tool_name,
                    "deviceId": device_id,
                    "args": args,
                    "success": result.success,
                    "error": result.error,
                    "trustLevel": level.as_str(),
                    "reversibility": format!("{:?}", reversibility),
                })
                .to_string();

                let action = AgentAction::new(
                    self.workspace_id.clone(),
                    "default".into(),
                    None,
                    Some(device_id.to_string()),
                    EventType::Heartbeat,
                    ActionType::AutoExecuted,
                    content,
                );

                let action_id = action.id.clone();
                if let Err(e) = self.action_repo.insert(&action).await {
                    tracing::error!("ToolExecutor: failed to record auto action: {}", e);
                }

                ExecutionOutcome {
                    tool_result: result,
                    blocked: false,
                    block_reason: None,
                    snapshot,
                    reversibility,
                    action_id: Some(action_id),
                }
            }
        }
    }

    /// Extract device_id from tool args JSON
    fn extract_device_id(args: &serde_json::Value) -> Option<&str> {
        args.get("device_id").or_else(|| args.get("deviceId")).and_then(|v| v.as_str())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_reversibility_write_properties() {
        assert_eq!(classify_reversibility("write_properties"), Reversibility::Reversible);
    }

    #[test]
    fn test_classify_reversibility_send_command_irreversible() {
        assert_eq!(classify_reversibility("send_command"), Reversibility::Irreversible);
    }

    #[test]
    fn test_classify_reversibility_read_only_tools() {
        assert_eq!(classify_reversibility("search_devices"), Reversibility::ReadOnly);
        assert_eq!(classify_reversibility("get_device"), Reversibility::ReadOnly);
        assert_eq!(classify_reversibility("read_properties"), Reversibility::ReadOnly);
        assert_eq!(classify_reversibility("list_schedules"), Reversibility::ReadOnly);
    }

    #[test]
    fn test_classify_reversibility_unknown_tool_irreversible() {
        assert_eq!(classify_reversibility("some_custom_tool"), Reversibility::Irreversible);
    }

    #[test]
    fn test_trust_level_default_is_approval_required() {
        assert_eq!(TrustLevel::default(), TrustLevel::ApprovalRequired);
    }

    #[test]
    fn test_resolve_trust_empty_config_defaults() {
        let config = TrustConfig::new();
        let level = resolve_trust(&config, "write_properties", "dev-01");
        assert_eq!(level, TrustLevel::ApprovalRequired);
    }

    #[test]
    fn test_resolve_trust_wildcard() {
        let mut config = TrustConfig::new();
        let mut devices = std::collections::HashMap::new();
        devices.insert("*".to_string(), TrustLevel::FullAuto);
        config.insert("send_command".to_string(), devices);

        assert_eq!(resolve_trust(&config, "send_command", "any-device"), TrustLevel::FullAuto);
    }

    #[test]
    fn test_resolve_trust_device_specific_overrides_wildcard() {
        let mut config = TrustConfig::new();
        let mut devices = std::collections::HashMap::new();
        devices.insert("*".to_string(), TrustLevel::FullAuto);
        devices.insert("dev-01".to_string(), TrustLevel::ApprovalRequired);
        config.insert("write_properties".to_string(), devices);

        // General devices: FullAuto
        assert_eq!(resolve_trust(&config, "write_properties", "dev-02"), TrustLevel::FullAuto);
        // dev-01 specifically: ApprovalRequired (override)
        assert_eq!(
            resolve_trust(&config, "write_properties", "dev-01"),
            TrustLevel::ApprovalRequired
        );
    }
}
