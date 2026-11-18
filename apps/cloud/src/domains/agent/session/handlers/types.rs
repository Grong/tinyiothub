use serde::Deserialize;

/// Request body for PUT /api/v1/agents/{id}/config
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
