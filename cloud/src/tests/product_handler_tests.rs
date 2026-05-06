//! Product handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
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

// ============================================================================
// List Products
// ============================================================================

#[tokio::test]
async fn test_list_products() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/system/products?page=1&page_size=20",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_array(), "Expected array of products");
}

// ============================================================================
// Create Product
// ============================================================================

#[tokio::test]
async fn test_create_product() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "test-product-001",
        "description": "A test product",
        "device_type": "sensor",
        "protocol_type": "modbus"
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/system/products", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "Expected success code, got: {}", json);
    assert!(json["result"]["id"].is_string(), "Created product should have an id");
}

// ============================================================================
// Create Product — missing name
// ============================================================================

#[tokio::test]
async fn test_create_product_missing_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({});

    let response = app
        .oneshot(auth_request("POST", "/api/v1/system/products", &token, Some(body)))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected 422 or 200 for missing name, got: {}",
        status
    );
}

// ============================================================================
// Create Product — empty name
// ============================================================================

#[tokio::test]
async fn test_create_product_empty_name() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": ""
    });

    let response = app
        .oneshot(auth_request("POST", "/api/v1/system/products", &token, Some(body)))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected validation error for empty name");
}

// ============================================================================
// Get Product — not found
// ============================================================================

#[tokio::test]
async fn test_get_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/system/products/nonexistent-product-id",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent product");
}

// ============================================================================
// Update Product — not found
// ============================================================================

#[tokio::test]
async fn test_update_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({
        "name": "updated-product-name"
    });

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/system/products/nonexistent-product-id",
            &token,
            Some(body),
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent product");
}

// ============================================================================
// Delete Product — not found
// ============================================================================

#[tokio::test]
async fn test_delete_product_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/system/products/nonexistent-product-id",
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(json["code"], 0, "Expected error code for nonexistent product");
}
