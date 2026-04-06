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
fn default_agent_config() -> serde_json::Value {
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
fn compute_hash(s: &str) -> String {
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
        Box::pin(async move { Ok(serde_json::json!([])) })
    }

    fn tools_effective(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(serde_json::json!([])) })
    }

    fn tools_toggle(
        &self,
        _agent_id: &str,
        _tool_name: &str,
        _enabled: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }
}

// ============================================================================
// Fallback Agent client — in-memory, used when no agent backend is available
// ============================================================================

/// Fallback agent client — in-memory implementation for when no agent backend is available
pub struct FallbackAgentClient {
    pub agents: std::sync::Mutex<std::collections::HashMap<String, AgentInfo>>,
}

impl FallbackAgentClient {
    pub fn new() -> Self {
        Self {
            agents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn with_agents(agents: Vec<AgentInfo>) -> Self {
        let map: std::collections::HashMap<String, AgentInfo> = agents
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        Self {
            agents: std::sync::Mutex::new(map),
        }
    }
}

impl Default for FallbackAgentClient {
    fn default() -> Self {
        Self::new()
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
        _session_key: &str,
        _limit: u32,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move { Err(AgentError::Unavailable("fallback: chat_history not implemented".into())) })
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
        Box::pin(async move { Ok(serde_json::json!([])) })
    }

    fn tools_effective(
        &self,
        _agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(serde_json::json!([])) })
    }

    fn tools_toggle(
        &self,
        _agent_id: &str,
        _tool_name: &str,
        _enabled: bool,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }
}
