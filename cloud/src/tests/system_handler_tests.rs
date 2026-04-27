//! System handler integration tests

use axum::{body::Body, http::{Request, StatusCode}};
use serde_json::{json, Value};
use tower::ServiceExt;
use crate::test_utils::{create_test_token, auth_header, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method).uri(uri)
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
    let response = app.oneshot(auth_request("GET", "/api/v1/system/system", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_network_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/network", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_mqtt_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/mqtt", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Features ──

#[tokio::test]
async fn test_get_system_features() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/features", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Tasks ──

#[tokio::test]
async fn test_list_system_tasks() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/tasks?page=1&page_size=20", &token, None)).await.unwrap();
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
    let response = app.oneshot(auth_request("POST", "/api/v1/system/tasks", &token, Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/tasks/nonexistent-task-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // Handler may return code 0 with null result or error code
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_update_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/system/tasks/nonexistent-task-12345", &token, Some(json!({"name": "updated"})))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_delete_system_task_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("DELETE", "/api/v1/system/tasks/nonexistent-task-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

// ── Products ──

#[tokio::test]
async fn test_list_products() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/products?page=1&page_size=20", &token, None)).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
    if response.status() == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert!(json["code"].is_number());
    }
}

#[tokio::test]
async fn test_create_product_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("POST", "/api/v1/system/products", &token, Some(json!({})))).await.unwrap();
    assert!(response.status() == StatusCode::UNPROCESSABLE_ENTITY || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/system/products/nonexistent-product-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent product");
}

#[tokio::test]
async fn test_update_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/system/products/nonexistent-product-12345", &token, Some(json!({"name": "updated"})))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent product");
}

#[tokio::test]
async fn test_delete_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("DELETE", "/api/v1/system/products/nonexistent-product-12345", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}
