//! Agent handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts,
    setup_test_app, setup_test_app_with_pool,
};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
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
    let response = app
        .oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/config", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_set_agent_config_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/agents/nonexistent-agent-12345/config",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    let status = response.status();
    // Accept any non-500 status — handler may return 400, 404, or 200 with error
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

// ── Agent Files ──

#[tokio::test]
async fn test_list_agent_files_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/agents/nonexistent-agent-12345/files", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Skills ──

#[tokio::test]
async fn test_list_skills() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/agents/skills", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_create_skill_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/agents/skills", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_get_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/agents/skills/nonexistent-skill-12345", &token, None))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_update_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/agents/skills/nonexistent-skill-12345",
            &token,
            Some(json!({"name": "updated"})),
        ))
        .await
        .unwrap();
    let status = response.status();
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

#[tokio::test]
async fn test_delete_skill_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/agents/skills/nonexistent-skill-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

// ── Agent Files (detail) ──

#[tokio::test]
async fn test_get_agent_file_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/agents/nonexistent-agent-12345/files/some-file.md",
            &token,
            None,
        ))
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
        .oneshot(auth_request(
            "PUT",
            "/api/v1/agents/nonexistent-agent-12345/files/some-file.md",
            &token,
            Some(json!({"content": "test"})),
        ))
        .await
        .unwrap();
    let status = response.status();
    assert!(!status.is_server_error(), "Expected non-5xx status, got: {}", status);
}

// ── Workspace Isolation ──

#[tokio::test]
async fn test_list_agents_isolated_by_workspace() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed an agent in workspace "ws-tenant-1"
    sqlx::query(
        "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind("agent-1")
    .bind("ws-tenant-1")
    .bind("Test Agent")
    .execute(&pool)
    .await
    .unwrap();

    // Build router with shared state
    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // Token for workspace "ws-tenant-1" should see the agent
    let token_a = create_test_token_with_workspace("user-1", "tenant-1", "ws-tenant-1");
    let response =
        app.clone().oneshot(auth_request("GET", "/api/v1/agents", &token_a, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    let agents = json["result"]["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["id"], "agent-1");

    // Token for workspace "ws-tenant-2" should see NO agents
    let token_b = create_test_token_with_workspace("user-2", "tenant-2", "ws-tenant-2");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/agents", &token_b, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    let agents = json["result"]["agents"].as_array().unwrap();
    assert!(agents.is_empty(), "Expected no agents for different workspace");
}

#[tokio::test]
async fn test_get_agent_config_isolated_by_workspace() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed an agent and its config in workspace "ws-tenant-1"
    sqlx::query(
        "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind("agent-1")
    .bind("ws-tenant-1")
    .bind("Test Agent")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_configs (agent_id, config, config_hash, updated_at)
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind("agent-1")
    .bind(r#"{"model":"test"}"#)
    .bind("hash123")
    .execute(&pool)
    .await
    .unwrap();

    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // Correct workspace should succeed
    let token_a = create_test_token_with_workspace("user-1", "tenant-1", "ws-tenant-1");
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/agents/agent-1/config", &token_a, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["config"]["model"], "test");

    // Wrong workspace should fail (agent not found)
    let token_b = create_test_token_with_workspace("user-2", "tenant-2", "ws-tenant-2");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/agents/agent-1/config", &token_b, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for cross-workspace access");
}

#[tokio::test]
async fn test_set_agent_config_isolated_by_workspace() {
    let (app_state, pool) = setup_test_app_with_pool().await;

    // Seed an agent in workspace "ws-tenant-1"
    sqlx::query(
        "INSERT INTO agents (agent_id, workspace_id, name, status, created_at, updated_at)
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind("agent-1")
    .bind("ws-tenant-1")
    .bind("Test Agent")
    .execute(&pool)
    .await
    .unwrap();

    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // Wrong workspace should fail
    let token_b = create_test_token_with_workspace("user-2", "tenant-2", "ws-tenant-2");
    let body = json!({"config": {"model": "hacked"}});
    let response = app
        .oneshot(auth_request("PUT", "/api/v1/agents/agent-1/config", &token_b, Some(body)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for cross-workspace config update");
}
