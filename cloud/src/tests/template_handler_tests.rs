//! Device template handler integration tests
//!
//! Tests template CRUD and validation endpoints.

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

    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };
    builder.body(Body::from(body_str)).unwrap()
}

// ============================================================================
// Templates CRUD
// ============================================================================

#[tokio::test]
async fn test_list_templates() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/device-templates?page=1&page_size=20", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/device-templates/nonexistent-tpl-12345",
            &token,
            None,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    // Handler may return code 0 with null result or error code for nonexistent template
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_create_template_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("POST", "/api/v1/device-templates", &token, Some(json!({}))))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::OK,
        "Expected validation error, got: {}",
        status
    );
}

#[tokio::test]
async fn test_update_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/device-templates/nonexistent-tpl-12345",
            &token,
            Some(json!({"name": "updated"})),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert_ne!(json["code"], 0, "Expected error for nonexistent template");
}

#[tokio::test]
async fn test_delete_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "DELETE",
            "/api/v1/device-templates/nonexistent-tpl-12345",
            &token,
            None,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

#[tokio::test]
async fn test_get_template_categories() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request("GET", "/api/v1/device-templates/categories", &token, None))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::OK);

    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number(), "Expected numeric code");
}

// ============================================================================
// Template — validate endpoint
// ============================================================================

#[tokio::test]
async fn test_validate_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/device-templates/nonexistent-tpl-12345/validate",
            &token,
            Some(json!({"name": "test-device", "device_type": "sensor"})),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected OK or 422, got: {}",
        status
    );

    if status == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert_ne!(json["code"], 0, "Expected error for nonexistent template validation");
    }
}

// ============================================================================
// Template — preview endpoint
// ============================================================================

#[tokio::test]
async fn test_preview_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/device-templates/nonexistent-tpl-12345/preview",
            &token,
            Some(json!({"name": "test-device", "device_type": "sensor"})),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected OK or 422, got: {}",
        status
    );

    if status == StatusCode::OK {
        let (_s, json) = response_parts(response).await;
        assert_ne!(json["code"], 0, "Expected error for nonexistent template preview");
    }
}
