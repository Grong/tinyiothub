//! Adapter: host `AgentPool` → loop `AgentPoolLike` (P4-Task22).
//!
//! Formerly `cloud::shared::ai_adapter::CloudAgentPoolAdapter`; now that the
//! agent loop and host live in one crate, this bridge is agent-internal glue
//! and lives with the host. The composition layer wires it into the
//! heartbeat runner at startup.

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::heartbeat::TrustConfig;

use tinyiothub_agent::pool::{AgentPool, StreamingToolCall};
use tinyiothub_agent::runtime::agent::pool::{AgentPoolLike, AgentRunOutput, ToolCallRecord};

/// Wraps the host `AgentPool` to implement the loop's `AgentPoolLike` trait.
///
/// Holds the db handle (cloud-side composition detail) and provisions pooled
/// agents via `chat::service::ensure_agent` — the pool itself is storage-free
/// and receives fully resolved config + tools (Task 7 fix round 1).
pub struct HostAgentPoolAdapter {
    pool: Arc<AgentPool>,
    db_pool: sqlx::SqlitePool,
}

impl HostAgentPoolAdapter {
    pub fn new(pool: Arc<AgentPool>, db_pool: sqlx::SqlitePool) -> Self {
        Self { pool, db_pool }
    }
}

#[async_trait]
impl AgentPoolLike for HostAgentPoolAdapter {
    async fn get_or_create_agent(&self, workspace_id: &str) -> anyhow::Result<String> {
        // Host AgentPool uses agent_id = workspace_id (one agent per workspace).
        // Config fetch stays on the cloud side (Task 7): the pool receives it.
        let config = crate::domains::agent::host::config::service::get_config(&self.db_pool, workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("Agent config error: {}", e))?;
        crate::domains::agent::host::chat::service::ensure_agent(&self.pool, workspace_id, workspace_id, &config)
            .await
            .map_err(|e| anyhow::anyhow!("AgentPool error: {}", e))?;
        // Return workspace_id as the handle identifier
        Ok(workspace_id.to_string())
    }

    async fn send_message(&self, workspace_id: &str, prompt: &str) -> anyhow::Result<AgentRunOutput> {
        // Ensure the heartbeat agent is pooled (cache hit is config-fetch-free),
        // then delegate to AgentPool's run_streaming and collect the response.
        let agent_id = tinyiothub_agent::pool::heartbeat_agent_id(workspace_id);
        crate::domains::agent::host::chat::service::ensure_agent(
            &self.pool,
            &agent_id,
            workspace_id,
            &tinyiothub_agent::config::AgentRuntimeConfig::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("AgentPool error: {}", e))?;
        let result = self
            .pool
            .run_streaming(workspace_id, prompt)
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;
        let tool_calls = result.tool_calls.into_iter().map(map_tool_call).collect();
        Ok(AgentRunOutput {
            text: result.final_text,
            tool_calls,
        })
    }

    async fn shutdown(&self) {
        // AgentPool doesn't have explicit shutdown; agents are dropped naturally
    }

    fn set_trust_config(&self, workspace_id: &str, config: TrustConfig) {
        // Trust evaluation (evaluate_tool_trust) classifies tools by name pattern —
        // no per-tool name lists needed here.
        self.pool.set_trust_config(workspace_id, config);
    }

    fn cleanup_idle(&self) -> usize {
        self.pool.cleanup_idle()
    }
}

/// Map a streaming tool call to the heartbeat audit record. Lookup chain:
/// `thing_id` (snake) → `thingId` (advertised by InvokeActionTool /
/// AutonomousInvokeTool) → `deviceId` (legacy camelCase) → `device_id`
/// (legacy snake). A missing result string becomes empty details rather than
/// a lossy "null".
fn map_tool_call(c: StreamingToolCall) -> ToolCallRecord {
    ToolCallRecord {
        tool_name: c.name,
        device_id: c
            .args
            .get("thing_id")
            .or_else(|| c.args.get("thingId"))
            .or_else(|| c.args.get("deviceId"))
            .or_else(|| c.args.get("device_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        success: c.success,
        details: c.result.unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: serde_json::Value, result: Option<String>) -> StreamingToolCall {
        StreamingToolCall {
            name: "set_temperature".into(),
            args,
            result,
            success: true,
        }
    }

    #[test]
    fn snake_case_device_id_wins_over_camel_case() {
        let rec = map_tool_call(call(
            serde_json::json!({"thing_id": "d_snake", "deviceId": "d_camel"}),
            Some("ok".into()),
        ));
        assert_eq!(rec.device_id.as_deref(), Some("d_snake"));
        assert_eq!(rec.tool_name, "set_temperature");
        assert!(rec.success);
        assert_eq!(rec.details, "ok");
    }

    #[test]
    fn camel_case_device_id_is_accepted_as_fallback() {
        let rec = map_tool_call(call(serde_json::json!({"deviceId": "d_camel"}), None));
        assert_eq!(rec.device_id.as_deref(), Some("d_camel"));
        assert_eq!(rec.details, "");
    }

    #[test]
    fn missing_device_id_maps_to_none() {
        let rec = map_tool_call(call(serde_json::json!({"value": 42}), None));
        assert_eq!(rec.device_id, None);
    }

    #[test]
    fn non_string_device_id_maps_to_none() {
        let rec = map_tool_call(call(serde_json::json!({"thing_id": 42}), None));
        assert_eq!(rec.device_id, None);
    }

    #[test]
    fn advertised_camel_thing_id_is_accepted() {
        // InvokeActionTool / AutonomousInvokeTool advertise camelCase `thingId`.
        let rec = map_tool_call(call(serde_json::json!({"thingId": "d_thing"}), None));
        assert_eq!(rec.device_id.as_deref(), Some("d_thing"));
    }

    #[test]
    fn legacy_snake_device_id_is_accepted_as_last_fallback() {
        let rec = map_tool_call(call(serde_json::json!({"device_id": "d_legacy"}), None));
        assert_eq!(rec.device_id.as_deref(), Some("d_legacy"));
    }

    #[test]
    fn advertised_thing_id_wins_over_legacy_keys() {
        let rec = map_tool_call(call(
            serde_json::json!({"thingId": "d_thing", "deviceId": "d_camel", "device_id": "d_legacy"}),
            None,
        ));
        assert_eq!(rec.device_id.as_deref(), Some("d_thing"));
    }
}
