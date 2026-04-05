// MCP HTTP Handlers
// HTTP endpoint handlers for MCP protocol (tools/list + tools/call)

use axum::{
    extract::Extension,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::{
    dto::response::builder::ApiResponseBuilder,
    shared::{app_state::AppState, security::jwt::Claims},
};

use super::tool_registry::{HandlerRegistry, ToolError, ToolMetadata};

/// Thread-local storage for MCP request context (tenant_id from JWT)
thread_local! {
    static MCP_CONTEXT: std::cell::RefCell<Option<Claims>> = const { std::cell::RefCell::new(None) };
}

/// Set MCP context (tenant_id) for the current async task
fn set_mcp_context(claims: Claims) {
    MCP_CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(claims));
}

/// Get MCP context (tenant_id) for the current async task
pub fn get_mcp_context() -> Option<Claims> {
    MCP_CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// RAII guard for MCP context - clears on drop
struct McpContextGuard;

impl McpContextGuard {
    fn new(claims: Claims) -> Self {
        set_mcp_context(claims);
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

/// Extract JWT claims from request headers (reuses context.rs logic)
fn extract_jwt_claims(
    headers: &axum::http::HeaderMap,
) -> Result<crate::shared::security::jwt::Claims, ToolError> {
    let auth_header = headers.typed_get::<Authorization<Bearer>>().ok_or_else(|| {
        ToolError::Unauthorized("Missing Authorization header".to_string())
    })?;

    crate::shared::security::jwt::validate_jwt(auth_header.token())
        .map_err(|e| ToolError::Unauthorized(e.to_string()))
}

/// Handle all MCP requests
async fn handle_mcp_request(
    headers: axum::http::HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    // Validate JWT and set tenant context with RAII guard
    let claims = match extract_jwt_claims(&headers) {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    // Extract needed fields before passing ownership to guard
    let user_id = claims.user_id.clone();
    let tenant_id = claims.tenant_id.clone();
    let _guard = McpContextGuard::new(claims);

    // Get registry from global state
    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500,
                "MCP registry not initialized",
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
            let tool = registry.get(&params.name);
            // Clone args for logging before passing ownership to handler
            let args_for_log = params.arguments.clone();
            let sanitized_args = serde_json::to_string(&args_for_log)
                .unwrap_or_else(|_| "<invalid JSON>".to_string());
            let start = Instant::now();

            match tool {
                Some(handler) => match handler.execute(params.arguments).await {
                    Ok(result) => {
                        let latency_ms = start.elapsed().as_millis() as u64;
                        tracing::info!(
                            tool = %params.name,
                            user_id = %user_id,
                            tenant_id = %tenant_id,
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
                            user_id = %user_id,
                            tenant_id = %tenant_id,
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
                        user_id = %user_id,
                        tenant_id = %tenant_id,
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
    // Validate JWT and set tenant context with RAII guard
    let claims = match extract_jwt_claims(&headers) {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    // Extract needed fields before passing ownership to guard
    let _user_id = claims.user_id.clone();
    let _tenant_id = claims.tenant_id.clone();
    let _guard = McpContextGuard::new(claims);

    // Get registry from global state
    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500,
                "MCP registry not initialized",
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
    // Validate JWT and set tenant context with RAII guard
    let claims = match extract_jwt_claims(&headers) {
        Ok(c) => c,
        Err(e) => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(401, e.to_string())
                .into_response();
        }
    };
    // Extract needed fields before passing ownership to guard
    let user_id = claims.user_id.clone();
    let tenant_id = claims.tenant_id.clone();
    let _guard = McpContextGuard::new(claims);

    // Get registry from global state
    let registry = match super::get_mcp_registry() {
        Some(reg) => reg,
        None => {
            return ApiResponseBuilder::error_with_code::<serde_json::Value>(
                500,
                "MCP registry not initialized",
            )
            .into_response();
        }
    };

    let registry = registry.read().await;

    let tool = registry.get(&params.name);
    // Clone args for logging before passing ownership to handler
    let args_for_log = params.arguments.clone();
    let sanitized_args = serde_json::to_string(&args_for_log)
        .unwrap_or_else(|_| "<invalid JSON>".to_string());
    let start = Instant::now();

    match tool {
        Some(handler) => match handler.execute(params.arguments).await {
            Ok(result) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                tracing::info!(
                    tool = %params.name,
                    user_id = %user_id,
                    tenant_id = %tenant_id,
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
                    user_id = %user_id,
                    tenant_id = %tenant_id,
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
                user_id = %user_id,
                tenant_id = %tenant_id,
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
            crate::api::mcp::tool_registry::InputSchema::object(vec![], std::collections::HashMap::new())
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
}
