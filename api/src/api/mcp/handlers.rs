// MCP HTTP Handlers
// HTTP endpoint handlers for MCP protocol (tools/list + tools/call)

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use sha2::Digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;

use crate::{
    dto::response::builder::ApiResponseBuilder,
    shared::app_state::AppState,
};

use super::tool_registry::{ToolError, ToolMetadata};

/// MCP auth context: workspace isolation for API Key authentication.
/// Unlike JWT-based auth (which had user_id/tenant_id), API Keys are bound
/// to a workspace and have no user identity.
#[derive(Debug, Clone)]
pub struct McpAuthContext {
    pub workspace_id: String,
    /// The API key ID used for this request (for audit logging)
    pub api_key_id: String,
    /// The API key name (for audit logging)
    pub api_key_name: String,
}

impl McpAuthContext {
    /// Returns "api_key" as the actor identifier, since API Keys
    /// have no user identity. Used for alarm acknowledgements and similar.
    pub fn actor_identifier(&self) -> &'static str {
        "api_key"
    }
}

// Thread-local storage for MCP request context (workspace_id from API Key)
thread_local! {
    static MCP_CONTEXT: std::cell::RefCell<Option<McpAuthContext>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Set MCP context for the current async task
fn set_mcp_context(ctx: McpAuthContext) {
    MCP_CONTEXT.with(|c| *c.borrow_mut() = Some(ctx));
}

/// Get MCP context for the current async task
pub fn get_mcp_context() -> Option<McpAuthContext> {
    MCP_CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// RAII guard for MCP context - clears on drop
struct McpContextGuard;

impl McpContextGuard {
    fn new(ctx: McpAuthContext) -> Self {
        set_mcp_context(ctx);
        McpContextGuard
    }
}

impl Drop for McpContextGuard {
    fn drop(&mut self) {
        MCP_CONTEXT.with(|ctx| *ctx.borrow_mut() = None);
    }
}

/// MCP JSON-RPC request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(flatten)]
    pub method: JsonRpcMethod,
}

/// JSON-RPC method call
#[derive(Debug, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum JsonRpcMethod {
    /// Initialize the MCP session
    #[serde(rename = "initialize")]
    Initialize,
    /// List available tools
    #[serde(rename = "tools/list")]
    ToolsList,
    /// Call a specific tool
    #[serde(rename = "tools/call")]
    ToolsCall(ToolCallParams),
}

/// Parameters for tools/call
#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// MCP JSON-RPC response result
#[derive(Debug, Serialize)]
pub struct JsonRpcResult {
    pub tools: Vec<ToolMetadata>,
}

/// Create the MCP router with global registry
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", post(handle_mcp_request))
        .route("/tools/list", post(handle_tools_list))
        .route("/tools/call", post(handle_tools_call))
}

/// Extract and validate API Key from X-API-Key header.
/// Returns McpAuthContext on success, ToolError on failure.
async fn extract_api_key(
    headers: &axum::http::HeaderMap,
    db: &crate::infrastructure::persistence::database::Database,
) -> Result<McpAuthContext, ToolError> {
    let raw_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ToolError::Unauthorized("Missing X-API-Key header".into()))?;

    // Hash the incoming key for secure lookup
    let key_hash = format!("{:x}", sha2::Sha256::digest(raw_key.as_bytes()));

    // Look up by hash (secure, no prefix collision possible)
    let api_key = crate::dto::entity::tenant::ApiKey::find_by_hash(db, &key_hash)
        .await
        .map_err(|e| ToolError::Internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ToolError::Unauthorized("Invalid API key".into()))?;

    // Verify key is enabled
    if !api_key.is_enabled {
        return Err(ToolError::Unauthorized("API key is disabled".into()));
    }

    // Verify key is not revoked
    if api_key.is_revoked {
        return Err(ToolError::Unauthorized("API key has been revoked".into()));
    }

    // Verify key has not expired
    if let Some(expires_at) = &api_key.expires_at {
        if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if expires < chrono::Utc::now() {
                return Err(ToolError::Unauthorized("API key has expired".into()));
            }
        }
    }

    Ok(McpAuthContext {
        workspace_id: api_key.workspace_id.clone(),
        api_key_id: api_key.id.clone(),
        api_key_name: api_key.name.clone(),
    })
}

/// Shared helper: extract API key and set MCP context with RAII guard.
/// All three handlers use this, eliminating the previous code duplication.
#[allow(dead_code)]
async fn with_mcp_context<F, R>(
    headers: axum::http::HeaderMap,
    db: &crate::infrastructure::persistence::database::Database,
    f: F,
) -> R
where
    F: FnOnce(McpAuthContext) -> R,
{
    let ctx = extract_api_key(&headers, db)
        .await
        .expect("Caller handles errors");
    let _guard = McpContextGuard::new(ctx.clone());
    f(ctx)
}

/// Handle all MCP requests
async fn handle_mcp_request(
    headers: axum::http::HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    let state = match super::get_app_state() {
        Some(s) => s,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let db = state.database();
    let ctx = match extract_api_key(&headers, db).await {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    let _guard = McpContextGuard::new(ctx.clone());

    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let registry = registry.read().await;

    match request.method {
        JsonRpcMethod::Initialize => {
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "tinyiothub",
                        "version": "1.0.0"
                    },
                    "capabilities": {
                        "tools": {}
                    }
                }
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        JsonRpcMethod::ToolsList => {
            let tools = registry.list_tools();
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": {
                    "tools": tools
                }
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        JsonRpcMethod::ToolsCall(params) => {
            let args_for_log = params.arguments.clone();
            let sanitized_args = serde_json::to_string(&args_for_log)
                .unwrap_or_else(|_| "<invalid JSON>".to_string());
            let start = Instant::now();

            match registry.get(&params.name) {
                Some(handler) => match handler.execute(params.arguments).await {
                    Ok(result) => {
                        let latency_ms = start.elapsed().as_millis() as u64;
                        tracing::info!(
                            tool = %params.name,
                            workspace_id = %ctx.workspace_id,
                            api_key_id = %ctx.api_key_id,
                            api_key_name = %ctx.api_key_name,
                            args = %sanitized_args,
                            latency_ms = %latency_ms,
                            success = true,
                            "MCP tool invocation succeeded"
                        );
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request.id,
                            "result": result
                        });
                        (StatusCode::OK, Json(response)).into_response()
                    }
                    Err(e) => {
                        let latency_ms = start.elapsed().as_millis() as u64;
                        tracing::error!(
                            tool = %params.name,
                            workspace_id = %ctx.workspace_id,
                            api_key_id = %ctx.api_key_id,
                            api_key_id = %ctx.api_key_id,
                            args = %sanitized_args,
                            latency_ms = %latency_ms,
                            success = false,
                            error = %e.to_string(),
                            "MCP tool invocation failed"
                        );
                        let code = match &e {
                            ToolError::InvalidParams(_) => 400,
                            ToolError::NotImplemented(_) => 501,
                            ToolError::Unauthorized(_) => 401,
                            ToolError::Forbidden(_) => 403,
                            ToolError::NotFound(_) => 404,
                            ToolError::RateLimited(_) => 429,
                            ToolError::ApiError(_, _) => 500,
                            ToolError::Internal(_) => 500,
                        };
                        ApiResponseBuilder::error_with_code::<serde_json::Value>(code, e.to_string())
                            .into_response()
                    }
                },
                None => {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    tracing::warn!(
                        tool = %params.name,
                        workspace_id = %ctx.workspace_id,
                        api_key_id = %ctx.api_key_id,
                        args = %sanitized_args,
                        latency_ms = %latency_ms,
                        success = false,
                        error = "Tool not found",
                        "MCP tool not found"
                    );
                    ApiResponseBuilder::error_with_code::<serde_json::Value>(
                        404,
                        format!("Tool not found: {}", params.name),
                    )
                    .into_response()
                }
            }
        }
    }
}

/// Handle tools/list endpoint
async fn handle_tools_list(headers: axum::http::HeaderMap) -> Response {
    let state = match super::get_app_state() {
        Some(s) => s,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let db = state.database();
    let ctx = match extract_api_key(&headers, db).await {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    let _guard = McpContextGuard::new(ctx);

    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let registry = registry.read().await;
    let tools = registry.list_tools();
    ApiResponseBuilder::success(serde_json::json!({ "tools": tools })).into_response()
}

/// Handle tools/call endpoint
async fn handle_tools_call(
    headers: axum::http::HeaderMap,
    Json(params): Json<ToolCallParams>,
) -> Response {
    let state = match super::get_app_state() {
        Some(s) => s,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let db = state.database();
    let ctx = match extract_api_key(&headers, db).await {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    let _guard = McpContextGuard::new(ctx.clone());

    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500, "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let registry = registry.read().await;

    let args_for_log = params.arguments.clone();
    let sanitized_args = serde_json::to_string(&args_for_log)
        .unwrap_or_else(|_| "<invalid JSON>".to_string());
    let start = Instant::now();

    match registry.get(&params.name) {
        Some(handler) => match handler.execute(params.arguments).await {
            Ok(result) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                tracing::info!(
                    tool = %params.name,
                    workspace_id = %ctx.workspace_id,
                    api_key_id = %ctx.api_key_id,
                    args = %sanitized_args,
                    latency_ms = %latency_ms,
                    success = true,
                    "MCP tool invocation succeeded"
                );
                ApiResponseBuilder::success(result).into_response()
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                tracing::error!(
                    tool = %params.name,
                    workspace_id = %ctx.workspace_id,
                    api_key_id = %ctx.api_key_id,
                    args = %sanitized_args,
                    latency_ms = %latency_ms,
                    success = false,
                    error = %e.to_string(),
                    "MCP tool invocation failed"
                );
                let code = match &e {
                    ToolError::InvalidParams(_) => 400,
                    ToolError::NotImplemented(_) => 501,
                    ToolError::Unauthorized(_) => 401,
                    ToolError::Forbidden(_) => 403,
                    ToolError::NotFound(_) => 404,
                    ToolError::RateLimited(_) => 429,
                    ToolError::ApiError(code, _) => *code,
                    ToolError::Internal(_) => 500,
                };
                ApiResponseBuilder::error_with_code::<serde_json::Value>(code, e.to_string())
                    .into_response()
            }
        },
        None => {
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::warn!(
                tool = %params.name,
                workspace_id = %ctx.workspace_id,
                api_key_id = %ctx.api_key_id,
                args = %sanitized_args,
                latency_ms = %latency_ms,
                success = false,
                error = "Tool not found",
                "MCP tool not found"
            );
            ApiResponseBuilder::error_with_code::<serde_json::Value>(
                404,
                format!("Tool not found: {}", params.name),
            )
            .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mcp::tool_registry::ToolHandler;
    use async_trait::async_trait;

    struct DummyHandler;

    #[async_trait]
    impl ToolHandler for DummyHandler {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn input_schema(&self) -> crate::api::mcp::tool_registry::InputSchema {
            crate::api::mcp::tool_registry::InputSchema::object(
                vec![],
                std::collections::HashMap::new(),
            )
        }

        async fn execute(&self, args: Value) -> Result<Value, ToolError> {
            Ok(args)
        }
    }

    #[test]
    fn test_json_rpc_request_deserialize() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(request.method, JsonRpcMethod::ToolsList));
    }

    #[test]
    fn test_json_rpc_call_params_deserialize() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"test_tool","arguments":{"foo":"bar"}}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(request.method, JsonRpcMethod::ToolsCall(_)));
    }

    #[test]
    fn test_mcp_auth_context_actor_identifier() {
        let ctx = McpAuthContext {
            workspace_id: "ws-001".to_string(),
            api_key_id: "key-001".to_string(),
            api_key_name: "TestKey".to_string(),
        };
        assert_eq!(ctx.actor_identifier(), "api_key");
    }
}
