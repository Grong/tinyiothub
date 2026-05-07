use std::collections::HashMap;

use async_stream::stream;
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response, Sse, sse::Event as SseEvent},
};
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::*;
use crate::{
    modules::agent::{
        ChatRequest,
        handler::types::{AgentConfigUpdateRequest, ToolToggleRequest},
    },
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

/// POST /api/v1/chat/stream — SSE streaming chat
pub async fn chat_stream(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ChatStreamRequest>,
) -> Response {
    // 后端自行读取 agent 配置获取 system_prompt
    let agent_config = state
        .agent_runtime
        .get_agent_config(&req.agent_id, &claims.workspace_id)
        .await
        .map(|v| v.get("config").cloned().unwrap_or_default())
        .unwrap_or_default();
    let user_persona = agent_config.get("systemPrompt").and_then(|v| v.as_str()).unwrap_or("");

    // session_key format: agent:<workspace_id>:<agent_id>/<sess_uuid>
    // Use claims.workspace_id (from JWT) as the source of truth.
    // If claims has a workspace_id, replace the session_key's workspace segment
    // to ensure the correct workspace is used for MCP tool calls.
    let session_key = if !claims.workspace_id.is_empty() {
        // Parse agent_id and uuid from the incoming session_key
        let parts: Vec<&str> = req.session_key.split(':').collect();
        if parts.len() >= 3 {
            let agent_and_sess = parts[2]; // "agent_id/uuid"
            format!("agent:{}:{}", claims.workspace_id, agent_and_sess)
        } else {
            req.session_key.clone()
        }
    } else {
        // Old token without workspace_id — fallback to session_key parsing
        req.session_key.clone()
    };

    let workspace_id = session_key
        .split(':')
        .nth(1)
        .and_then(|s| s.split('/').next())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let system_prompts = &crate::shared::config::get().agent.system_prompts;
    let full_prompt = crate::shared::agent::build_full_system_prompt(
        system_prompts,
        user_persona,
        Some(&workspace_id),
        None,
    )
    .await;

    let chat_request = ChatRequest {
        session_key,
        message: req.message,
        run_id: req.run_id,
        system_prompt_override: req.system_prompt.or(Some(full_prompt)),
    };

    let mut chat_stream = match state.chat_service.chat(chat_request).await {
        Ok(stream) => stream,
        Err(e) => {
            let err: Json<ApiResponse<()>> =
                ApiResponseBuilder::error(format!("Chat stream failed: {}", e));
            return err.into_response();
        }
    };

    let event_stream = stream! {
        use futures::StreamExt;
        while let Some(event) = chat_stream.next().await {
            let payload = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, std::io::Error>(SseEvent::default().data(payload));
        }
    };

    Sse::new(event_stream).into_response()
}

/// GET /api/v1/chat/history
pub async fn chat_history(
    State(state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(200);

    // Verify workspace isolation: session_key must belong to the authenticated workspace
    let session_workspace = extract_workspace_from_session_key(&query.session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    match state.chat_service.get_history(&query.session_key, limit).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to load chat history: {}", e)),
    }
}

/// POST /api/v1/chat/abort
pub async fn chat_abort(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ChatAbortRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Verify workspace isolation
    let session_workspace = extract_workspace_from_session_key(&req.session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    let run_id_ref = req.run_id.as_deref();
    match state.chat_service.abort_chat(&req.session_key, run_id_ref).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"aborted": true})),
        Err(e) => ApiResponseBuilder::error(format!("Abort failed: {}", e)),
    }
}

/// GET /api/v1/chat/sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ChatSessionsQuery>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    let workspace_id = if claims.workspace_id.is_empty() {
        query.workspace_id.as_deref()
    } else {
        Some(claims.workspace_id.as_str())
    };
    match state
        .session_service
        .list_sessions(workspace_id, query.agent_id.as_deref(), limit, offset)
        .await
    {
        Ok(sessions) => ApiResponseBuilder::success(serde_json::json!({ "sessions": sessions })),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list sessions: {}", e)),
    }
}

/// POST /api/v1/chat/sessions/{session_key}/label
pub async fn update_session_label(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    claims: Claims,
    Json(req): Json<UpdateSessionLabelRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Verify workspace isolation
    let session_workspace = extract_workspace_from_session_key(&session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    match state.session_service.update_label(&session_key, &req.label).await {
        Ok(session) => ApiResponseBuilder::success(serde_json::json!({ "session": session })),
        Err(e) => ApiResponseBuilder::error(format!("Failed to update session label: {}", e)),
    }
}

/// DELETE /api/v1/chat/sessions/{session_key}
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    // Verify workspace isolation
    let session_workspace = extract_workspace_from_session_key(&session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    match state.session_service.delete_session(&session_key).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({ "deleted": true })),
        Err(e) => ApiResponseBuilder::error(format!("Failed to delete session: {}", e)),
    }
}

/// GET /api/v1/agents
pub async fn list_agents(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_runtime.list_agents(&claims.workspace_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list agents: {}", e)),
    }
}

/// GET /api/v1/agents/{id}/config
pub async fn get_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_runtime.get_agent_config(&agent_id, &claims.workspace_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get agent config: {}", e)),
    }
}

/// PUT /api/v1/agents/{id}/config
pub async fn set_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    claims: Claims,
    Json(req): Json<AgentConfigUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    let base_hash_ref = req.base_hash.as_deref();
    match state
        .agent_runtime
        .set_agent_config(&agent_id, &config_str, base_hash_ref, &claims.workspace_id)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"saved": true})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to save config: {}", e)),
    }
}

/// GET /api/v1/tools/catalog
pub async fn tools_catalog(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    match state.agent_runtime.tools_catalog(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get tools catalog: {}", e)),
    }
}

/// GET /api/v1/tools/effective
pub async fn tools_effective(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    match state.agent_runtime.tools_effective(agent_id, &claims.workspace_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get effective tools: {}", e)),
    }
}

/// POST /api/v1/tools/toggle
pub async fn tools_toggle(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ToolToggleRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state
        .agent_runtime
        .tools_toggle(&req.agent_id, &req.tool_name, req.enabled, &claims.workspace_id)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"toggled": true})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to toggle tool: {}", e)),
    }
}

/// Extract workspace_id from session key in format: agent:{workspace_id}:{agent_id}/{session_uuid}
fn extract_workspace_from_session_key(session_key: &str) -> String {
    session_key
        .split(':')
        .nth(1)
        .and_then(|s| s.split('/').next())
        .map(|s| s.to_string())
        .unwrap_or_default()
}
