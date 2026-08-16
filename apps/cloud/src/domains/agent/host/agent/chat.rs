// AgentPool — chat abort + heartbeat runs (run_single / run_streaming).
//
// Storage-free (Task 7 fix round 1): chat send/history need db handles and
// live as cloud-side free functions (`host::chat::service::send_with_pool`,
// `host::chat::history::session_history_json`); only the storage-free
// operations stay on the pool.

use super::pool::AgentPool;
use crate::domains::agent::host::shared::config::AgentError;

// ============================================================================
// Streaming run result types
// ============================================================================

/// Result of a streaming heartbeat run
pub struct StreamingRunResult {
    pub final_text: String,
    pub tool_calls: Vec<StreamingToolCall>,
}

/// Tool call captured during streaming execution
#[derive(Debug, Clone)]
pub struct StreamingToolCall {
    pub name: String,
    pub args: serde_json::Value,
    pub result: Option<String>,
    pub success: bool,
}

impl AgentPool {
    // ========================================================================
    // Chat abort
    // ========================================================================

    pub async fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
        authorized_workspace: &str,
    ) -> Result<(), AgentError> {
        let parsed = crate::domains::agent::host::session::SessionKey::parse(session_key)?;
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }
        let _ = agent_id;
        if let Some(rid) = run_id {
            let mut handles = self.chat_handles.lock().await;
            match handles.remove(rid) {
                Some(handle) => handle.abort(),
                // An unknown run_id must not look like a successful abort —
                // the caller's run may still be streaming.
                None => {
                    return Err(AgentError::NotFound(format!(
                        "Unknown or already-finished run_id: {rid}"
                    )));
                }
            }
        }
        Ok(())
    }

    // ========================================================================
    // Run single (for cron jobs)
    // ========================================================================

    /// The heartbeat agent must already be pooled — the cloud caller ensures
    /// it via `host::chat::service::ensure_agent` (config/tool resolution is
    /// db-backed and stays on the cloud side).
    pub async fn run_single(&self, workspace_id: &str, message: &str) -> Result<String, AgentError> {
        // Per-workspace agent key prevents cross-workspace tool context leak.
        let agent_id = heartbeat_agent_id(workspace_id);
        let agent = self
            .get_cached(&agent_id)
            .ok_or_else(|| AgentError::NotFound(format!("Heartbeat agent not pooled: {agent_id}")))?;
        let mut ag = agent.lock().await;
        ag.run_single(message)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    // ========================================================================
    // Run streaming (for heartbeat with TurnEvent interception)
    // ========================================================================

    /// Run the heartbeat agent with streaming TurnEvents, enabling per-tool-call
    /// interception (trust gate, action recording).
    ///
    /// Like [`Self::run_single`], the agent must be pooled by the caller.
    pub async fn run_streaming(&self, workspace_id: &str, message: &str) -> Result<StreamingRunResult, AgentError> {
        let agent_id = heartbeat_agent_id(workspace_id);
        let agent = self
            .get_cached(&agent_id)
            .ok_or_else(|| AgentError::NotFound(format!("Heartbeat agent not pooled: {agent_id}")))?;

        // Set up TurnEvent channel for real-time event interception
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<zeroclaw::agent::TurnEvent>(64);

        // Spawn tool call collector
        let tool_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tool_calls_clone = std::sync::Arc::clone(&tool_calls);
        let collector = tokio::spawn(async move {
            while let Some(evt) = event_rx.recv().await {
                match evt {
                    zeroclaw::agent::TurnEvent::ToolCall { name, args, .. } => {
                        let mut calls = tool_calls_clone.lock().unwrap();
                        calls.push(StreamingToolCall {
                            name,
                            args,
                            result: None,
                            success: true,
                        });
                    }
                    zeroclaw::agent::TurnEvent::ToolResult { name, output, .. } => {
                        let mut calls = tool_calls_clone.lock().unwrap();
                        if let Some(last) = calls.iter_mut().rev().find(|c| c.name == name) {
                            last.result = Some(output.clone());
                            // NOTE: TurnEvent::ToolResult doesn't carry ToolResult.success.
                            // Trust enforcement is handled by TrustAwareTool wrapping;
                            // the LLM's response text handles error reporting via healing report.
                        }
                    }
                    _ => {}
                }
            }
        });

        // No inner timeout here: the heartbeat tick in tinyiothub-ai bounds the
        // whole run (see heartbeat::loop_ TICK_TIMEOUT). A shorter inner timeout
        // fires first every time, making the tick-level bound unreachable.
        let mut ag = agent.lock().await;
        let result = ag.turn_streamed(message, event_tx, None).await;
        drop(ag);

        // Wait for collector to finish processing remaining events
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), collector).await;

        let tool_calls = match std::sync::Arc::try_unwrap(tool_calls) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        match result {
            Ok((final_text, _conversation)) => Ok(StreamingRunResult { final_text, tool_calls }),
            Err(e) => Err(AgentError::RequestFailed(e.to_string())),
        }
    }
}

/// "__heartbeat__" has no DB row, so the cloud caller pools it with
/// AgentRuntimeConfig::default() → server-level [minimax] model.
pub fn heartbeat_agent_id(workspace_id: &str) -> String {
    format!("__heartbeat__:{}", workspace_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> AgentPool {
        AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            crate::domains::agent::host::autonomous_factory::minimax_provider_factory(),
        )
        .expect("test AgentPool")
    }

    #[tokio::test]
    async fn chat_abort_rejects_session_from_other_workspace() {
        let pool = test_pool();
        let err = pool
            .chat_abort("agent_main", "agent:ws_other:agent_main/s1", Some("r1"), "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }

    #[tokio::test]
    async fn chat_abort_with_unknown_run_id_errors_and_none_run_id_is_noop() {
        let pool = test_pool();
        let err = pool
            .chat_abort("agent_main", "agent:ws1:agent_main/s1", Some("nonexistent-run"), "")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AgentError::NotFound(_)),
            "unknown run_id must not silently succeed: {err:?}"
        );
        pool.chat_abort("agent_main", "agent:ws1:agent_main/s1", None, "")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_streaming_requires_pooled_agent() {
        let pool = test_pool();
        let result = pool.run_streaming("ws1", "tick").await;
        assert!(matches!(result, Err(AgentError::NotFound(_))));
    }
}
