use serde::Deserialize;

/// Request body for POST /api/v1/chat/stream
#[derive(Debug, Deserialize)]
pub struct ChatStreamRequest {
    pub agent_id: String,
    pub session_key: String,
    pub message: String,
    pub run_id: String,
    /// Full system prompt (Layer1 platform base + Layer2 user persona) to inject into ZeroClaw
    #[serde(default)]
    pub system_prompt: Option<String>,
}

/// Query parameters for GET /api/v1/chat/history
#[derive(Debug, Deserialize)]
pub struct ChatHistoryQuery {
    pub agent_id: String,
    pub session_key: String,
    pub limit: Option<u32>,
}

/// Request body for POST /api/v1/chat/abort
#[derive(Debug, Deserialize)]
pub struct ChatAbortRequest {
    pub agent_id: String,
    pub session_key: String,
    pub run_id: Option<String>,
}

/// Request body for PUT /api/v1/agents/:id/config
#[derive(Debug, Deserialize)]
pub struct AgentConfigUpdateRequest {
    pub config: serde_json::Value,
    pub base_hash: Option<String>,
}

/// Request body for POST /api/v1/tools/toggle
#[derive(Debug, Deserialize)]
pub struct ToolToggleRequest {
    pub agent_id: String,
    pub tool_name: String,
    pub enabled: bool,
}
