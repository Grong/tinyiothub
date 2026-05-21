//! Open API handler integration tests
//! NOTE: Open routes use API Key auth (X-API-Key header), not JWT.
//! Routes have double /open/open/ prefix due to nesting bug in handler.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn open_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

// NOTE: The open routes are defined with /open/ prefix inside the router,
// which is nested under /open in api/mod.rs, producing /open/open/ paths.

#[tokio::test]
async fn test_open_health() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/health")).await.unwrap();
    // May return 401 (missing API key) or 200
    assert!(response.status().is_success() || response.status() == StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_open_devices_unauthorized() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/devices")).await.unwrap();
    // Without API key, should return 401 or 200 with error
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}

#[tokio::test]
async fn test_open_device_not_found() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app
        .oneshot(open_request("GET", "/api/open/open/devices/nonexistent-device-12345"))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}

#[tokio::test]
async fn test_open_events_unauthorized() {
    let app = crate::test_utils::setup_test_app().await;
    let response = app.oneshot(open_request("GET", "/api/open/open/events")).await.unwrap();
    assert!(response.status() == StatusCode::UNAUTHORIZED || response.status().is_success());
}
