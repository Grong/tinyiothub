//! Jobs handler integration tests
//!
//! Tests the jobs API which is a compatibility layer over the cron system.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace,
    setup_test_app, setup_test_app_with_pool,
};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };
    builder.body(Body::from(body_str)).unwrap()
}

// Use a token with a tenant that has a workspace, so resolve_workspace succeeds.
fn test_token() -> String {
    create_test_token_with_workspace("user-1", "tenant-default-001", "ws-default-001")
}

// ============================================================================
// List Jobs
// ============================================================================

#[tokio::test]
async fn test_list_jobs() {
    let app = setup_test_app().await;
    let token = test_token();

    let response = app
        .oneshot(auth_request("GET", "/api/v1/jobs?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of jobs");
}

// ============================================================================
// Create Job
// ============================================================================

#[tokio::test]
async fn test_create_job() {
    let app = setup_test_app().await;
    let token = test_token();

    let body = json!({
        "name": "test-job-001",
        "job_type": "shell",
        "cron_expression": "0 0 * * * *",
        "config": "{}",
        "timeout_seconds": 60,
        "retry_count": 3
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/jobs", &token, Some(body))).await.unwrap();

    let status = response.status();
    assert!(
        !status.is_informational() && status != StatusCode::SWITCHING_PROTOCOLS,
        "Unexpected status: {}",
        status
    );
}

#[tokio::test]
async fn test_create_job_invalid_cron() {
    let app = setup_test_app().await;
    let token = test_token();

    let body = json!({
        "name": "bad-cron-job",
        "job_type": "shell",
        "cron_expression": "invalid cron expression",
        "config": "{}"
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/jobs", &token, Some(body))).await.unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected validation error for invalid cron");
}

#[tokio::test]
async fn test_create_job_with_5_field_cron() {
    let app = setup_test_app().await;
    let token = test_token();

    // 5-field cron should be normalized to 6-field (prepend seconds=0)
    let body = json!({
        "name": "five-field-cron",
        "job_type": "shell",
        "cron_expression": "0 0 * * *",
        "config": "{}"
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/jobs", &token, Some(body))).await.unwrap();

    let status = response.status();
    assert!(
        !status.is_informational() && status != StatusCode::SWITCHING_PROTOCOLS,
        "5-field cron should be accepted: {}",
        status
    );
}

// ============================================================================
// Get Job — not found
// ============================================================================

#[tokio::test]
async fn test_get_job_not_found() {
    let app = setup_test_app().await;
    let token = test_token();

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/jobs/nonexistent-id-12345?workspace_id=ws-default-001",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent job");
}

// ============================================================================
// Update Job — not found
// ============================================================================

#[tokio::test]
async fn test_update_job_not_found() {
    let app = setup_test_app().await;
    let token = test_token();

    let body = json!({ "name": "updated-job" });

    let response = app
        .oneshot(auth_request("PUT", "/api/v1/jobs/nonexistent-id-12345", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent job");
}

// ============================================================================
// Delete Job — not found
// ============================================================================

#[tokio::test]
async fn test_delete_job_not_found() {
    let app = setup_test_app().await;
    let token = test_token();

    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/jobs/nonexistent-id-12345?workspace_id=ws-default-001",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error for nonexistent job");
}

// ============================================================================
// Job Statistics
// ============================================================================

#[tokio::test]
async fn test_get_job_statistics() {
    let app = setup_test_app().await;
    let token = test_token();

    let response =
        app.oneshot(auth_request("GET", "/api/v1/jobs/statistics", &token, None)).await.unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"]["total_jobs"].is_number(), "Expected total_jobs");
    assert!(json["result"]["enabled_jobs"].is_number(), "Expected enabled_jobs");
    assert!(json["result"]["disabled_jobs"].is_number(), "Expected disabled_jobs");
}

// ============================================================================
// Regression: workspace resolution failure should return proper JSON error
// ============================================================================

#[tokio::test]
async fn test_get_job_statistics_no_workspace() {
    let app = setup_test_app().await;
    // Token with a tenant that has no workspace — simulates the production bug
    let token = create_test_token_with_workspace("user-1", "tenant-no-workspace", "ws-default-001");

    let response =
        app.oneshot(auth_request("GET", "/api/v1/jobs/statistics", &token, None)).await.unwrap();

    let (status, json) = response_parts(response).await;
    // HTTP status is 200, but the API returns a JSON error body
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code when no workspace found");
    assert!(
        json["msg"].as_str().is_some_and(|m| !m.is_empty()),
        "Should have a non-empty error message, got: {:?}",
        json["msg"]
    );
}

// ============================================================================
// List All Executions
// ============================================================================

#[tokio::test]
async fn test_list_all_executions() {
    let app = setup_test_app().await;
    let token = test_token();

    let response = app
        .oneshot(auth_request("GET", "/api/v1/jobs/executions?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"]["data"].is_array(), "Expected data array");
    assert!(json["result"]["pagination"].is_object(), "Expected pagination");
}

// ============================================================================
// Regression: statistics must match list_jobs workspace scope
// ============================================================================

#[tokio::test]
async fn test_statistics_scopes_to_workspace() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;

    let api_router = crate::api::create_router();
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);

    // User A creates a job in workspace A
    let token_a = create_test_token_with_workspace("user-a", "tenant-a", "ws-a");
    let body = json!({
        "name": "job-in-ws-a",
        "job_type": "shell",
        "cron_expression": "0 0 * * * *",
        "config": "{\"command\":\"echo hello\"}"
    });

    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/jobs", &token_a, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success creating job in workspace A");

    // User B (workspace B) lists jobs — should see 0
    let token_b = create_test_token_with_workspace("user-b", "tenant-b", "ws-b");

    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/jobs?page=1&page_size=20", &token_b, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    let list_count = json["result"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(list_count, 0, "list_jobs should return 0 jobs for workspace B");

    // User B queries statistics — should also see 0 total_jobs
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/jobs/statistics", &token_b, None))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code for statistics");

    let stats_total = json["result"]["total_jobs"].as_i64().unwrap_or(-1);
    assert_eq!(
        stats_total, 0,
        "BUG: statistics total_jobs ({}) does not match list_jobs count (0). \
         Statistics counts globally instead of scoping to workspace.",
        stats_total
    );

    let enabled_jobs = json["result"]["enabled_jobs"].as_i64().unwrap_or(-1);
    assert_eq!(
        enabled_jobs, 0,
        "BUG: statistics enabled_jobs ({}) should be 0 for workspace B",
        enabled_jobs
    );
}
