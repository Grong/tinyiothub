//! Marketplace handler integration tests

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

/// Verify the response follows the ApiResponse wrapper format:
/// { code: number, msg: string, result: T | null }
fn assert_api_response_format(json: &Value) {
    assert!(
        json.get("code").is_some(),
        "Response missing 'code' field: {}",
        json
    );
    assert!(
        json["code"].is_number(),
        "Expected 'code' to be a number, got: {}",
        json["code"]
    );
    assert!(
        json.get("msg").is_some(),
        "Response missing 'msg' field: {}",
        json
    );
    assert!(
        json.get("result").is_some(),
        "Response missing 'result' field: {}",
        json
    );
}

// ── Templates ──

#[tokio::test]
async fn test_list_marketplace_templates() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/marketplace/templates", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_list_marketplace_templates_with_pagination_params() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/marketplace/templates?page=1&pageSize=10",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_get_marketplace_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/marketplace/templates/nonexistent-tpl-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_install_marketplace_template_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/marketplace/templates/nonexistent-tpl-12345/install",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
    // Nonexistent template should return an error code
    assert_ne!(json["code"], 0, "Expected error for nonexistent template");
}

// ── Drivers ──

#[tokio::test]
async fn test_list_marketplace_drivers() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/marketplace/drivers", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_get_marketplace_driver_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/marketplace/drivers/nonexistent-drv-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_install_marketplace_driver_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/marketplace/drivers/nonexistent-drv-12345/install",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
}

#[tokio::test]
async fn test_install_marketplace_driver_requires_admin() {
    let app = setup_test_app().await;
    // Regular user token (no admin role)
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "POST",
            "/api/v1/marketplace/drivers/some-driver/install",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_api_response_format(&json);
    // Should fail with permission error (user-1 has no admin role)
    assert_ne!(json["code"], 0, "Expected permission error for non-admin");
}

// ── Response format ──

#[tokio::test]
async fn test_marketplace_response_wrapper_consistency() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Both templates and drivers should use the same ApiResponse wrapper
    let template_response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/marketplace/templates", &token, None))
        .await
        .unwrap();
    let driver_response = app
        .oneshot(auth_request("GET", "/api/v1/marketplace/drivers", &token, None))
        .await
        .unwrap();

    let (_s, template_json) = response_parts(template_response).await;
    let (_s, driver_json) = response_parts(driver_response).await;

    assert_api_response_format(&template_json);
    assert_api_response_format(&driver_json);

    // Both should have the same top-level structure
    assert_eq!(
        template_json.as_object().map(|o| o.keys().collect::<Vec<_>>()),
        driver_json.as_object().map(|o| o.keys().collect::<Vec<_>>()),
    );
}
