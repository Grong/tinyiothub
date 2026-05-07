//! MCP handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/mcp/tools/list", &token, Some(json!({}))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_mcp_tools_call_missing_params() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/mcp/tools/call", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_mcp_request() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/mcp",
            &token,
            Some(json!({"jsonrpc": "2.0", "method": "initialize", "id": 1})),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}
