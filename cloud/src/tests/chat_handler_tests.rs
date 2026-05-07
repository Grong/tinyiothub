//! Chat handler integration tests

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
async fn test_list_chat_sessions() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/chat/sessions", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_chat_history() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/chat/history", &token, None)).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_delete_chat_session_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/chat/sessions/nonexistent-session-key",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_update_session_label_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/chat/sessions/nonexistent-session-key/label",
            &token,
            Some(json!({"label": "test"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Tools endpoints (standalone under /v1/tools) ──

#[tokio::test]
async fn test_tools_catalog() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/tools/catalog", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_tools_effective() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/tools/effective", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_tools_toggle() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/tools/toggle", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ── Abort ──

#[tokio::test]
async fn test_abort_chat_session_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/chat/abort",
            &token,
            Some(json!({"session_key": "nonexistent"})),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}
