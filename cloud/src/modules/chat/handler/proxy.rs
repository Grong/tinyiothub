use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    response::{sse::Event as SseEvent, IntoResponse, Response, Sse},
    Json,
};
use async_stream::stream;

use crate::{
    modules::agent::ChatRequest,
    modules::mcp::handlers::{McpAuthContext, McpContextGuard},
    shared::api_response::ApiResponse,
    shared::{app_state::AppState},
};

use super::types::*;
use crate::modules::agent::handler::types::{AgentConfigUpdateRequest, ToolToggleRequest};

/// POST /api/v1/chat/stream — SSE streaming chat
pub async fn chat_stream(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ChatStreamRequest>,
) -> Response {
    // 后端自行读取 agent 配置获取 system_prompt
    let agent_config = state.agent_runtime.get_agent_config(&req.agent_id).await
        .map(|v| v.get("config").cloned().unwrap_or_default())
        .unwrap_or_default();
    let user_persona = agent_config.get("systemPrompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // session_key format: agent:<agentId>:<mainKey>/<sess_uuid>
    // Extract workspace_id from the second colon-separated segment
    let workspace_id = req.session_key.split(':').nth(1).and_then(|s| s.split('/').next());
    let system_prompts = &crate::shared::config::get().agent.system_prompts;
    let full_prompt = crate::shared::agent::build_full_system_prompt(
        system_prompts,
        user_persona,
        workspace_id,
        None,
    ).await;

    // Set MCP context for in-process tool calls (JWT-authenticated)
    let mcp_ctx = McpAuthContext::for_jwt(
        workspace_id.map(|s| s.to_string()).unwrap_or_default(),
        claims.user_id.clone(),
    );
    let _guard = McpContextGuard::new(mcp_ctx);

    let chat_request = ChatRequest {
        session_key: req.session_key,
        message: req.message,
        run_id: req.run_id,
        system_prompt_override: req.system_prompt.or(Some(full_prompt)),
    };

    let mut chat_stream = match state.chat_service.chat(chat_request).await {
        Ok(stream) => stream,
        Err(e) => {
            let err: Json<ApiResponse<()>> = ApiResponseBuilder::error(format!("Chat stream failed: {}", e));
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
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(200);
    match state.chat_service.get_history(&query.session_key, limit).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to load chat history: {}", e)),
    }
}

/// POST /api/v1/chat/abort
pub async fn chat_abort(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ChatAbortRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
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
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    match state
        .session_service
        .list_sessions(
            query.workspace_id.as_deref(),
            query.agent_id.as_deref(),
            limit,
            offset,
        )
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
    _claims: Claims,
    Json(req): Json<UpdateSessionLabelRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.session_service.update_label(&session_key, &req.label).await {
        Ok(session) => ApiResponseBuilder::success(serde_json::json!({ "session": session })),
        Err(e) => ApiResponseBuilder::error(format!("Failed to update session label: {}", e)),
    }
}

/// DELETE /api/v1/chat/sessions/{session_key}
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.session_service.delete_session(&session_key).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({ "deleted": true })),
        Err(e) => ApiResponseBuilder::error(format!("Failed to delete session: {}", e)),
    }
}

/// GET /api/v1/agents
pub async fn list_agents(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_runtime.list_agents().await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list agents: {}", e)),
    }
}

/// GET /api/v1/agents/{id}/config
pub async fn get_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.agent_runtime.get_agent_config(&agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get agent config: {}", e)),
    }
}

/// PUT /api/v1/agents/{id}/config
pub async fn set_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
    Json(req): Json<AgentConfigUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    let base_hash_ref = req.base_hash.as_deref();
    match state
        .agent_runtime
        .set_agent_config(&agent_id, &config_str, base_hash_ref)
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
    _claims: Claims,
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
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    match state.agent_runtime.tools_effective(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get effective tools: {}", e)),
    }
}

/// POST /api/v1/tools/toggle
pub async fn tools_toggle(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ToolToggleRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state
        .agent_runtime
        .tools_toggle(&req.agent_id, &req.tool_name, req.enabled)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"toggled": true})),
        Err(e) => ApiResponseBuilder::error(format!("Failed to toggle tool: {}", e)),
    }
}
