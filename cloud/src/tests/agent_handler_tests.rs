//! Agent handler integration tests

use axum::{body::Body, http::{Request, StatusCode}};
use serde_json::{json, Value};
use tower::ServiceExt;
use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method).uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

// ── Agents ──

#[tokio::test]
async fn test_list_agents() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_agent_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/config", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_set_agent_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/agents/nonexistent-agent-12345/config", &token, Some(json!({})))).await.unwrap();
    let status = response.status();
    // Accept any non-500 status — handler may return 400, 404, or 200 with error
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

// ── Agent Files ──

#[tokio::test]
async fn test_list_agent_files_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/files", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Agent Heartbeat ──

#[tokio::test]
async fn test_get_agent_heartbeat_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/heartbeat/config", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_agent_heartbeat_logs_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/heartbeat/logs", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_agent_heartbeat_tasks_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/heartbeat/tasks", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Skills ──

#[tokio::test]
async fn test_list_skills() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/skills", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_create_skill_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("POST", "/api/v1/agents/skills", &token, Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/agents/skills/nonexistent-skill-12345", &token, None)).await.unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_update_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/agents/skills/nonexistent-skill-12345", &token, Some(json!({"name": "updated"})))).await.unwrap();
    let status = response.status();
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

#[tokio::test]
async fn test_delete_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("DELETE", "/api/v1/agents/skills/nonexistent-skill-12345", &token, None)).await.unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

// ── Agent Files (detail) ──

#[tokio::test]
async fn test_get_agent_file_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/files/some-file.md", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_put_agent_file_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("PUT", "/api/v1/agents/nonexistent-agent-12345/files/some-file.md", &token, Some(json!({"content": "test"}))))
        .await
        .unwrap();
    let status = response.status();
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

// ── Agent Heartbeat — write endpoints ──

#[tokio::test]
async fn test_update_heartbeat_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let body = json!({"enabled": true, "interval_minutes": 5});
    let response = app
        .oneshot(auth_request("PUT", "/api/v1/agents/nonexistent-agent-12345/heartbeat/config", &token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_update_heartbeat_tasks_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let body = json!({"tasks": [{"priority": "high", "text": "Check system health", "paused": false}]});
    let response = app
        .oneshot(auth_request("PUT", "/api/v1/agents/nonexistent-agent-12345/heartbeat/tasks", &token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["code"].is_number(), "Expected numeric code");
}
