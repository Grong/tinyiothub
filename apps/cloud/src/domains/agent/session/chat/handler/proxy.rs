use crate::domains::agent::AgentState;
use std::collections::HashMap;

use async_stream::stream;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response, Sse, sse::Event as SseEvent},
};
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use super::types::*;
use crate::domains::agent::session::handlers::types::{AgentConfigUpdateRequest, ToolToggleRequest};
use tinyiothub_agent::session::SessionKey;
use tinyiothub_web::api_response::ApiResponse;

/// POST /api/v1/chat/stream — SSE streaming chat
pub async fn chat_stream(
    State(state): State<AgentState>,
    claims: Claims,
    Json(req): Json<ChatStreamRequest>,
) -> Response {
    // Same fail-closed guard as chat_history/chat_abort: an unscoped token
    // must not be able to stream into another workspace's session by forging
    // the session_key.
    let session_workspace = extract_workspace_from_session_key(&req.session_key);
    if session_workspace != claims.workspace_id {
        let err: Json<ApiResponse<()>> = ApiResponseBuilder::error_with_code(404, "Session not found");
        return err.into_response();
    }

    // session_key format: agent:<workspace_id>:<agent_id>/<sess_uuid>
    let session_key = scope_session_key_to_claims(&claims.workspace_id, &req.session_key);

    let workspace_id = session_key
        .split(':')
        .nth(1)
        .and_then(|s| s.split('/').next())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let system_prompts = &state.system_prompts;
    let memory_source: std::sync::Arc<dyn crate::domains::agent::prompt::PromptMemorySource> = std::sync::Arc::new(
        crate::domains::agent::session::persist::memory::PromptMemoryStoreAdapter(state.memory_store.clone()),
    );
    let prompt = match crate::domains::agent::prompt::assistant::build_assistant_prompt(
        system_prompts,
        &workspace_id,
        &req.agent_id,
        req.message.clone(),
        Some(&memory_source),
    )
    .await
    {
        Ok(prompt) => prompt,
        Err(error) => {
            let err: Json<ApiResponse<()>> =
                ApiResponseBuilder::error(format!("Failed to build assistant prompt: {error}"));
            return err.into_response();
        }
    };

    // Server-minted run_id: the client learns it from the first SSE event
    // (every ChatEvent carries runId) and uses it for /chat/abort.
    let run_id = uuid::Uuid::new_v4().to_string();
    let agent_id = req.agent_id.clone();

    // Task 7: cloud fetches the agent config and supplies the storage handles;
    // the pool itself is storage-free.
    let db_pool = state.db_pool();
    let config = match crate::domains::agent::session::config::service::get_config(&db_pool, &agent_id).await {
        Ok(c) => c,
        Err(e) => {
            let err: Json<ApiResponse<()>> = ApiResponseBuilder::error(format!("Failed to load agent config: {}", e));
            return err.into_response();
        }
    };

    let mut rx = match crate::domains::agent::session::chat::service::send_with_pool(
        &state.agent_pool,
        &db_pool,
        &agent_id,
        &session_key,
        prompt,
        &run_id,
        &claims.workspace_id,
        &config,
        state.memory_service.clone(),
        state.session_runtime.clone(),
    )
    .await
    {
        Ok(rx) => rx,
        Err(e) => {
            let err: Json<ApiResponse<()>> = ApiResponseBuilder::error(format!("Chat stream failed: {}", e));
            return err.into_response();
        }
    };

    let event_stream = stream! {
        while let Some(event) = rx.recv().await {
            let payload = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, std::io::Error>(SseEvent::default().data(payload));
        }
    };

    let mut response = Sse::new(event_stream).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    response
}

/// GET /api/v1/chat/history
pub async fn chat_history(
    State(state): State<AgentState>,
    Query(query): Query<ChatHistoryQuery>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let limit = query.limit.unwrap_or(200);

    let session_workspace = extract_workspace_from_session_key(&query.session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    // Format validation first (session_history_json re-parses internally).
    if let Err(e) = SessionKey::parse(&query.session_key) {
        return ApiResponseBuilder::error(format!("Invalid session key: {}", e));
    }

    match crate::domains::agent::session::chat::history::session_history_json(
        &state.db_pool(),
        &query.session_key,
        limit,
        &claims.workspace_id,
    )
    .await
    {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to load chat history: {}", e)),
    }
}

/// POST /api/v1/chat/abort
pub async fn chat_abort(
    State(state): State<AgentState>,
    claims: Claims,
    Json(req): Json<ChatAbortRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let session_workspace = extract_workspace_from_session_key(&req.session_key);
    if session_workspace != claims.workspace_id {
        return ApiResponseBuilder::error_with_code(404, "Session not found");
    }

    let parsed = match SessionKey::parse(&req.session_key) {
        Ok(k) => k,
        Err(e) => return ApiResponseBuilder::error(format!("Invalid session key: {}", e)),
    };

    let run_id_ref = req.run_id.as_deref();
    match state
        .agent_pool
        .chat_abort(&parsed.agent_id, &req.session_key, run_id_ref, &claims.workspace_id)
        .await
    {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"aborted": true})),
        Err(e) => ApiResponseBuilder::error(format!("Abort failed: {}", e)),
    }
}

/// GET /api/v1/chat/sessions
pub async fn list_sessions(
    State(state): State<AgentState>,
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
    State(state): State<AgentState>,
    Path(session_key): Path<String>,
    claims: Claims,
    Json(req): Json<UpdateSessionLabelRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
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
    State(state): State<AgentState>,
    Path(session_key): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
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
pub async fn list_agents(State(state): State<AgentState>, claims: Claims) -> Json<ApiResponse<serde_json::Value>> {
    match crate::domains::agent::session::config::service::list_agents(&state.db_pool(), &claims.workspace_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to list agents: {}", e)),
    }
}

/// GET /api/v1/agents/{id}/config
pub async fn get_agent_config(
    State(state): State<AgentState>,
    Path(agent_id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    match crate::domains::agent::session::config::service::get_agent_config_json(
        &state.db_pool(),
        &agent_id,
        &claims.workspace_id,
    )
    .await
    {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get agent config: {}", e)),
    }
}

/// PUT /api/v1/agents/{id}/config
pub async fn set_agent_config(
    State(state): State<AgentState>,
    Path(agent_id): Path<String>,
    claims: Claims,
    Json(req): Json<AgentConfigUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    // Silently ignore base_hash mismatch — last write wins
    let _ = req.base_hash.as_deref();
    match crate::domains::agent::session::config::service::set_agent_config(
        &state.db_pool(),
        &agent_id,
        &config_str,
        &claims.workspace_id,
    )
    .await
    {
        Ok(()) => {
            // Config changed: drop the cached agent so it rebuilds lazily.
            state.agent_pool.invalidate(&agent_id);
            ApiResponseBuilder::success(serde_json::json!({"saved": true}))
        }
        Err(e) => ApiResponseBuilder::error(format!("Failed to save config: {}", e)),
    }
}

/// GET /api/v1/tools/catalog
pub async fn tools_catalog(
    State(state): State<AgentState>,
    Query(params): Query<HashMap<String, String>>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let _ = params;
    // Catalog snapshot from the pool's tool registry, generated from the same
    // ToolDescriptor declarations as LLM exposure and policy.
    let runtime = state.agent_pool.runtime_context().await;
    let data = state
        .agent_pool
        .tool_registry()
        .build_catalog(&claims.workspace_id, &runtime)
        .await;
    ApiResponseBuilder::success(data)
}

/// GET /api/v1/tools/effective
pub async fn tools_effective(
    State(state): State<AgentState>,
    Query(params): Query<HashMap<String, String>>,
    claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    let runtime = state.agent_pool.runtime_context().await;
    match crate::domains::agent::tool::service::effective_tool_names(
        &state.db_pool(),
        &state.agent_pool.tool_registry(),
        &runtime,
        agent_id,
        &claims.workspace_id,
    )
    .await
    {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(format!("Failed to get effective tools: {}", e)),
    }
}

/// POST /api/v1/tools/toggle
pub async fn tools_toggle(
    State(state): State<AgentState>,
    claims: Claims,
    Json(req): Json<ToolToggleRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match crate::domains::agent::session::config::service::toggle_tool(
        &state.db_pool(),
        &req.agent_id,
        &req.tool_name,
        req.enabled,
        &claims.workspace_id,
    )
    .await
    {
        Ok(()) => {
            // Denylist changed: drop the cached agent so it rebuilds lazily.
            state.agent_pool.invalidate(&req.agent_id);
            ApiResponseBuilder::success(serde_json::json!({"toggled": true}))
        }
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

/// Rewrite the session key's workspace segment to the claim's workspace, so a
/// client holding a scoped token cannot address another tenant's session by
/// forging the key. An empty claim workspace (unscoped/admin token) leaves the
/// key untouched.
fn scope_session_key_to_claims(claims_workspace: &str, req_key: &str) -> String {
    if claims_workspace.is_empty() {
        return req_key.to_string();
    }
    let parts: Vec<&str> = req_key.split(':').collect();
    if parts.len() >= 3 {
        format!("agent:{}:{}", claims_workspace, parts[2])
    } else {
        req_key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_workspace_from_standard_key() {
        assert_eq!(
            extract_workspace_from_session_key("agent:ws-1:agent_main/sess_9"),
            "ws-1"
        );
    }

    #[test]
    fn extract_workspace_from_malformed_key_is_empty() {
        assert_eq!(extract_workspace_from_session_key("no-colons"), "");
        assert_eq!(extract_workspace_from_session_key(""), "");
    }

    #[test]
    fn scope_rewrites_workspace_for_scoped_claims() {
        // Even if the client sends another tenant's workspace, the claim wins.
        assert_eq!(
            scope_session_key_to_claims("ws_mine", "agent:ws_other:agent_main/s1"),
            "agent:ws_mine:agent_main/s1"
        );
    }

    #[test]
    fn scope_passthrough_for_unscoped_claims() {
        let key = "agent:ws_other:agent_main/s1";
        assert_eq!(scope_session_key_to_claims("", key), key);
    }

    #[test]
    fn scope_passthrough_for_malformed_key() {
        assert_eq!(scope_session_key_to_claims("ws_mine", "garbage"), "garbage");
    }
}
