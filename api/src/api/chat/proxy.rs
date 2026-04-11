use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    response::{sse::Event as SseEvent, IntoResponse, Response, Sse},
    Json,
};
use async_stream::stream;
use futures::StreamExt;

use crate::{
    dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder},
    shared::{app_state::AppState, security::jwt::Claims},
};

use super::types::*;

/// POST /api/v1/chat/stream — SSE streaming chat
pub async fn chat_stream(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ChatStreamRequest>,
) -> Response {
    // 后端自行读取 agent 配置获取 system_prompt
    let agent_config = state.agent_client.get_agent_config(&req.agent_id).await
        .map(|v| v.get("config").cloned().unwrap_or_default())
        .unwrap_or_default();
    let user_persona = agent_config.get("systemPrompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // session_key format: agent:<agentId>:<mainKey>/<sess_uuid>
    // Extract workspace_id from the second colon-separated segment
    let workspace_id = req.session_key.split(':').nth(1).map(|s| s.split('/').next()).flatten();
    let full_prompt = crate::infrastructure::zeroclaw_agent::build_full_system_prompt(
        user_persona,
        workspace_id,
        None,
    );
    // 只传递原始用户消息，不混入系统提示词（系统提示词由 Agent 内部处理）
    let original_message = req.message.clone();

    let response = match state
        .agent_client
        .chat_send(&req.agent_id, &req.session_key, &original_message, &req.run_id, &full_prompt)
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            let err: Json<ApiResponse<()>> = ApiResponseBuilder::error(&format!("Chat stream failed: {}", e));
            return err.into_response();
        }
    };

    // Forward the SSE stream from the agent to the client
    let byte_stream = response.bytes_stream();
    let event_stream = stream! {
        let mut stream = byte_stream;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                        for line in text.lines() {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            // ZeroClaw already sends SSE-formatted lines ("data: {...}")
                            // Extract the payload to avoid double "data:" prefix
                            let payload = if let Some(rest) = trimmed.strip_prefix("data: ") {
                                rest.to_string()
                            } else {
                                trimmed.to_string()
                            };
                            yield Ok::<_, std::io::Error>(SseEvent::default().data(payload));
                        }
                    }
                }
                Err(e) => {
                    yield Ok(SseEvent::default().data(format!("error: {}", e)));
                    break;
                }
            }
        }
    };

    Sse::new(event_stream).into_response()
}

/// GET /api/v1/chat/history
pub async fn chat_history(
    State(state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(200);
    match state
        .agent_client
        .chat_history(&query.agent_id, &query.session_key, limit)
        .await
    {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to load chat history: {}", e)),
    }
}

/// POST /api/v1/chat/abort
pub async fn chat_abort(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ChatAbortRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let run_id_ref = req.run_id.as_deref();
    match state
        .agent_client
        .chat_abort(&req.agent_id, &req.session_key, run_id_ref)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"aborted": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Abort failed: {}", e)),
    }
}

/// GET /api/v1/agents
pub async fn list_agents(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_client.list_agents().await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to list agents: {}", e)),
    }
}

/// GET /api/v1/agents/:id/config
pub async fn get_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_client.get_agent_config(&agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get agent config: {}", e)),
    }
}

/// PUT /api/v1/agents/:id/config
pub async fn set_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
    Json(req): Json<AgentConfigUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    let base_hash_ref = req.base_hash.as_deref();
    match state
        .agent_client
        .set_agent_config(&agent_id, &config_str, base_hash_ref)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"saved": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to save config: {}", e)),
    }
}

/// GET /api/v1/tools/catalog
pub async fn tools_catalog(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    match state.agent_client.tools_catalog(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get tools catalog: {}", e)),
    }
}

/// GET /api/v1/tools/effective
pub async fn tools_effective(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    match state.agent_client.tools_effective(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get effective tools: {}", e)),
    }
}

/// POST /api/v1/tools/toggle
pub async fn tools_toggle(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ToolToggleRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state
        .agent_client
        .tools_toggle(&req.agent_id, &req.tool_name, req.enabled)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"toggled": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to toggle tool: {}", e)),
    }
}
