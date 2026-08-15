// AgentPool — chat session forwarding (send/history/abort, delegated to
// ChatService) and heartbeat runs (run_single / run_streaming).

use super::pool::AgentPool;
use crate::domains::agent::host::shared::config::AgentError;
use crate::domains::agent::host::{chat::service as chat_service, config::service as config_service};

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
    // Chat (delegated to ChatService)
    // ========================================================================

    pub async fn chat_send(
        &self,
        agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
        authorized_workspace: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<crate::domains::agent::host::types::ChatEvent>, AgentError> {
        let parsed = crate::domains::agent::host::session::SessionKey::parse(session_key)?;
        // Empty authorized workspace = unscoped (admin) token; nothing to check against.
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }
        let agent = self.get_or_create(agent_id, &parsed.workspace_id).await?;
        let config = config_service::get_config(&self.db_pool, agent_id).await?;
        let enable_reflection = config.enable_reflection;
        let model = config.model.clone();
        let memory_service = self.memory_service.read().await.clone();
        let event_publisher = self.event_publisher.read().await.clone();
        chat_service::send_message(
            &agent,
            message,
            run_id,
            session_key,
            system_prompt,
            &self.chat_handles,
            memory_service,
            event_publisher,
            enable_reflection,
            &model,
            &parsed.workspace_id,
            agent_id,
            &self.db_pool,
        )
        .await
        .map_err(|e| AgentError::RequestFailed(e.to_string()))
    }

    pub async fn chat_history(
        &self,
        _agent_id: &str,
        session_key: &str,
        limit: u32,
        authorized_workspace: &str,
    ) -> Result<serde_json::Value, AgentError> {
        let parsed = crate::domains::agent::host::session::SessionKey::parse(session_key)?;
        if !authorized_workspace.is_empty() {
            parsed.verify_workspace(authorized_workspace)?;
        }

        // DB-backed, session-scoped history. The zeroclaw in-memory agent
        // history is shared across all sessions of the workspace agent and
        // cannot isolate them.
        let messages = crate::domains::agent::host::chat::history::list_messages(&self.db_pool, session_key, limit)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
        Ok(crate::domains::agent::host::chat::history::messages_to_history_json(
            messages, session_key,
        ))
    }

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

    pub async fn run_single(&self, workspace_id: &str, message: &str) -> Result<String, AgentError> {
        // Per-workspace agent key prevents cross-workspace tool context leak.
        // "__heartbeat__" has no DB row, so it always falls back to
        // AgentRuntimeConfig::default() → server-level [minimax] model.
        let agent_id = format!("__heartbeat__:{}", workspace_id);
        let agent = self.get_or_create(&agent_id, workspace_id).await?;
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
    pub async fn run_streaming(&self, workspace_id: &str, message: &str) -> Result<StreamingRunResult, AgentError> {
        let agent_id = format!("__heartbeat__:{}", workspace_id);
        let agent = self.get_or_create(&agent_id, workspace_id).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use std::sync::Arc;

    async fn test_db() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        pool
    }

    async fn test_agent_pool() -> AgentPool {
        let db = test_db().await;
        let memory_store: Arc<tinyiothub_storage::memory::MemoryStore> =
            Arc::new(tinyiothub_storage::memory::MemoryStore::new(db.clone()));
        AgentPool::new(
            db,
            memory_store,
            &tinyiothub_core::config::AgentSettings::default(),
            crate::domains::agent::host::autonomous_factory::minimax_provider_factory(),
        )
        .expect("test AgentPool")
    }

    #[tokio::test]
    async fn chat_send_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_send("agent_main", "agent:ws_other:agent_main/s1", "hi", "r1", "", "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
        // The workspace check must run before any agent is built.
        assert_eq!(pool.pool_size(), 0);
    }

    #[tokio::test]
    async fn chat_history_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_history("agent_main", "agent:ws_other:agent_main/s1", 50, "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }

    #[tokio::test]
    async fn chat_history_with_unscoped_token_reads_persisted_messages() {
        let pool = test_agent_pool().await;
        let key = "agent:ws1:agent_main/s1";
        crate::domains::agent::host::chat::history::ensure_session(&pool.db_pool, key, "ws1", "agent_main")
            .await
            .unwrap();
        crate::domains::agent::host::chat::history::append_message(&pool.db_pool, key, "user", "hello", "r1")
            .await
            .unwrap();

        // Empty authorized_workspace = unscoped (admin) token: no workspace
        // check, history served straight from the DB.
        let out = pool.chat_history("agent_main", key, 50, "").await.unwrap();
        assert!(out.to_string().contains("hello"));
    }

    #[tokio::test]
    async fn chat_abort_rejects_session_from_other_workspace() {
        let pool = test_agent_pool().await;
        let err = pool
            .chat_abort("agent_main", "agent:ws_other:agent_main/s1", Some("r1"), "ws_mine")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }

    #[tokio::test]
    async fn chat_abort_with_unknown_run_id_errors_and_none_run_id_is_noop() {
        let pool = test_agent_pool().await;
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
}
