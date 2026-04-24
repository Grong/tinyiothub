//! Heartbeat handler integration tests

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

#[tokio::test]
async fn test_get_heartbeat() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/heartbeat", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_report_heartbeat() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("POST", "/api/v1/heartbeat", &token, Some(json!({})))).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_get_heartbeat_config() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("GET", "/api/v1/heartbeat/config", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_configure_heartbeat() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app.oneshot(auth_request("PUT", "/api/v1/heartbeat/config", &token, Some(json!({"enabled": true})))).await.unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}
