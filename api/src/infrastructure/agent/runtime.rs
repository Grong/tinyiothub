// Agent Runtime Implementation
//
// This module provides the AgentRuntimeImpl which implements the AgentRuntime trait
// using zeroclaw for AI Agent functionality.

use std::sync::Arc;
use std::pin::Pin;
use async_trait::async_trait;

use crate::infrastructure::agent::config::{AgentConfig, AgentError, AgentInfo};
use crate::infrastructure::agent::AgentClient;
use crate::infrastructure::agent::AgentRuntime;
use crate::api::mcp::tool_metadata::{name_infers_concurrency_safe, name_infers_destructive, name_infers_read_only, IoTToolMetadata, PermissionLevel};
use crate::api::mcp::tool_registry::ToolHandler;
use zeroclaw::tools::traits::{Tool, ToolResult};
use zeroclaw::memory::Memory;
use zeroclaw::observability::Observer;
use zeroclaw::agent::dispatcher::NativeToolDispatcher;
use zeroclaw::agent::TurnEvent;

// ============================================================================
// AgentRuntimeImpl - zeroclaw Agent driver
// ============================================================================

/// TinyIoTHub built-in Agent runtime implementation
pub struct AgentRuntimeImpl {
    db_pool: sqlx::SqlitePool,
    /// Provider and model stored for rebuilding Agent
    _provider: Arc<std::sync::Mutex<Option<Box<dyn zeroclaw::providers::traits::Provider>>>>,
    model_name: String,
    /// zeroclaw Agent (needs &mut to call turn_streamed)
    agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
    /// Persistent memory (SqliteMemory) — survives tool refresh
    memory: Arc<dyn zeroclaw::memory::Memory>,
    /// Observability observer — survives tool refresh
    observer: Arc<dyn zeroclaw::observability::Observer>,
    /// Active chat run handles for abort support
    chat_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl AgentRuntimeImpl {
    /// Create using zeroclaw's built-in OpenAiCompatibleProvider (MiniMax supports system prompt merge)
    pub fn new(
        db_pool: sqlx::SqlitePool,
        provider: Box<dyn zeroclaw::providers::traits::Provider>,
        model_name: String,
        agent_settings: &crate::infrastructure::config::AgentSettings,
    ) -> anyhow::Result<Self> {
        // Initial build (tool list may be empty because MCP not yet registered)
        let mut tool_boxed: Vec<Box<dyn Tool>> = Vec::new();
        tool_boxed.push(Box::new(CanvasTool));

        let workspace_dir = std::path::PathBuf::from(&agent_settings.workspace_dir);
        std::fs::create_dir_all(&workspace_dir).ok();

        let mut memory_config = zeroclaw::config::schema::MemoryConfig::default();
        memory_config.backend = agent_settings.memory_backend.clone();
        memory_config.auto_save = true;
        memory_config.hygiene_enabled = true;

        let memory = zeroclaw::memory::create_memory(&memory_config, &workspace_dir, None)
            .map_err(|e| anyhow::anyhow!("Failed to create memory backend '{}': {}", agent_settings.memory_backend, e))?;
        let memory: Arc<dyn Memory> = Arc::from(memory);

        let mut observer_config = zeroclaw::config::schema::ObservabilityConfig::default();
        observer_config.backend = agent_settings.observer_backend.clone();
        let observer = zeroclaw::observability::create_observer(&observer_config);
        let observer: Arc<dyn Observer> = Arc::from(observer);

        let tool_dispatcher = Box::new(NativeToolDispatcher);

        let agent = zeroclaw::agent::Agent::builder()
            .provider(provider)
            .tools(tool_boxed)
            .memory(Arc::clone(&memory))
            .observer(Arc::clone(&observer))
            .tool_dispatcher(tool_dispatcher)
            .model_name(model_name.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("Agent build failed: {}", e))?;

        Ok(Self {
            db_pool,
            _provider: Arc::new(std::sync::Mutex::new(None)),
            model_name,
            agent: Arc::new(tokio::sync::Mutex::new(agent)),
            memory,
            observer,
            chat_handles: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }

    /// Rebuild Agent from current MCP registry (call after tool registration)
    ///
    /// # Safety
    /// This should be called after MCP tool registration completes, and not while Agent is processing
    pub async fn refresh_tools_impl(&self) -> anyhow::Result<()> {
        // Build tool list
        let mut tool_boxed: Vec<Box<dyn Tool>> = Vec::new();

        // Add canvas tool
        tool_boxed.push(Box::new(CanvasTool));

        // Add IoT tools from MCP registry
        if let Some(registry) = crate::api::mcp::get_mcp_registry() {
            let reg = registry.write().await;
            let tool_metas = reg.list_tools();
            let tool_count = tool_metas.len();
            for meta in tool_metas {
                if meta.name.trim().is_empty() { continue; }
                let name = meta.name.clone();
                let description = meta.description.clone();
                let input_schema = meta.input_schema.clone();
                if let Some(handler) = reg.get_owned(&name) {
                    tool_boxed.push(Box::new(IoTToolAdapter::new(name, description, input_schema, handler)));
                }
            }
            tracing::info!("Loaded {} tools from MCP registry", tool_count);
        } else {
            tracing::warn!("MCP registry not available, no IoT tools loaded");
        }

        let memory = Arc::clone(&self.memory);
        let observer = Arc::clone(&self.observer);
        let tool_dispatcher = Box::new(NativeToolDispatcher);

        // Get current provider - need to recreate since Provider trait doesn't have Clone
        let minimax_config = crate::infrastructure::config::get()
            .minimax
            .clone()
            .expect("minimax config required");

        let provider = zeroclaw::providers::create_provider(
            "minimaxi",
            Some(&minimax_config.auth_token)
        ).map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))?;

        let agent = zeroclaw::agent::Agent::builder()
            .provider(provider)
            .tools(tool_boxed)
            .memory(memory)
            .observer(observer)
            .tool_dispatcher(tool_dispatcher)
            .model_name(self.model_name.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("Agent build failed: {}", e))?;

        let mut guard = self.agent.lock().await;
        *guard = agent;
        drop(guard);

        tracing::info!("Agent tools refreshed successfully");
        Ok(())
    }
}

impl AgentClient for AgentRuntimeImpl {
    fn create_agent(&self, config: &AgentConfig) -> Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>> {
        let db_pool = self.db_pool.clone();
        let workspace_id = config.workspace_id.clone();
        let name = config.name.clone();
        Box::pin(async move {
            let agent_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
            )
            .bind(&agent_id)
            .bind(&workspace_id)
            .bind(&name)
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
            Ok(agent_id)
        })
    }

    fn delete_agent(&self, agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let result = sqlx::query("DELETE FROM agents WHERE agent_id = ?")
                .bind(&agent_id)
                .execute(&db_pool)
                .await
                .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
            if result.rows_affected() == 0 {
                return Err(AgentError::NotFound(agent_id));
            }
            let _ = sqlx::query("DELETE FROM agent_configs WHERE agent_id = ?")
                .bind(&agent_id)
                .execute(&db_pool)
                .await;
            let _ = sqlx::query("DELETE FROM agent_tools WHERE agent_id = ?")
                .bind(&agent_id)
                .execute(&db_pool)
                .await;
            Ok(())
        })
    }

    fn get_agent(&self, agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let row: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT agent_id, workspace_id, name, status FROM agents WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            match row {
                Some((id, _workspace, name, status)) => Ok(AgentInfo {
                    id,
                    name,
                    status,
                    created_at: None,
                }),
                None => Err(AgentError::NotFound(agent_id)),
            }
        })
    }

    fn update_agent(&self, _agent_id: &str, _config: &str) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn chat_send(
        &self,
        _agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();
        let message = message.to_string();
        let run_id = run_id.to_string();
        let agent = Arc::clone(&self.agent);
        let system_prompt = system_prompt.to_string();
        let chat_handles = Arc::clone(&self.chat_handles);

        Box::pin(async move {
            // Set system prompt (only first time)
            {
                let mut ag = agent.lock().await;
                if ag.history().is_empty() && !system_prompt.is_empty() {
                    ag.seed_history(&[
                        zeroclaw::providers::traits::ChatMessage {
                            role: "system".into(),
                            content: system_prompt,
                        },
                    ]);
                }
            }

            // SSE channel
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<bytes::Bytes, std::io::Error>>();
            let tx_stream = tx.clone();
            let session_key_clone = session_key.clone();
            let run_id_clone = run_id.clone();

            // Guard to clean up chat_handles entry when the task finishes or is aborted
            struct ChatRunGuard {
                chat_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
                run_id: String,
            }
            impl Drop for ChatRunGuard {
                fn drop(&mut self) {
                    let chat_handles = self.chat_handles.clone();
                    let run_id = self.run_id.clone();
                    tokio::spawn(async move {
                        chat_handles.lock().await.remove(&run_id);
                    });
                }
            }

            let chat_handles_for_spawn = chat_handles.clone();
            let run_id_for_spawn = run_id.clone();

            // Run Agent::turn_streamed in background
            let handle = tokio::spawn(async move {
                let _guard = ChatRunGuard {
                    chat_handles: chat_handles_for_spawn.clone(),
                    run_id: run_id_for_spawn.clone(),
                };

                let mut ag = agent.lock().await;

                // Send channel to turn_streamed (using mpsc)
                let (event_tx, event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(32);
                let event_rx_shared = Arc::new(tokio::sync::Mutex::new(event_rx));
                let event_rx_main = Arc::clone(&event_rx_shared);
                let event_rx_fwd = Arc::clone(&event_rx_shared);

                let forward_sse = |evt: &TurnEvent, forward_run: &str, forward_session: &str| -> bytes::Bytes {
                    let sse_data = match evt {
                        TurnEvent::Chunk { delta } => {
                            serde_json::json!({
                                "runId": forward_run,
                                "sessionKey": forward_session,
                                "state": "delta",
                                "message": {
                                    "role": "assistant",
                                    "content": [{ "type": "text", "text": delta }],
                                }
                            })
                        }
                        TurnEvent::Thinking { delta } => {
                            serde_json::json!({
                                "runId": forward_run,
                                "sessionKey": forward_session,
                                "state": "thinking",
                                "thinking": delta,
                            })
                        }
                        TurnEvent::ToolCall { name, args } => {
                            // Log tool concurrency safety info (inferred from tool name)
                            let is_safe = name_infers_concurrency_safe(name);
                            let is_readonly = name_infers_read_only(name);
                            let is_destructive = name_infers_destructive(name);
                            tracing::info!(
                                "Tool call: {} (concurrency_safe: {}, read_only: {}, destructive: {})",
                                name, is_safe, is_readonly, is_destructive
                            );
                            let args_str = serde_json::to_string(&args).unwrap_or_default();
                            let a2ui_jsonl = if name == "canvas" {
                                args.get("jsonl")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            };
                            serde_json::json!({
                                "runId": forward_run,
                                "sessionKey": forward_session,
                                "state": "tool_call_start",
                                "toolName": name,
                                "toolArgs": args_str,
                                "a2ui": if a2ui_jsonl.is_empty() { serde_json::Value::Null } else { serde_json::json!(a2ui_jsonl) },
                            })
                        }
                        TurnEvent::ToolResult { name, output } => {
                            serde_json::json!({
                                "runId": forward_run,
                                "sessionKey": forward_session,
                                "state": "tool_result",
                                "toolName": name,
                                "result": output,
                            })
                        }
                    };
                    bytes::Bytes::from(format!("data: {}\n", sse_data))
                };

                // Separate task: event_rx_fwd -> SSE (doesn't block main loop)
                let forward_tx = tx_stream.clone();
                let forward_run = run_id_clone.clone();
                let forward_session = session_key_clone.clone();
                tokio::spawn(async move {
                    let mut rx = event_rx_fwd.lock().await;
                    while let Some(evt) = rx.recv().await {
                        let sse_bytes = forward_sse(&evt, &forward_run, &forward_session);
                        let _ = forward_tx.send(Ok(sse_bytes));
                    }
                });

                // Run turn_streamed (executes tool loop internally), with 120s timeout
                match tokio::time::timeout(std::time::Duration::from_secs(120), ag.turn_streamed(&message, event_tx)).await {
                    Ok(Ok(final_text)) => {
                        // Drain any remaining tool result events from the main receiver
                        {
                            let mut rx = event_rx_main.lock().await;
                            while let Ok(evt) = rx.try_recv() {
                                let sse_bytes = forward_sse(&evt, &run_id_clone, &session_key_clone);
                                let _ = tx_stream.send(Ok(sse_bytes));
                            }
                        }

                        let sse_final = serde_json::json!({
                            "runId": run_id_clone,
                            "sessionKey": session_key_clone,
                            "state": "final",
                            "message": {
                                "role": "assistant",
                                "content": [{ "type": "text", "text": final_text }],
                            }
                        });
                        let _ = tx_stream.send(Ok(bytes::Bytes::from(format!("data: {}\n\n", sse_final))));
                    }
                    Ok(Err(e)) => {
                        let err_json = serde_json::json!({
                            "runId": run_id_clone,
                            "sessionKey": session_key_clone,
                            "state": "error",
                            "error": e.to_string(),
                        });
                        let _ = tx_stream.send(Ok(bytes::Bytes::from(format!("data: {}\n\n", err_json))));
                    }
                    Err(_) => {
                        // Timeout
                        let err_json = serde_json::json!({
                            "runId": run_id_clone,
                            "sessionKey": session_key_clone,
                            "state": "error",
                            "error": "Agent execution timed out after 120 seconds",
                        });
                        let _ = tx_stream.send(Ok(bytes::Bytes::from(format!("data: {}\n\n", err_json))));
                    }
                }
            });

            chat_handles.lock().await.insert(run_id, handle);

            let http_response = http::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .header("cache-control", "no-cache")
                .header("connection", "keep-alive")
                .body(reqwest::Body::wrap_stream(
                    tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
                ))
                .map_err(|e| AgentError::RequestFailed(format!("SSE build error: {}", e)))?;

            Ok(reqwest::Response::from(http_response))
        })
    }

    fn chat_history(&self, _agent_id: &str, session_key: &str, limit: u32) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            // First check for any system messages (for debugging)
            let system_count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM chat_messages WHERE session_key = ? AND role = 'system'",
            )
            .bind(&session_key)
            .fetch_one(&db_pool)
            .await
            .unwrap_or((0,));

            if system_count.0 > 0 {
                tracing::warn!("Found {} system messages in session {}", system_count.0, session_key);
            }

            let rows = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
                "SELECT role, content, timestamp, run_id FROM chat_messages
                 WHERE session_key = ? AND role != 'system' ORDER BY timestamp ASC LIMIT ?",
            )
            .bind(&session_key)
            .bind(limit as i64)
            .fetch_all(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            let messages: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|row: (String, String, i64, Option<String>)| {
                    let content_parsed: serde_json::Value = serde_json::from_str(&row.1)
                        .unwrap_or_else(|_| serde_json::json!([{ "type": "text", "text": &row.1 }]));
                    serde_json::json!({
                        "role": row.0,
                        "content": content_parsed,
                        "timestamp": row.2,
                        "toolCallId": row.3,
                    })
                })
                .collect();

            Ok(serde_json::json!({ "messages": messages }))
        })
    }

    fn chat_abort(
        &self,
        _agent_id: &str,
        _session_key: &str,
        run_id: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let run_id = run_id.map(String::from);
        let chat_handles = Arc::clone(&self.chat_handles);
        Box::pin(async move {
            if let Some(rid) = run_id {
                if let Some(handle) = chat_handles.lock().await.remove(&rid) {
                    handle.abort();
                    tracing::info!("Aborted chat run {}", rid);
                }
            }
            Ok(())
        })
    }

    fn list_agents(&self) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT agent_id, workspace_id, name, status FROM agents ORDER BY created_at DESC"
            )
            .fetch_all(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            let agents: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(id, _workspace, name, status)| {
                    serde_json::json!({
                        "id": id,
                        "name": name,
                        "status": status,
                        "created_at": null,
                    })
                })
                .collect();

            Ok(serde_json::json!({ "agents": agents }))
        })
    }

    fn get_agent_config(&self, agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let row = sqlx::query_as::<_, (String, String)>(
                "SELECT config, config_hash FROM agent_configs WHERE agent_id = ?",
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            if let Some((config_str, config_hash)) = row {
                let config: serde_json::Value = serde_json::from_str(&config_str)
                    .unwrap_or_else(|_| crate::infrastructure::agent::config::default_agent_config());
                return Ok(serde_json::json!({ "config": config, "baseHash": config_hash }));
            }
            Ok(serde_json::json!({
                "config": crate::infrastructure::agent::config::default_agent_config(),
                "baseHash": null,
            }))
        })
    }

    fn set_agent_config(&self, agent_id: &str, config: &str, _base_hash: Option<&str>) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let config = config.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let _: serde_json::Value = serde_json::from_str(&config)
                .map_err(|e| AgentError::RequestFailed(format!("Invalid config: {}", e)))?;
            let config_hash = crate::infrastructure::agent::config::compute_hash(&config);
            sqlx::query(
                "INSERT INTO agent_configs (agent_id, config, config_hash, updated_at)
                 VALUES (?, ?, ?, datetime('now'))
                 ON CONFLICT(agent_id) DO UPDATE SET
                   config = excluded.config,
                   config_hash = excluded.config_hash,
                   updated_at = datetime('now')",
            )
            .bind(&agent_id)
            .bind(&config)
            .bind(&config_hash)
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;
            Ok(())
        })
    }

    fn tools_catalog(&self, _agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move {
            Ok(crate::infrastructure::agent::build_tools_catalog_json())
        })
    }

    fn tools_effective(&self, agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let overrides_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?",
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            let overrides: serde_json::Value = overrides_row
                .map(|row: (String,)| serde_json::from_str(&row.0).unwrap_or_default())
                .unwrap_or_else(|| serde_json::json!({ "enabled": [], "disabled": [] }));

            let enabled_list: Vec<String> = overrides
                .get("enabled")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let disabled_list: Vec<String> = overrides
                .get("disabled")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let catalog = crate::infrastructure::agent::build_tools_catalog_json();
            let groups = catalog.get("groups").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            let filtered_groups: Vec<serde_json::Value> = groups
                .into_iter()
                .map(|group| {
                    let tools = group.get("tools").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let filtered_tools: Vec<serde_json::Value> = tools
                        .into_iter()
                        .map(|mut tool| {
                            let tool_id = tool.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let is_dangerous = tool.get("danger").and_then(|v| v.as_bool()).unwrap_or(false);
                            let effective_enabled = if !enabled_list.is_empty() {
                                enabled_list.contains(&tool_id)
                            } else if !disabled_list.is_empty() {
                                !disabled_list.contains(&tool_id)
                            } else {
                                !is_dangerous
                            };
                            tool["enabled"] = serde_json::json!(effective_enabled);
                            tool
                        })
                        .collect();
                    serde_json::json!({
                        "id": group.get("id"),
                        "label": group.get("label"),
                        "source": group.get("source"),
                        "tools": filtered_tools,
                    })
                })
                .collect();

            Ok(serde_json::json!({ "groups": filtered_groups }))
        })
    }

    fn tools_toggle(&self, agent_id: &str, tool_name: &str, enabled: bool) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let tool_name = tool_name.to_string();
        let db_pool = self.db_pool.clone();
        Box::pin(async move {
            let current_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?",
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            let overrides: serde_json::Value = current_row
                .map(|row: (String,)| serde_json::from_str(&row.0).unwrap_or_default())
                .unwrap_or_else(|| serde_json::json!({ "enabled": [], "disabled": [] }));

            let mut enabled_list: Vec<String> = overrides
                .get("enabled")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let mut disabled_list: Vec<String> = overrides
                .get("disabled")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            enabled_list.retain(|t| t != &tool_name);
            disabled_list.retain(|t| t != &tool_name);
            if enabled {
                enabled_list.push(tool_name.clone());
            } else {
                disabled_list.push(tool_name);
            }

            let new_overrides = serde_json::json!({ "enabled": enabled_list, "disabled": disabled_list });

            sqlx::query(
                "INSERT INTO agent_tools (agent_id, tool_overrides, updated_at)
                 VALUES (?, ?, datetime('now'))
                 ON CONFLICT(agent_id) DO UPDATE SET
                   tool_overrides = excluded.tool_overrides,
                   updated_at = datetime('now')",
            )
            .bind(&agent_id)
            .bind(new_overrides.to_string())
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            Ok(())
        })
    }
}

// ============================================================================
// AgentRuntime Implementation
// ============================================================================

impl AgentRuntime for AgentRuntimeImpl {
    fn refresh_tools(&self) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move { self.refresh_tools_impl().await })
    }
}

// ============================================================================
// CanvasTool - A2UI Tool
// ============================================================================

pub struct CanvasTool;

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str { "canvas" }
    fn description(&self) -> &str { "Push A2UI components to frontend rendering. Use this tool to create user interfaces." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["a2ui_push"] },
                "jsonl": { "type": "string" },
            },
            "required": ["action", "jsonl"],
        })
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let jsonl = args.get("jsonl").and_then(|v| v.as_str()).unwrap_or("");
        if action == "a2ui_push" {
            Ok(ToolResult { success: true, output: format!("A2UI pushed: {} bytes", jsonl.len()), error: None })
        } else {
            Ok(ToolResult { success: false, output: String::new(), error: Some("Unknown action".into()) })
        }
    }
}

// ============================================================================
// IoTToolAdapter - MCP ToolHandler -> zeroclaw Tool
// ============================================================================

pub struct IoTToolAdapter {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    handler: Arc<dyn ToolHandler>,
}

impl IoTToolAdapter {
    pub fn new(name: String, description: String, input_schema: serde_json::Value, handler: Arc<dyn ToolHandler>) -> Self {
        Self { name, description, input_schema, handler }
    }
}

#[async_trait]
impl Tool for IoTToolAdapter {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn parameters_schema(&self) -> serde_json::Value { self.input_schema.clone() }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        tracing::info!("Executing IoT tool: {} with args: {}", self.name, args);
        match self.handler.execute(args).await {
            Ok(output) => {
                let output_str = serde_json::to_string(&output).unwrap_or_default();
                tracing::info!("IoT tool {} succeeded: output length = {}", self.name, output_str.len());
                Ok(ToolResult {
                    success: true,
                    output: output_str,
                    error: None,
                })
            }
            Err(err) => {
                tracing::error!("IoT tool {} failed: {}", self.name, err);
                Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(err.to_string()),
                })
            }
        }
    }
}

impl IoTToolMetadata for IoTToolAdapter {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn input_schema(&self) -> serde_json::Value { self.input_schema.clone() }

    fn is_concurrency_safe(&self, _input: &serde_json::Value) -> bool {
        name_infers_concurrency_safe(&self.name)
    }

    fn is_read_only(&self, _input: &serde_json::Value) -> bool {
        name_infers_read_only(&self.name)
    }

    fn is_destructive(&self, _input: &serde_json::Value) -> bool {
        name_infers_destructive(&self.name)
    }

    fn permission_level(&self, input: &serde_json::Value) -> PermissionLevel {
        // Dangerous operations require asking
        if self.is_destructive(input) {
            PermissionLevel::Ask
        // Read-only operations allowed by default
        } else if self.is_read_only(input) {
            PermissionLevel::Allow
        // Other operations require asking
        } else {
            PermissionLevel::Ask
        }
    }
}
