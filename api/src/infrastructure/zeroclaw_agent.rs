// Agent Client — adapter between TinyIoTHub HTTP API and AI Agent backends (ZeroClaw, etc.)
//
// The agent backend (e.g. ZeroClaw) uses WebSocket (/ws/chat) for chat.
// This adapter receives HTTP POST+SSE from the frontend and translates to WS frames.
// Chat history is stored locally in SQLite.
//
// Two implementations:
// - ZeroClawAgentClient: connects to a real ZeroClaw Gateway via WebSocket
// - FallbackAgentClient: in-memory fallback when no agent backend is available

use std::pin::Pin;
use std::time::Duration;

use crate::domain::agent::skill::AgentSkill;
use futures_util::{SinkExt, StreamExt};
use hex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Errors from Agent operations
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent API request failed: {0}")]
    RequestFailed(String),
    #[error("Agent API returned error: {0}")]
    ApiError(String),
    #[error("Agent API timeout")]
    Timeout,
    #[error("Agent unavailable: {0}")]
    Unavailable(String),
    #[error("agent not found: {0}")]
    NotFound(String),
}

/// Agent configuration passed when creating an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl AgentConfig {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            workspace_id: String::new(),
            name: String::new(),
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            system_prompt: None,
        }
    }
}

/// Default agent config returned when no persisted config exists
pub fn default_agent_config() -> serde_json::Value {
    serde_json::json!({
        "model": "claude-sonnet-4-5",
        "temperature": 0.7,
        "maxTokens": 4096,
        "topP": 1.0,
        "systemPrompt": "",
        "workspace": "default"
    })
}

/// Compute SHA-256 hex digest of a string
pub fn compute_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    Digest::update(&mut hasher, s.as_bytes());
    hex::encode(hasher.finalize())
}

/// Agent info returned on creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: Option<String>,
}

/// API response wrapper
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: Option<T>,
    error: Option<String>,
}

/// Trait for Agent operations — supports ZeroClaw and fallback implementations
pub trait AgentClient: Send + Sync {
    /// Create a new agent for the given workspace
    fn create_agent(
        &self,
        config: &AgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>>;

    /// Delete an agent by ID
    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Get agent info by ID
    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>>;

    /// Update agent configuration
    fn update_agent(
        &self,
        agent_id: &str,
        config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Send a chat message and get SSE stream response
    fn chat_send(
        &self,
        agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        system_prompt: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, AgentError>> + Send + '_>>;

    /// Get chat history
    fn chat_history(
        &self,
        agent_id: &str,
        session_key: &str,
        limit: u32,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Abort a chat run
    fn chat_abort(
        &self,
        agent_id: &str,
        session_key: &str,
        run_id: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// List all agents
    fn list_agents(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Get agent config
    fn get_agent_config(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Set agent config
    fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        base_hash: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Get tools catalog for an agent
    fn tools_catalog(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Get effective tools for an agent
    fn tools_effective(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>>;

    /// Toggle a tool on/off for an agent
    fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>>;
}

// ============================================================================
// ZeroClaw WebSocket message types
// ============================================================================

#[derive(Debug, Deserialize)]
struct ZeroClawIncoming {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    full_response: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

// ============================================================================
// ZeroClaw Agent Client — WebSocket adapter
// ============================================================================

/// ZeroClaw Agent client — talks to ZeroClaw Gateway via WebSocket + HTTP
pub struct ZeroClawAgentClient {
    http_client: Client,
    base_url: String,
    ws_url: String,
    gateway_token: Option<String>,
    db_pool: sqlx::SqlitePool,
    /// Active WebSocket connections keyed by session_key (for abort)
    active_connections: Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl Clone for ZeroClawAgentClient {
    fn clone(&self) -> Self {
        ZeroClawAgentClient {
            http_client: self.http_client.clone(),
            base_url: self.base_url.clone(),
            ws_url: self.ws_url.clone(),
            gateway_token: self.gateway_token.clone(),
            db_pool: self.db_pool.clone(),
            active_connections: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl ZeroClawAgentClient {
    pub fn new(base_url: String, ws_url: String, gateway_token: Option<String>, db_pool: sqlx::SqlitePool) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            base_url,
            ws_url,
            gateway_token,
            db_pool,
            active_connections: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Build WS URL for a chat session
    fn build_ws_url(&self, session_key: &str) -> String {
        let mut url = format!(
            "{}/ws/chat?session_id={}",
            self.ws_url.trim_end_matches('/'),
            urlencoding::encode(session_key)
        );
        if let Some(token) = &self.gateway_token {
            url.push_str(&format!("&token={}", urlencoding::encode(token)));
        }
        url
    }

    /// Make an authenticated HTTP request to ZeroClaw
    async fn http_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<reqwest::Response, AgentError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut request = self.http_client.request(method, &url);
        request = request.header("Content-Type", "application/json");
        if let Some(ref token) = self.gateway_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().await.map_err(|e| {
            if e.is_timeout() {
                AgentError::Timeout
            } else {
                AgentError::Unavailable(e.to_string())
            }
        })
    }

    /// Persist a chat message to local SQLite
    async fn persist_message(&self, session_key: &str, role: &str, content: &serde_json::Value, run_id: Option<&str>) {
        let content_str = content.to_string();
        let timestamp = chrono::Utc::now().timestamp_millis();
        let run_id_val = run_id.unwrap_or("");

        // Upsert session
        let _ = sqlx::query(
            "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
             VALUES (?, 'default', datetime('now'), datetime('now'))
             ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')"
        )
        .bind(session_key)
        .execute(&self.db_pool)
        .await;

        // Insert message
        let _ = sqlx::query(
            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(session_key)
        .bind(role)
        .bind(&content_str)
        .bind(timestamp)
        .bind(run_id_val)
        .execute(&self.db_pool)
        .await;
    }
}

impl AgentClient for ZeroClawAgentClient {
    fn create_agent(
        &self,
        _config: &AgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>> {
        // ZeroClaw is single-agent — return default
        Box::pin(async move { Ok("default".to_string()) })
    }

    fn delete_agent(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        // No-op for single-agent
        Box::pin(async move { Ok(()) })
    }

    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            Ok(AgentInfo {
                id: agent_id,
                name: "ZeroClaw Agent".to_string(),
                status: "active".to_string(),
                created_at: None,
            })
        })
    }

    fn update_agent(
        &self,
        _agent_id: &str,
        _config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn chat_send(
        &self,
        _agent_id: &str,
        session_key: &str,
        message: &str,
        run_id: &str,
        _system_prompt: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, AgentError>> + Send + '_>> {
        let ws_url = self.build_ws_url(session_key);
        let message = message.to_string();
        let session_key = session_key.to_string();
        let run_id = run_id.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            // Persist user message
            let user_content = serde_json::json!([{"type": "text", "text": message}]);
            let _ = sqlx::query(
                "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
                 VALUES (?, 'default', datetime('now'), datetime('now'))
                 ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')"
            )
            .bind(&session_key)
            .execute(&db_pool)
            .await;
            let _ = sqlx::query(
                "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
                 VALUES (?, 'user', ?, ?, ?)"
            )
            .bind(&session_key)
            .bind(user_content.to_string())
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(&run_id)
            .execute(&db_pool)
            .await;

            // Connect to ZeroClaw WebSocket
            let (ws_stream, _) = connect_async(&ws_url).await.map_err(|e| {
                AgentError::Unavailable(format!("WebSocket connect failed: {}", e))
            })?;

            let (mut write, mut read) = ws_stream.split();

            // Send the user message to ZeroClaw
            let ws_msg = serde_json::json!({
                "type": "message",
                "content": message,
            });
            write.send(Message::Text(ws_msg.to_string().into())).await.map_err(|e| {
                AgentError::RequestFailed(format!("WebSocket send failed: {}", e))
            })?;

            // Build a streaming response from WS frames
            // We create a channel that converts WS frames → SSE text lines
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<String, std::io::Error>>();

            let session_key_clone = session_key.clone();
            let run_id_clone = run_id.clone();
            let db_pool_clone = db_pool.clone();

            let _handle = tokio::spawn(async move {
                let mut full_text = String::new();

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(zc_msg) = serde_json::from_str::<ZeroClawIncoming>(&text) {
                                match zc_msg.msg_type.as_str() {
                                    "chunk" => {
                                        let content = zc_msg.content.unwrap_or_default();
                                        full_text.push_str(&content);
                                        // Emit as SSE delta event
                                        let sse_data = serde_json::json!({
                                            "runId": run_id_clone,
                                            "sessionKey": session_key_clone,
                                            "state": "delta",
                                            "message": {
                                                "role": "assistant",
                                                "content": [{"type": "text", "text": content}],
                                            }
                                        });
                                        let _ = tx.send(Ok(format!("data: {}\n", sse_data)));
                                    }
                                    "done" => {
                                        let final_text = zc_msg.full_response.unwrap_or(full_text.clone());
                                        // Persist assistant message
                                        let assistant_content = serde_json::json!([{"type": "text", "text": final_text}]);
                                        let _ = sqlx::query(
                                            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
                                             VALUES (?, 'assistant', ?, ?, ?)"
                                        )
                                        .bind(&session_key_clone)
                                        .bind(assistant_content.to_string())
                                        .bind(chrono::Utc::now().timestamp_millis())
                                        .bind(&run_id_clone)
                                        .execute(&db_pool_clone)
                                        .await;

                                        // Emit as SSE final event
                                        let sse_data = serde_json::json!({
                                            "runId": run_id_clone,
                                            "sessionKey": session_key_clone,
                                            "state": "final",
                                            "message": {
                                                "role": "assistant",
                                                "content": [{"type": "text", "text": final_text}],
                                            }
                                        });
                                        let _ = tx.send(Ok(format!("data: {}\n", sse_data)));
                                        break;
                                    }
                                    "error" => {
                                        let err_msg = zc_msg.message.unwrap_or_else(|| "Unknown error".to_string());
                                        let sse_data = serde_json::json!({
                                            "runId": run_id_clone,
                                            "sessionKey": session_key_clone,
                                            "state": "error",
                                            "errorMessage": err_msg,
                                        });
                                        let _ = tx.send(Ok(format!("data: {}\n", sse_data)));
                                        break;
                                    }
                                    _ => {
                                        // Unknown frame type — skip
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("WebSocket error: {}", e),
                            )));
                            break;
                        }
                        _ => {
                            // Binary, Ping, Pong — skip
                        }
                    }
                }
            });

            // Convert the channel receiver into a stream, then into a reqwest::Response
            let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
            let mapped = stream.map(|r| r.map(bytes::Bytes::from));
            let pinned: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send>>
                = Box::pin(mapped);

            // Build a synthetic HTTP response with streaming body
            let http_response = http::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::wrap_stream(pinned))
                .map_err(|e| AgentError::RequestFailed(format!("Failed to build response: {}", e)))?;

            // Convert http::Response into reqwest::Response
            let response = reqwest::Response::from(http_response);
            Ok(response)
        })
    }

    fn chat_history(
        &self,
        _agent_id: &str,
        session_key: &str,
        limit: u32,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let rows = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
                "SELECT role, content, timestamp, run_id FROM chat_messages
                 WHERE session_key = ? ORDER BY timestamp ASC LIMIT ?"
            )
            .bind(&session_key)
            .bind(limit as i64)
            .fetch_all(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let messages: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(role, content, timestamp, run_id)| {
                    let content_parsed: serde_json::Value = serde_json::from_str(&content)
                        .unwrap_or(serde_json::json!([{"type": "text", "text": content}]));
                    serde_json::json!({
                        "role": role,
                        "content": content_parsed,
                        "timestamp": timestamp,
                        "toolCallId": run_id,
                    })
                })
                .collect();

            Ok(serde_json::json!({ "messages": messages }))
        })
    }

    fn chat_abort(
        &self,
        _agent_id: &str,
        session_key: &str,
        _run_id: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();

        Box::pin(async move {
            // TODO: Implement WS connection tracking for mid-stream abort
            Ok(())
        })
    }

    fn list_agents(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move {
            Ok(serde_json::json!({
                "agents": [{
                    "id": "default",
                    "name": "ZeroClaw Agent",
                    "status": "active",
                    "created_at": null
                }]
            }))
        })
    }

    fn get_agent_config(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            // Try local SQLite first
            let row = sqlx::query_as::<_, (String, String)>(
                "SELECT config, config_hash FROM agent_configs WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            if let Some((config_str, config_hash)) = row {
                let config: serde_json::Value = serde_json::from_str(&config_str)
                    .unwrap_or_else(|_| default_agent_config());
                return Ok(serde_json::json!({
                    "config": config,
                    "baseHash": config_hash,
                }));
            }

            // Return default config if not found
            Ok(serde_json::json!({
                "config": default_agent_config(),
                "baseHash": null,
            }))
        })
    }

    fn set_agent_config(
        &self,
        agent_id: &str,
        config: &str,
        _base_hash: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let config = config.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            // Validate JSON before storing
            let _: serde_json::Value = serde_json::from_str(&config)
                .map_err(|e| AgentError::RequestFailed(format!("Invalid config JSON: {}", e)))?;

            let config_hash = compute_hash(&config);

            sqlx::query(
                "INSERT INTO agent_configs (agent_id, config, config_hash, updated_at)
                 VALUES (?, ?, ?, datetime('now'))
                 ON CONFLICT(agent_id) DO UPDATE SET
                   config = excluded.config,
                   config_hash = excluded.config_hash,
                   updated_at = datetime('now')"
            )
            .bind(&agent_id)
            .bind(&config)
            .bind(&config_hash)
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB upsert failed: {}", e)))?;

            Ok(())
        })
    }

    fn tools_catalog(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(build_tools_catalog_json()) })
    }

    fn tools_effective(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let overrides_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let overrides: serde_json::Value = overrides_row
                .map(|(json_str,)| serde_json::from_str(&json_str).unwrap_or_default())
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

            let catalog = build_tools_catalog_json();
            let groups = catalog
                .get("groups")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let filtered_groups: Vec<serde_json::Value> = groups
                .into_iter()
                .map(|group| {
                    let tools = group
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let filtered_tools: Vec<serde_json::Value> = tools
                        .into_iter()
                        .map(|mut tool| {
                            let tool_id = tool
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let is_dangerous = tool
                                .get("danger")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

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

    fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let tool_name = tool_name.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let current_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let overrides: serde_json::Value = current_row
                .map(|(json_str,)| serde_json::from_str(&json_str).unwrap_or_default())
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

            let new_overrides = serde_json::json!({
                "enabled": enabled_list,
                "disabled": disabled_list,
            });

            sqlx::query(
                "INSERT INTO agent_tools (agent_id, tool_overrides, updated_at)
                 VALUES (?, ?, datetime('now'))
                 ON CONFLICT(agent_id) DO UPDATE SET
                   tool_overrides = excluded.tool_overrides,
                   updated_at = datetime('now')"
            )
            .bind(&agent_id)
            .bind(new_overrides.to_string())
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB upsert failed: {}", e)))?;

            Ok(())
        })
    }
}

/// Returns the static catalog of all available TinyIoTHub tools grouped by category.
pub fn build_tools_catalog_json() -> serde_json::Value {
    serde_json::json!({
        "groups": [
            {
                "id": "device",
                "label": "设备管理",
                "source": "core",
                "tools": [
                    { "id": "device_list",           "name": "device_list",           "label": "查询设备列表",     "description": "列出所有已注册的IoT设备，支持分页和过滤",           "danger": false, "enabled": true  },
                    { "id": "device_get",             "name": "device_get",             "label": "获取设备详情",     "description": "根据设备ID获取设备完整信息",                          "danger": false, "enabled": true  },
                    { "id": "device_create",          "name": "device_create",          "label": "创建设备",         "description": "注册一个新的IoT设备到系统",                           "danger": false, "enabled": true  },
                    { "id": "device_update",          "name": "device_update",          "label": "更新设备",         "description": "更新已有设备的基本信息或配置",                       "danger": false, "enabled": true  },
                    { "id": "device_delete",          "name": "device_delete",          "label": "删除设备",         "description": "永久删除一个设备及其所有数据",                        "danger": true,  "enabled": false },
                    { "id": "device_read",            "name": "device_read",            "label": "读取设备属性",     "description": "从设备读取当前属性/遥测数据",                       "danger": false, "enabled": true  },
                    { "id": "device_write",           "name": "device_write",           "label": "写入设备属性",     "description": "向设备写入属性值或下发控制指令",                    "danger": false, "enabled": true  },
                    { "id": "device_batch_read",      "name": "device_batch_read",      "label": "批量读取设备",     "description": "批量读取多个设备的属性数据",                       "danger": false, "enabled": true  },
                    { "id": "batch_command_execute",  "name": "batch_command_execute",  "label": "批量执行命令",     "description": "向多个设备批量下发控制命令",                        "danger": true,  "enabled": false },
                    { "id": "batch_property_write",   "name": "batch_property_write",   "label": "批量写入属性",     "description": "批量写入多个设备的属性值",                          "danger": true,  "enabled": false },
                    { "id": "device_template_list",   "name": "device_template_list",   "label": "查询设备模板",     "description": "列出系统中所有设备模板",                            "danger": false, "enabled": true  },
                ]
            },
            {
                "id": "alarm",
                "label": "告警管理",
                "source": "core",
                "tools": [
                    { "id": "alarm_list",      "name": "alarm_list",      "label": "查询告警列表",  "description": "列出当前告警和历史告警记录",                  "danger": false, "enabled": true },
                    { "id": "alarm_get",       "name": "alarm_get",       "label": "获取告警详情",  "description": "获取指定告警的详细信息",                    "danger": false, "enabled": true },
                    { "id": "alarm_ack",       "name": "alarm_ack",       "label": "确认告警",      "description": "确认并关闭一条告警",                      "danger": false, "enabled": true },
                    { "id": "alarm_rule_list", "name": "alarm_rule_list", "label": "查询告警规则",  "description": "列出系统中所有告警规则",                  "danger": false, "enabled": true },
                    { "id": "alarm_stats",     "name": "alarm_stats",     "label": "告警统计",      "description": "获取告警统计摘要（总数、等级分布等）",      "danger": false, "enabled": true },
                ]
            },
            {
                "id": "workspace",
                "label": "工作空间",
                "source": "core",
                "tools": [
                    { "id": "workspace_list",    "name": "workspace_list",    "label": "查询工作空间", "description": "列出当前用户所属的所有工作空间",           "danger": false, "enabled": true },
                    { "id": "workspace_get",    "name": "workspace_get",    "label": "获取工作空间", "description": "获取指定工作空间的详细信息",           "danger": false, "enabled": true },
                    { "id": "workspace_create", "name": "workspace_create", "label": "创建工作空间", "description": "创建一个新的工作空间",                   "danger": false, "enabled": true },
                    { "id": "workspace_update", "name": "workspace_update", "label": "更新工作空间", "description": "更新工作空间的名称、描述等",           "danger": false, "enabled": true },
                    { "id": "workspace_delete", "name": "workspace_delete", "label": "删除工作空间", "description": "删除指定工作空间（不可恢复）",           "danger": true,  "enabled": false },
                    { "id": "agent_list",       "name": "agent_list",       "label": "查询 Agent",   "description": "列出当前工作空间中的所有 Agent 实例",   "danger": false, "enabled": true },
                ]
            },
            {
                "id": "monitoring",
                "label": "系统监控",
                "source": "core",
                "tools": [
                    { "id": "system_health", "name": "system_health", "label": "系统健康检查", "description": "查询系统各组件的运行状态和健康度",    "danger": false, "enabled": true },
                    { "id": "event_list",    "name": "event_list",    "label": "查询事件列表", "description": "列出系统事件日志，支持过滤和分页",  "danger": false, "enabled": true },
                ]
            },
            {
                "id": "driver",
                "label": "驱动管理",
                "source": "core",
                "tools": [
                    { "id": "driver_list", "name": "driver_list", "label": "查询驱动列表", "description": "列出系统中所有已注册的协议驱动（Modbus/ONVIF等）", "danger": false, "enabled": true },
                    { "id": "driver_get",  "name": "driver_get",  "label": "获取驱动详情", "description": "获取指定驱动的配置和状态信息",                     "danger": false, "enabled": true },
                ]
            },
            {
                "id": "job",
                "label": "任务管理",
                "source": "core",
                "tools": [
                    { "id": "job_list",   "name": "job_list",   "label": "查询任务列表", "description": "列出系统中所有调度任务",                    "danger": false, "enabled": true },
                    { "id": "job_get",    "name": "job_get",    "label": "获取任务详情", "description": "获取指定任务的执行状态和历史记录",          "danger": false, "enabled": true },
                    { "id": "job_cancel", "name": "job_cancel", "label": "取消任务",     "description": "取消一个正在等待或运行中的调度任务",        "danger": true,  "enabled": false },
                ]
            },
            {
                "id": "mcp",
                "label": "MCP 工具",
                "source": "plugin",
                "tools": [
                    { "id": "mcp_workspace_list", "name": "mcp_workspace_list", "label": "查询 MCP 工作空间", "description": "列出 AI Agent 可用的 MCP 工作空间资源", "danger": false, "enabled": true },
                    { "id": "mcp_workspace_read", "name": "mcp_workspace_read", "label": "读取 MCP 资源",      "description": "读取 MCP 工作空间中的文件或配置资源",  "danger": false, "enabled": true },
                ]
            },
        ]
    })
}

// ============================================================================
// Fallback Agent client — in-memory, used when no agent backend is available
// ============================================================================

/// Fallback agent client — in-memory implementation for when no agent backend is available
pub struct FallbackAgentClient {
    pub agents: std::sync::Mutex<std::collections::HashMap<String, AgentInfo>>,
    pub db_pool: sqlx::SqlitePool,
}

impl FallbackAgentClient {
    pub fn new(db_pool: sqlx::SqlitePool) -> Self {
        Self {
            agents: std::sync::Mutex::new(std::collections::HashMap::new()),
            db_pool,
        }
    }

    pub fn with_agents(db_pool: sqlx::SqlitePool, agents: Vec<AgentInfo>) -> Self {
        let map: std::collections::HashMap<String, AgentInfo> = agents
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        Self {
            agents: std::sync::Mutex::new(map),
            db_pool,
        }
    }
}

impl Default for FallbackAgentClient {
    fn default() -> Self {
        panic!("FallbackAgentClient::default() is not available — use FallbackAgentClient::new(pool) instead")
    }
}

impl AgentClient for FallbackAgentClient {
    fn create_agent(
        &self,
        config: &AgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, AgentError>> + Send + '_>> {
        let agent_id = format!("agent-{}", uuid::Uuid::new_v4());
        let agent = AgentInfo {
            id: agent_id.clone(),
            name: config.name.clone(),
            status: "active".to_string(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        self.agents.lock().unwrap().insert(agent_id.clone(), agent);

        Box::pin(async move { Ok(agent_id) })
    }

    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let result = self
            .agents
            .lock()
            .unwrap()
            .remove(agent_id)
            .ok_or(AgentError::NotFound(agent_id.to_string()));

        Box::pin(async move { result.map(|_| ()) })
    }

    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AgentInfo, AgentError>> + Send + '_>> {
        let result = self
            .agents
            .lock()
            .unwrap()
            .get(agent_id)
            .cloned()
            .ok_or(AgentError::NotFound(agent_id.to_string()));

        Box::pin(async move { result })
    }

    fn update_agent(
        &self,
        agent_id: &str,
        _config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let exists = self.agents.lock().unwrap().contains_key(agent_id);
        let result = if exists {
            Ok(())
        } else {
            Err(AgentError::NotFound(agent_id.to_string()))
        };

        Box::pin(async move { result })
    }

    fn chat_send(
        &self,
        _agent_id: &str,
        session_key: &str,
        _message: &str,
        run_id: &str,
        _system_prompt: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();
        let run_id = run_id.to_string();

        Box::pin(async move {
            let reply_text = "Agent 后端未连接。请配置 ZeroClaw Gateway 后重试。";
            let sse_final = serde_json::json!({
                "runId": run_id,
                "sessionKey": session_key,
                "state": "final",
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": reply_text}],
                }
            });
            let data_line = format!("data: {}\n", sse_final);

            let stream = futures_util::stream::once(async move {
                Ok::<_, std::io::Error>(bytes::Bytes::from(data_line))
            });
            let pinned: Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send>>
                = Box::pin(stream);

            let http_response = http::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::wrap_stream(pinned))
                .map_err(|e| AgentError::RequestFailed(format!("Failed to build response: {}", e)))?;

            Ok(reqwest::Response::from(http_response))
        })
    }

    fn chat_history(
        &self,
        _agent_id: &str,
        session_key: &str,
        limit: u32,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let session_key = session_key.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let rows = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
                "SELECT role, content, timestamp, run_id FROM chat_messages
                 WHERE session_key = ? ORDER BY timestamp ASC LIMIT ?"
            )
            .bind(&session_key)
            .bind(limit as i64)
            .fetch_all(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let messages: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(role, content, timestamp, run_id)| {
                    let content_parsed: serde_json::Value = serde_json::from_str(&content)
                        .unwrap_or(serde_json::json!([{"type": "text", "text": content}]));
                    serde_json::json!({
                        "role": role,
                        "content": content_parsed,
                        "timestamp": timestamp,
                        "toolCallId": run_id,
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
        _run_id: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Err(AgentError::Unavailable("fallback: chat_abort not implemented".into())) })
    }

    fn list_agents(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agents: Vec<serde_json::Value> = self
            .agents
            .lock()
            .unwrap()
            .values()
            .map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null))
            .collect();
        Box::pin(async move { Ok(serde_json::json!({ "agents": agents })) })
    }

    fn get_agent_config(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move {
            Ok(serde_json::json!({
                "config": default_agent_config(),
                "baseHash": null,
            }))
        })
    }

    fn set_agent_config(
        &self,
        _agent_id: &str,
        _config: &str,
        _base_hash: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn tools_catalog(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(build_tools_catalog_json()) })
    }

    fn tools_effective(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let overrides_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let overrides: serde_json::Value = overrides_row
                .map(|(json_str,)| serde_json::from_str(&json_str).unwrap_or_default())
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

            let catalog = build_tools_catalog_json();
            let groups = catalog
                .get("groups")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let filtered_groups: Vec<serde_json::Value> = groups
                .into_iter()
                .map(|group| {
                    let tools = group
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let filtered_tools: Vec<serde_json::Value> = tools
                        .into_iter()
                        .map(|mut tool| {
                            let tool_id = tool
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let is_dangerous = tool
                                .get("danger")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

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

    fn tools_toggle(
        &self,
        agent_id: &str,
        tool_name: &str,
        enabled: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let tool_name = tool_name.to_string();
        let db_pool = self.db_pool.clone();

        Box::pin(async move {
            let current_row = sqlx::query_as::<_, (String,)>(
                "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB query failed: {}", e)))?;

            let overrides: serde_json::Value = current_row
                .map(|(json_str,)| serde_json::from_str(&json_str).unwrap_or_default())
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

            let new_overrides = serde_json::json!({
                "enabled": enabled_list,
                "disabled": disabled_list,
            });

            sqlx::query(
                "INSERT INTO agent_tools (agent_id, tool_overrides, updated_at)
                 VALUES (?, ?, datetime('now'))
                 ON CONFLICT(agent_id) DO UPDATE SET
                   tool_overrides = excluded.tool_overrides,
                   updated_at = datetime('now')"
            )
            .bind(&agent_id)
            .bind(new_overrides.to_string())
            .execute(&db_pool)
            .await
            .map_err(|e| AgentError::RequestFailed(format!("DB upsert failed: {}", e)))?;

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig {
            workspace_id: "ws1".to_string(),
            name: "test".to_string(),
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            system_prompt: None,
        };
        assert_eq!(config.workspace_id, "ws1");
        assert_eq!(config.name, "test");
        assert!(config.model.is_none());
        assert!(config.temperature.is_none());
    }

    #[test]
    fn test_agent_info_creation() {
        let info = AgentInfo {
            id: "agent-1".to_string(),
            name: "Test Agent".to_string(),
            status: "active".to_string(),
            created_at: Some("2026-04-07T00:00:00Z".to_string()),
        };
        assert_eq!(info.id, "agent-1");
        assert_eq!(info.status, "active");
    }

    #[test]
    fn test_agent_error_display() {
        let err = AgentError::Unavailable("connection refused".to_string());
        assert!(err.to_string().contains("Agent unavailable"));
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_agent_error_not_found() {
        let err = AgentError::NotFound("missing-agent".to_string());
        assert!(err.to_string().contains("agent not found"));
        assert!(err.to_string().contains("missing-agent"));
    }

    #[tokio::test]
    async fn test_fallback_agent_client_chat_abort_returns_unavailable() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::with_agents(pool, vec![]);
        let result = client.chat_abort("agent1", "session1", None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::Unavailable(msg) => {
                assert!(msg.contains("fallback"));
                assert!(msg.contains("chat_abort"));
            }
            _ => panic!("expected Unavailable error"),
        }
    }

    #[tokio::test]
    async fn test_fallback_agent_client_list_agents() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::with_agents(
            pool,
            vec![AgentInfo {
                id: "a1".to_string(),
                name: "Agent One".to_string(),
                status: "active".to_string(),
                created_at: None,
            }],
        );
        let result = client.list_agents().await;
        let json = result.unwrap();
        let agents = json.as_object().unwrap().get("agents").unwrap().as_array().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].get("id").unwrap(), "a1");
    }

    #[test]
    fn test_zeroclaw_agent_client_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<ZeroClawAgentClient>();
    }

    // ========================================================================
    // Helper — create an in-memory SQLite pool with all required tables
    // ========================================================================

    async fn create_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create in-memory SQLite pool");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chat_sessions (
                session_key TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create chat_sessions table");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_key TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                run_id TEXT NOT NULL
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create chat_messages table");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_configs (
                agent_id TEXT PRIMARY KEY,
                config TEXT NOT NULL,
                config_hash TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create agent_configs table");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_tools (
                agent_id TEXT PRIMARY KEY,
                tool_overrides TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create agent_tools table");

        pool
    }

    // ========================================================================
    // AgentConfig — serialization roundtrip and defaults
    // ========================================================================

    #[test]
    fn test_agent_config_to_json_roundtrip() {
        let config = AgentConfig {
            workspace_id: "ws-test-001".to_string(),
            name: "TestAgent".to_string(),
            model: Some("claude-sonnet-4-5".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(1.0),
            system_prompt: Some("You are a helpful assistant.".to_string()),
        };

        let json_str = config.to_json().expect("should serialize");
        let parsed: AgentConfig = serde_json::from_str(&json_str).expect("should deserialize");

        assert_eq!(parsed.workspace_id, "ws-test-001");
        assert_eq!(parsed.name, "TestAgent");
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.max_tokens, Some(4096));
        assert_eq!(parsed.top_p, Some(1.0));
        assert_eq!(
            parsed.system_prompt.as_deref(),
            Some("You are a helpful assistant.")
        );
    }

    #[test]
    fn test_agent_config_to_json_partial_fields() {
        let config = AgentConfig {
            workspace_id: "ws-x".to_string(),
            name: "MinimalAgent".to_string(),
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            system_prompt: None,
        };

        let json_str = config.to_json().expect("should serialize");
        let parsed: AgentConfig = serde_json::from_str(&json_str).expect("should deserialize");

        assert_eq!(parsed.workspace_id, "ws-x");
        assert!(parsed.model.is_none());
        assert!(parsed.system_prompt.is_none());
    }

    #[test]
    fn test_agent_config_default_values() {
        let config = AgentConfig::default();
        assert_eq!(config.workspace_id, "");
        assert_eq!(config.name, "");
        assert!(config.model.is_none());
        assert!(config.temperature.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.top_p.is_none());
        assert!(config.system_prompt.is_none());
    }

    // ========================================================================
    // Hash computation — deterministic and correct
    // ========================================================================

    #[test]
    fn test_compute_hash_deterministic() {
        let input = r#"{"model":"claude-sonnet-4-5","temperature":0.7}"#;
        let hash1 = compute_hash(input);
        let hash2 = compute_hash(input);
        assert_eq!(hash1, hash2, "hash should be deterministic");
        assert_eq!(hash1.len(), 64, "SHA-256 produces 64 hex chars");
    }

    #[test]
    fn test_compute_hash_different_inputs_different_hashes() {
        let hash1 = compute_hash(r#"{"model":"claude-sonnet-4-5"}"#);
        let hash2 = compute_hash(r#"{"model":"claude-opus"}"#);
        assert_ne!(hash1, hash2, "different inputs should produce different hashes");
    }

    #[test]
    fn test_compute_hash_empty_string() {
        let hash = compute_hash("");
        assert_eq!(hash.len(), 64);
        // Known SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // ========================================================================
    // default_agent_config() — format verification
    // ========================================================================

    #[test]
    fn test_default_agent_config_format() {
        let config = default_agent_config();
        let obj = config.as_object().expect("should be an object");

        assert_eq!(
            obj.get("model").and_then(|v| v.as_str()),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(
            obj.get("temperature").and_then(|v| v.as_f64()),
            Some(0.7)
        );
        assert_eq!(
            obj.get("maxTokens").and_then(|v| v.as_i64()),
            Some(4096)
        );
        assert_eq!(obj.get("topP").and_then(|v| v.as_f64()), Some(1.0));
        assert_eq!(
            obj.get("systemPrompt").and_then(|v| v.as_str()),
            Some("")
        );
        assert_eq!(
            obj.get("workspace").and_then(|v| v.as_str()),
            Some("default")
        );
    }

    // ========================================================================
    // build_tools_catalog_json — structure verification
    // ========================================================================

    #[test]
    fn test_tools_catalog_structure() {
        let catalog = build_tools_catalog_json();
        let obj = catalog.as_object().expect("should be an object");

        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("catalog should have 'groups' array");

        assert!(
            !groups.is_empty(),
            "catalog should have at least one tool group"
        );

        // Verify at least the device and workspace groups exist
        let group_ids: Vec<&str> = groups
            .iter()
            .filter_map(|g| g.get("id").and_then(|v| v.as_str()))
            .collect();

        assert!(
            group_ids.contains(&"device"),
            "catalog should have a 'device' group"
        );
        assert!(
            group_ids.contains(&"workspace"),
            "catalog should have a 'workspace' group"
        );

        // Verify each group has required fields
        for group in groups {
            let g_obj = group.as_object().expect("group should be an object");
            assert!(
                g_obj.contains_key("id"),
                "group should have 'id' field"
            );
            assert!(
                g_obj.contains_key("label"),
                "group should have 'label' field"
            );
            assert!(
                g_obj.contains_key("tools"),
                "group should have 'tools' field"
            );

            let tools = g_obj
                .get("tools")
                .and_then(|v| v.as_array())
                .expect("tools should be an array");

            for tool in tools {
                let t_obj = tool.as_object().expect("tool should be an object");
                assert!(
                    t_obj.contains_key("id"),
                    "tool should have 'id' field"
                );
                assert!(
                    t_obj.contains_key("danger"),
                    "tool should have 'danger' field"
                );
                assert!(
                    t_obj.contains_key("enabled"),
                    "tool should have 'enabled' field"
                );
            }
        }
    }

    #[test]
    fn test_tools_catalog_dangerous_tools_are_disabled_by_default() {
        let catalog = build_tools_catalog_json();
        let groups = catalog
            .as_object()
            .and_then(|v| v.get("groups"))
            .and_then(|v| v.as_array())
            .expect("catalog should have groups");

        for group in groups {
            let tools = group
                .as_object()
                .and_then(|v| v.get("tools"))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            for tool in tools {
                let is_dangerous = tool
                    .as_object()
                    .and_then(|v| v.get("danger"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_enabled = tool
                    .as_object()
                    .and_then(|v| v.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_dangerous {
                    assert!(
                        !is_enabled,
                        "dangerous tool {:?} should be disabled by default",
                        tool
                    );
                }
            }
        }
    }

    // ========================================================================
    // FallbackAgentClient — chat_send returns synthetic SSE response
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_chat_send_returns_sse_response() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let response = client
            .chat_send("agent1", "session:test/123", "hello", "run-abc", "")
            .await
            .expect("should succeed (returns synthetic response, not an error)");

        assert_eq!(response.status(), 200);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/event-stream")
        );
    }

    // ========================================================================
    // FallbackAgentClient — get_agent_config returns default format
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_get_agent_config_returns_default_format() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client
            .get_agent_config("any-agent-id")
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        assert!(obj.contains_key("config"), "response should have 'config' key");
        assert!(
            obj.contains_key("baseHash"),
            "response should have 'baseHash' key"
        );
        assert!(
            obj.get("baseHash").unwrap().is_null(),
            "baseHash should be null for default"
        );
    }

    // ========================================================================
    // FallbackAgentClient — set_agent_config is a no-op that returns Ok
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_set_agent_config_returns_ok() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client
            .set_agent_config("agent1", r#"{"model":"claude-opus"}"#, None)
            .await;
        assert!(
            result.is_ok(),
            "set_agent_config should be a no-op and return Ok"
        );
    }

    // ========================================================================
    // FallbackAgentClient — tools_catalog returns full catalog
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_tools_catalog_returns_full_catalog() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client.tools_catalog("agent1").await.expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("should have 'groups' key");
        assert!(
            !groups.is_empty(),
            "fallback client should return full tools catalog"
        );
    }

    // ========================================================================
    // FallbackAgentClient — tools_effective returns filtered list
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_tools_effective_returns_filtered_list() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client
            .tools_effective("agent1")
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("should have 'groups' key");
        assert!(
            !groups.is_empty(),
            "tools_effective should return groups"
        );

        // Every tool should have an "enabled" field
        for group in groups {
            let tools = group
                .as_object()
                .and_then(|v| v.get("tools"))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            for tool in tools {
                let t_obj = tool.as_object().expect("tool should be an object");
                assert!(
                    t_obj.contains_key("enabled"),
                    "each tool should have 'enabled' field"
                );
            }
        }
    }

    // ========================================================================
    // FallbackAgentClient — tools_toggle is a no-op that returns Ok
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_tools_toggle_returns_ok() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client
            .tools_toggle("agent1", "workspace_read", true)
            .await;
        assert!(
            result.is_ok(),
            "tools_toggle should be a no-op and return Ok"
        );
    }

    // ========================================================================
    // FallbackAgentClient — delete_agent returns NotFound for unknown agent
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_delete_unknown_agent_returns_not_found() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client.delete_agent("nonexistent-agent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::NotFound(id) => assert_eq!(id, "nonexistent-agent"),
            other => panic!("expected NotFound error, got {:?}", other),
        }
    }

    // ========================================================================
    // FallbackAgentClient — get_agent returns NotFound for unknown agent
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_get_unknown_agent_returns_not_found() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client.get_agent("nonexistent-agent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::NotFound(id) => assert_eq!(id, "nonexistent-agent"),
            other => panic!("expected NotFound error, got {:?}", other),
        }
    }

    // ========================================================================
    // FallbackAgentClient — create_agent returns a unique id
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_create_agent_returns_unique_id() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let id1 = client
            .create_agent(&AgentConfig::default())
            .await
            .expect("should succeed");
        let id2 = client
            .create_agent(&AgentConfig::default())
            .await
            .expect("should succeed");

        assert_ne!(id1, id2, "each create_agent should return a unique id");
        assert!(
            id1.starts_with("agent-"),
            "agent id should start with 'agent-'"
        );
    }

    // ========================================================================
    // FallbackAgentClient — chat_history with empty session (uses SQLite)
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_chat_history_empty_session() {
        let pool = create_test_pool().await;
        let client = FallbackAgentClient::new(pool);

        let result = client
            .chat_history("default", "session:empty/test", 50)
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let messages = obj.get("messages").expect("should have 'messages' key");
        assert!(messages.is_array(), "'messages' should be an array");
        assert!(
            messages.as_array().unwrap().is_empty(),
            "empty session should have no messages"
        );
    }

    // ========================================================================
    // FallbackAgentClient — chat_history with one message
    // ========================================================================

    #[tokio::test]
    async fn test_fallback_agent_client_chat_history_with_message() {
        let pool = create_test_pool().await;

        let session_key = "session:user/test-history";
        let msg_content = r#"[{"type":"text","text":"Hello, world!"}]"#;

        sqlx::query(
            "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
             VALUES (?, 'default', datetime('now'), datetime('now'))
             ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')"
        )
        .bind(session_key)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
             VALUES (?, 'user', ?, ?, ?)"
        )
        .bind(session_key)
        .bind(msg_content)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind("run-001")
        .execute(&pool)
        .await
        .unwrap();

        let client = FallbackAgentClient::new(pool);
        let result = client
            .chat_history("default", session_key, 50)
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let messages = obj.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 1, "should have exactly one message");
        assert_eq!(
            messages[0].get("role").and_then(|v| v.as_str()),
            Some("user")
        );
        assert!(
            messages[0].get("content").is_some(),
            "message should have content"
        );
    }

    // ========================================================================
    // AgentError variants — coverage for all enum arms
    // ========================================================================

    #[test]
    fn test_agent_error_request_failed() {
        let err = AgentError::RequestFailed("connection reset".to_string());
        assert!(err.to_string().contains("Agent API request failed"));
        assert!(err.to_string().contains("connection reset"));
    }

    #[test]
    fn test_agent_error_api_error() {
        let err = AgentError::ApiError("invalid json".to_string());
        assert!(err.to_string().contains("Agent API returned error"));
        assert!(err.to_string().contains("invalid json"));
    }

    #[test]
    fn test_agent_error_timeout() {
        let err = AgentError::Timeout;
        assert!(err.to_string().contains("Agent API timeout"));
    }

    // ========================================================================
    // ZeroClawAgentClient — chat_history with empty session
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_chat_history_empty_session() {
        let pool = create_test_pool().await;
        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client
            .chat_history("default", "session:empty/test", 50)
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let messages = obj.get("messages").expect("should have 'messages' key");
        assert!(messages.is_array(), "'messages' should be an array");
        assert!(
            messages.as_array().unwrap().is_empty(),
            "empty session should have no messages"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — chat_history with one message
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_chat_history_with_message() {
        let pool = create_test_pool().await;

        let session_key = "session:user/test-history";
        let msg_content = r#"[{"type":"text","text":"Hello, world!"}]"#;

        sqlx::query(
            "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
             VALUES (?, 'default', datetime('now'), datetime('now'))
             ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')"
        )
        .bind(session_key)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
             VALUES (?, 'user', ?, ?, ?)"
        )
        .bind(session_key)
        .bind(msg_content)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind("run-001")
        .execute(&pool)
        .await
        .unwrap();

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client
            .chat_history("default", session_key, 50)
            .await
            .expect("should succeed");

        let obj = result.as_object().expect("should be an object");
        let messages = obj.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 1, "should have exactly one message");
        assert_eq!(
            messages[0].get("role").and_then(|v| v.as_str()),
            Some("user")
        );
        assert!(
            messages[0].get("content").is_some(),
            "message should have content"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — chat_history respects limit parameter
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_chat_history_respects_limit() {
        let pool = create_test_pool().await;
        let session_key = "session:limit/test";

        sqlx::query(
            "INSERT INTO chat_sessions (session_key, agent_id, created_at, updated_at)
             VALUES (?, 'default', datetime('now'), datetime('now'))
             ON CONFLICT(session_key) DO UPDATE SET updated_at = datetime('now')"
        )
        .bind(session_key)
        .execute(&pool)
        .await
        .unwrap();

        // Insert 5 messages
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO chat_messages (session_key, role, content, timestamp, run_id)
                 VALUES (?, 'user', ?, ?, ?)"
            )
            .bind(session_key)
            .bind(format!(r#"[{{"type":"text","text":"msg {}"}}]"#, i))
            .bind((i + 1) as i64 * 1000)
            .bind(format!("run-{:03}", i))
            .execute(&pool)
            .await
            .unwrap();
        }

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        // Request only 3 messages
        let result = client
            .chat_history("default", session_key, 3)
            .await
            .expect("should succeed");

        let messages = result
            .as_object()
            .unwrap()
            .get("messages")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(messages.len(), 3, "should return at most 3 messages");
    }

    // ========================================================================
    // ZeroClawAgentClient — get_agent_config returns default when not found
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_get_agent_config_returns_default_when_not_found() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client
            .get_agent_config("nonexistent-agent")
            .await
            .expect("should succeed (returns default config)");

        let obj = result.as_object().expect("should be an object");
        assert!(obj.contains_key("config"), "response should have 'config' key");
        assert!(
            obj.contains_key("baseHash"),
            "response should have 'baseHash' key"
        );
        assert!(
            obj.get("baseHash").unwrap().is_null(),
            "baseHash should be null"
        );
        let config = obj.get("config").unwrap();
        assert_eq!(
            config.get("model").and_then(|v| v.as_str()),
            Some("claude-sonnet-4-5")
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — set + get agent_config roundtrip
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_set_and_get_agent_config_roundtrip() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let new_config = r#"{"model":"claude-opus-3","temperature":0.5,"maxTokens":8192,"topP":0.9,"systemPrompt":"Custom prompt","workspace":"ws-test"}"#;

        client
            .set_agent_config("agent-001", new_config, None)
            .await
            .expect("set_agent_config should succeed");

        let result = client
            .get_agent_config("agent-001")
            .await
            .expect("get_agent_config should succeed");

        let obj = result.as_object().expect("should be an object");
        let config = obj.get("config").expect("should have config");
        assert_eq!(
            config.get("model").and_then(|v| v.as_str()),
            Some("claude-opus-3")
        );
        assert_eq!(
            config.get("temperature").and_then(|v| v.as_f64()),
            Some(0.5)
        );
        assert!(
            obj.get("baseHash").unwrap().is_string(),
            "baseHash should be populated after set"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — set_agent_config rejects invalid JSON
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_set_agent_config_rejects_invalid_json() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client
            .set_agent_config("agent-001", "not valid json {{{", None)
            .await;
        assert!(result.is_err(), "invalid JSON should return an error");
        match result.unwrap_err() {
            AgentError::RequestFailed(msg) => {
                assert!(
                    msg.contains("Invalid config JSON") || msg.contains("json"),
                    "should be a JSON parse error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected RequestFailed error for invalid JSON, got {:?}",
                other
            ),
        }
    }

    // ========================================================================
    // ZeroClawAgentClient — get_agent returns static info
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_get_agent_returns_static_info() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let info = client
            .get_agent("any-agent-id")
            .await
            .expect("should succeed");
        assert_eq!(info.name, "ZeroClaw Agent");
        assert_eq!(info.status, "active");
    }

    // ========================================================================
    // ZeroClawAgentClient — list_agents returns single default agent
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_list_agents_returns_default() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client.list_agents().await.expect("should succeed");

        let agents = result
            .as_object()
            .unwrap()
            .get("agents")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(
            agents[0].get("id").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            agents[0].get("status").and_then(|v| v.as_str()),
            Some("active")
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — create_agent returns "default"
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_create_agent_returns_default() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let id = client
            .create_agent(&AgentConfig::default())
            .await
            .expect("should succeed");
        assert_eq!(
            id, "default",
            "ZeroClaw is single-agent, always returns 'default'"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — chat_abort is a no-op placeholder
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_chat_abort_returns_ok() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        // chat_abort is currently a no-op; returns Ok(()) as placeholder
        let result = client
            .chat_abort("default", "session:test", Some("run-abc"))
            .await;
        assert!(
            result.is_ok(),
            "chat_abort should be a no-op and return Ok"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — tools_toggle persists override to SQLite
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_tools_toggle_persists_override() {
        let pool = create_test_pool().await;

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool.clone(),
        );

        // Toggle device_delete from disabled -> enabled
        client
            .tools_toggle("agent-001", "device_delete", true)
            .await
            .expect("tools_toggle should succeed");

        // Verify the override was persisted
        let row: (String,) = sqlx::query_as(
            "SELECT tool_overrides FROM agent_tools WHERE agent_id = ?"
        )
        .bind("agent-001")
        .fetch_one(&pool)
        .await
        .expect("tool_overrides should be persisted");

        let overrides: serde_json::Value =
            serde_json::from_str(&row.0).expect("should parse as JSON");
        let enabled_list: Vec<String> = overrides
            .get("enabled")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        assert!(
            enabled_list.contains(&"device_delete".to_string()),
            "device_delete should be in enabled list after toggle"
        );
    }

    // ========================================================================
    // ZeroClawAgentClient — tools_effective respects per-agent overrides
    // ========================================================================

    #[tokio::test]
    async fn test_zeroclaw_tools_effective_respects_overrides() {
        let pool = create_test_pool().await;

        // Pre-populate agent_tools with device_delete enabled
        sqlx::query(
            "INSERT INTO agent_tools (agent_id, tool_overrides, updated_at)
             VALUES (?, ?, datetime('now'))"
        )
        .bind("agent-override-test")
        .bind(r#"{"enabled":["device_delete"],"disabled":[]}"#)
        .execute(&pool)
        .await
        .unwrap();

        let client = ZeroClawAgentClient::new(
            "http://localhost:8080".to_string(),
            "ws://localhost:8080".to_string(),
            None,
            pool,
        );

        let result = client
            .tools_effective("agent-override-test")
            .await
            .expect("tools_effective should succeed");

        let obj = result.as_object().expect("should be an object");
        let groups = obj
            .get("groups")
            .and_then(|v| v.as_array())
            .expect("should have groups");

        // Find the device_delete tool
        let mut found_delete = false;
        for group in groups {
            let tools = group
                .as_object()
                .and_then(|v| v.get("tools"))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            for tool in tools {
                let id = tool
                    .as_object()
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if id == "device_delete" {
                    found_delete = true;
                    let enabled = tool
                        .as_object()
                        .and_then(|v| v.get("enabled"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    assert!(
                        enabled,
                        "device_delete should be enabled due to override"
                    );
                }
            }
        }
        assert!(found_delete, "device_delete tool should exist in catalog");
    }
}

/// Layer 1: Platform base prompt — fixed TinyIoTHub identity, protocols, and operations
pub fn platform_base_prompt() -> String {
    r#"你是 TinyIoTHub 智能网关的 AI 助手。

## 平台身份
你运行在 TinyIoTHub 边缘网关上，管理物联网设备。

## 支持的设备类型
- Modbus (工业寄存器读写)
- ONVIF (网络摄像头)
- SNMP (网络设备监控)
- MQTT (消息订阅发布)

## 统一操作规范
- 读取：使用 read_register / get_device_status / subscribe_topic
- 控制：使用 write_register / publish_command
- 告警：使用 create_alarm_rule / get_alarm_events

## UI 组件渲染规范
当需要向用户展示结构化数据时，使用 canvas 工具推送 A2UI 组件：

**canvas 工具参数：**
- action: "a2ui_push"
- jsonl: A2UI JSONL 格式的组件描述

**A2UI 消息格式（JSONL）：**
```
{"createSurface":{"id":"<唯一ID>","surfaceKind":"inline"}}
{"updateComponents":{"components":[{"id":"comp1","componentKind":"DeviceCard","dataModel":{"deviceId":"001","name":"温度传感器","status":"online"}}]}}
{"updateDataModel":{"componentId":"comp1","dataModel":{"temperature":25.5}}}
```

**可用组件类型：**
- Text: 文本显示
- Button: 按钮
- Card: 卡片容器
- Row/Column: 布局
- DeviceCard: 设备卡片
- DeviceTable: 设备表格
- DataChart: 数据图表

**示例：展示设备列表**
```json
{"createSurface":{"id":"device-list","surfaceKind":"inline"}}
{"updateComponents":{"components":[{"id":"table1","componentKind":"DeviceTable","dataModel":{"columns":["名称","状态","温度"],"rows":[["传感器1","在线","25°C"],["传感器2","离线","--"]]}]}}
```
"#.to_string()
}

/// Build the full system prompt by combining Layer 1 (platform base) + Layer 2 (user persona) + Layer 3 (skills)
pub fn build_full_system_prompt(
    user_persona: &str,
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let base = platform_base_prompt();

    // Layer 2: user persona
    let layer2 = if user_persona.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n## Agent 灵魂设定（用户配置）\n{}\n", user_persona)
    };

    // Layer 3: skills loaded from filesystem
    let layer3 = load_skills_prompt(workspace_id, agent_id);

    format!("{}{}{}", base, layer2, layer3)
}

/// Load skill files from the skills/ directory and format as Layer 3 prompt
/// Priority: skills/<ws>/<ag>/prompts/ > skills/<ws>/prompts/ > skills/<ws>/ > skills/tinyiothub/prompts/
fn load_skills_prompt(workspace_id: Option<&str>, agent_id: Option<&str>) -> String {
    use crate::domain::agent::skill::AgentSkill;

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let ws = workspace_id.unwrap_or("tinyiothub");

    // Try in order: prompts subdir first, then flat directory
    let candidates: Vec<std::path::PathBuf> = match (workspace_id, agent_id) {
        (Some(w), Some(a)) => vec![
            base.join(w).join(a).join("prompts"),
            base.join(w).join("prompts"),
            base.join(w).join(a),
            base.join(w),
        ],
        (Some(_w), None) => vec![
            base.join(ws).join("prompts"),
            base.join(ws),
        ],
        _ => vec![
            base.join("tinyiothub").join("prompts"),
            base.join("tinyiothub"),
        ],
    };

    for dir in candidates {
        if dir.exists() {
            let result = read_skill_dir(&dir);
            if !result.is_empty() {
                return result;
            }
        }
    }

    String::new()
}

fn read_skill_dir(dir: &std::path::Path) -> String {
    use crate::domain::agent::skill::AgentSkill;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
            return String::new();
        }
    };

    let mut skill_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();

    skill_files.sort_by_key(|e| e.file_name().to_string_lossy().to_string());

    let mut all_skills = String::new();

    for entry in skill_files {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy().into_owned();
        let skill_name = file_name.trim_end_matches(".md");

        let (fm, body) = AgentSkill::parse_frontmatter(&content);
        let body = body.trim();

        if body.is_empty() {
            continue;
        }

        let description = fm
            .as_ref()
            .and_then(|f| f.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or(skill_name);

        let version = fm
            .as_ref()
            .and_then(|f| f.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        all_skills.push_str(&format!(
            "### {}{}\n{}\n{}\n",
            skill_name,
            if version.is_empty() {
                String::new()
            } else {
                format!(" (v{})", version)
            },
            description,
            body
        ));
    }

    if all_skills.is_empty() {
        String::new()
    } else {
        format!("\n\n## 技能（Skills）\n你可以使用以下技能来完成任务：\n\n{}\n", all_skills)
    }
}
