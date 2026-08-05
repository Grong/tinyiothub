//! System handler integration tests

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

// ── Configuration ──

#[tokio::test]
async fn test_get_system_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/system/system", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_network_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/system/network", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_mqtt_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/system/mqtt", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Features ──

#[tokio::test]
async fn test_get_system_features() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("GET", "/api/v1/system/features", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Tasks ──

#[tokio::test]
async fn test_list_system_tasks() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/system/tasks?page=1&page_size=20", &token, None))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_create_system_task_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/system/tasks", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_get_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/system/tasks/nonexistent-task-12345", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // Handler may return code 0 with null result or error code
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_update_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/system/tasks/nonexistent-task-12345",
            &token,
            Some(json!({"name": "updated"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_delete_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/system/tasks/nonexistent-task-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Configuration — write endpoints ──

#[tokio::test]
async fn test_update_system_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/system",
            &token,
            Some(json!({"system_name": "test"})),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_update_network_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/network",
            &token,
            Some(json!({"host": "0.0.0.0"})),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_update_mqtt_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/mqtt",
            &token,
            Some(json!({"broker": "mqtt.example.com"})),
        ))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_initialize_system() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/system/initialize", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_restart_system() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("POST", "/api/v1/system/restart", &token, None)).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_shutdown_system() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("POST", "/api/v1/system/shutdown", &token, None)).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ── Task Actions ──

#[tokio::test]
async fn test_enable_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/tasks/nonexistent-task-12345/enable",
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
async fn test_disable_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/tasks/nonexistent-task-12345/disable",
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
async fn test_run_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/system/tasks/nonexistent-task-12345/run",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}
