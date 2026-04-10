// TinyIoTHub × ZeroClaw 深度集成运行时
//
// 使用 zeroclaw Agent::turn_streamed() 实现完整的多轮工具调用循环。

use std::sync::Arc;
use std::pin::Pin;
use tokio::sync::mpsc;
use async_trait::async_trait;

use crate::infrastructure::zeroclaw_agent::{AgentClient, AgentConfig, AgentError, AgentInfo};
use crate::api::mcp::tool_metadata::{name_infers_concurrency_safe, name_infers_destructive, name_infers_read_only, IoTToolMetadata, PermissionLevel};
use crate::api::mcp::tool_registry::ToolHandler;
use zeroclaw::tools::traits::{Tool, ToolResult};
use zeroclaw::memory::Memory;
use zeroclaw::observability::Observer;
use zeroclaw::agent::dispatcher::NativeToolDispatcher;
use zeroclaw::agent::TurnEvent;
use zeroclaw::providers::traits::ToolCall;

// ============================================================================
// TinyIoTHubAgentClient - zeroclaw Agent 驱动
// ============================================================================

/// TinyIoTHub 内置 Agent 客户端
pub struct TinyIoTHubAgentClient {
    db_pool: sqlx::SqlitePool,
    /// Provider 和 model 存储用于重建 Agent
    provider: Arc<std::sync::Mutex<Option<Box<dyn zeroclaw::providers::traits::Provider>>>>,
    model_name: String,
    /// zeroclaw Agent（需要 &mut 调用 turn_streamed）
    agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
}

impl TinyIoTHubAgentClient {
    /// 使用 zeroclaw 内置的 OpenAiCompatibleProvider（MiniMax 支持 system prompt merge）
    pub fn new(
        db_pool: sqlx::SqlitePool,
        provider: Box<dyn zeroclaw::providers::traits::Provider>,
        model_name: String,
    ) -> anyhow::Result<Self> {
        // 初始构建（工具列表可能为空，因为 MCP 还未注册）
        let mut tool_boxed: Vec<Box<dyn Tool>> = Vec::new();
        tool_boxed.push(Box::new(CanvasTool));

        // 尝试从 MCP registry 加载工具（可能失败，如果 registry 未初始化）
        // 注意：这里使用 try_write 因为需要在同步上下文中获取锁
        // 如果失败，工具将在后续调用 refresh_tools() 时加载

        let memory: Arc<dyn Memory> = Arc::new(zeroclaw::memory::NoneMemory::new());
        let observer: Arc<dyn Observer> = Arc::new(zeroclaw::observability::NoopObserver);
        let tool_dispatcher = Box::new(NativeToolDispatcher);

        let agent = zeroclaw::agent::Agent::builder()
            .provider(provider)
            .tools(tool_boxed)
            .memory(memory)
            .observer(observer)
            .tool_dispatcher(tool_dispatcher)
            .model_name(model_name.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("Agent build failed: {}", e))?;

        Ok(Self {
            db_pool,
            provider: Arc::new(std::sync::Mutex::new(None)),
            model_name,
            agent: Arc::new(tokio::sync::Mutex::new(agent)),
        })
    }

    /// 从当前 MCP registry 重新构建 Agent（注册完工具后调用）
    ///
    /// # Safety
    /// 此方法应在 MCP 工具注册完成后调用，且不应在 Agent 正在处理请求时调用
    pub async fn refresh_tools(&self) -> anyhow::Result<()> {
        // 构建工具列表
        let mut tool_boxed: Vec<Box<dyn Tool>> = Vec::new();

        // 添加 canvas 工具
        tool_boxed.push(Box::new(CanvasTool));

        // 从 MCP registry 添加 IoT 工具
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

        let memory: Arc<dyn Memory> = Arc::new(zeroclaw::memory::NoneMemory::new());
        let observer: Arc<dyn Observer> = Arc::new(zeroclaw::observability::NoopObserver);
        let tool_dispatcher = Box::new(NativeToolDispatcher);

        // 获取当前 provider - 需要从现有 agent 中提取或重新创建
        // 由于 Provider trait 没有 Clone，我们需要重新创建
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

impl AgentClient for TinyIoTHubAgentClient {
    fn create_agent(&self, _config: &AgentConfig) -> Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>> {
        Box::pin(async move { Ok("default".to_string()) })
    }

    fn delete_agent(&self, _agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn get_agent(&self, agent_id: &str) -> Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            Ok(AgentInfo { id: agent_id, name: "TinyIoTHub Built-in Agent".to_string(), status: "active".to_string(), created_at: None })
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
        let db_pool = self.db_pool.clone();
        let agent = Arc::clone(&self.agent);
        let system_prompt = system_prompt.to_string();

        Box::pin(async move {
            // 保存用户消息
            let user_content = serde_json::json!([{ "type": "text", "text": message }]);
            sqlx::query(
                "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
                 VALUES (?, 'default', datetime('now'), datetime('now'))
                 ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')",
            )
            .bind(&session_key)
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            sqlx::query(
                "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
                 VALUES (?, 'user', ?, ?, ?)",
            )
            .bind(&session_key)
            .bind(user_content.to_string())
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(&run_id)
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(e.to_string()))?;

            // 设置 system prompt（仅首次）
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
            let db_pool_clone = db_pool.clone();

            // 在后台运行 Agent::turn_streamed
            tokio::spawn(async move {
                let mut ag = agent.lock().await;

                // 发送 channel 给 turn_streamed（使用 mpsc）
                let (event_tx, event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(32);
                // 用 Arc<Mutex<>> 包装 receiver 以便主任务和 forwarder 都能访问
                let event_rx_shared = Arc::new(tokio::sync::Mutex::new(event_rx));
                let event_rx_main = Arc::clone(&event_rx_shared);
                let event_rx_fwd = Arc::clone(&event_rx_shared);

                // Helper closures that clone what's needed
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
                            // 打印工具并发安全信息（基于工具名称推断）
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

                // 独立任务：event_rx_fwd → SSE（不阻塞主循环）
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

                // 运行 turn_streamed（会内部执行工具循环），加 120s 超时保护
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

                        // 保存 assistant 消息
                        let assistant_content = serde_json::json!([{ "type": "text", "text": final_text }]);
                        let _ = sqlx::query(
                            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
                             VALUES (?, 'assistant', ?, ?, ?)",
                        )
                        .bind(&session_key_clone)
                        .bind(assistant_content.to_string())
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(&run_id_clone)
                        .execute(&db_pool_clone)
                        .await;

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
                        // 超时
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

    fn chat_abort(&self, _agent_id: &str, _session_key: &str, _run_id: Option<&str>) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn list_agents(&self) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move {
            Ok(serde_json::json!({
                "agents": [{
                    "id": "default",
                    "name": "TinyIoTHub Built-in Agent",
                    "status": "active",
                    "created_at": null,
                }]
            }))
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
                    .unwrap_or_else(|_| crate::infrastructure::zeroclaw_agent::default_agent_config());
                return Ok(serde_json::json!({ "config": config, "baseHash": config_hash }));
            }
            Ok(serde_json::json!({
                "config": crate::infrastructure::zeroclaw_agent::default_agent_config(),
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
            let config_hash = crate::infrastructure::zeroclaw_agent::compute_hash(&config);
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
            Ok(crate::infrastructure::zeroclaw_agent::build_tools_catalog_json())
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

            let catalog = crate::infrastructure::zeroclaw_agent::build_tools_catalog_json();
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
// CanvasTool - A2UI 工具
// ============================================================================

pub struct CanvasTool;

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str { "canvas" }
    fn description(&self) -> &str { "推送 A2UI 组件到前端渲染。使用此工具创建用户界面。" }
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
// IoTToolAdapter - MCP ToolHandler → zeroclaw Tool
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

    fn is_concurrency_safe(&self, input: &serde_json::Value) -> bool {
        name_infers_concurrency_safe(&self.name)
    }

    fn is_read_only(&self, input: &serde_json::Value) -> bool {
        name_infers_read_only(&self.name)
    }

    fn is_destructive(&self, input: &serde_json::Value) -> bool {
        name_infers_destructive(&self.name)
    }

    fn permission_level(&self, input: &serde_json::Value) -> PermissionLevel {
        // 危险操作需要询问
        if self.is_destructive(input) {
            PermissionLevel::Ask
        // 只读操作默认允许
        } else if self.is_read_only(input) {
            PermissionLevel::Allow
        // 其他操作询问
        } else {
            PermissionLevel::Ask
        }
    }
}

// ============================================================================
// Re-exports
// ============================================================================

pub use crate::infrastructure::zeroclaw_agent::{compute_hash, default_agent_config, platform_base_prompt, build_full_system_prompt};
