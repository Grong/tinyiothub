//! Driver Health handler integration tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token, response_parts, setup_test_app};

fn auth_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn test_list_driver_health_empty() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/driver-health/drivers", &token))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let (_status, json) = response_parts(response).await;

    // Verify ApiResponse wrapper
    assert_eq!(json["code"], 0, "Expected success code");

    // Verify result structure
    let result = &json["result"];
    assert!(result.is_object(), "Expected result to be an object");
    assert!(result["workspace_id"].is_string(), "Expected workspace_id field");
    assert_eq!(result["total_count"], 0, "Expected zero drivers for empty workspace");

    let drivers = &result["drivers"];
    assert!(drivers.is_array(), "Expected drivers to be an array");
    assert_eq!(drivers.as_array().unwrap().len(), 0, "Expected empty drivers list");
}

#[tokio::test]
async fn test_list_driver_health_requires_auth() {
    let app = setup_test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/driver-health/drivers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Without auth token, should return 401
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_list_driver_health_response_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/driver-health/drivers", &token))
        .await
        .unwrap();

    let (_status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0);

    // When no drivers are loaded, verify all expected fields exist
    let result = &json["result"];
    assert!(result.get("workspace_id").is_some());
    assert!(result.get("total_count").is_some());
    assert!(result.get("drivers").is_some());

    // Verify drivers array elements would have the correct shape
    // (Schema validation — actual drivers depend on runtime registry state)
}
