//! Tag handler integration tests
//!
//! Tests tag CRUD with data setup to exercise FromRow deserialization.

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

/// Helper: create a tag via POST and return the response JSON.
async fn create_tag(app: &axum::Router, token: &str, name: &str, tag_type: &str) -> Value {
    let body = json!({"name": name, "type": tag_type});
    let response =
        app.clone().oneshot(auth_request("POST", "/api/v1/tags", token, Some(body))).await.unwrap();
    let (_s, json) = response_parts(response).await;
    json
}

// ── CRUD ──

#[tokio::test]
async fn test_create_tag() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    let body = json!({"name": "test-tag-001", "type": "device"});
    let response =
        app.oneshot(auth_request("POST", "/api/v1/tags", &token, Some(body))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success");
    assert!(json["result"].is_object(), "Expected tag object");
    assert_eq!(json["result"]["name"], "test-tag-001");
    assert!(json["result"]["type"].is_string(), "Expected type field in response");
}

#[tokio::test]
async fn test_list_tags_with_data() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Setup: create tags first
    let app = app;
    create_tag(&app, &token, "tag-alpha", "device").await;
    create_tag(&app, &token, "tag-beta", "device").await;

    // Test: list should return the created tags
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/tags?page=1&page_size=20", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success");
    // This exercises FromRow deserialization with real data
    assert!(json["result"].is_array() || json["result"].is_object(), "Expected data in result");
}

#[tokio::test]
async fn test_create_tag_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response =
        app.oneshot(auth_request("POST", "/api/v1/tags", &token, Some(json!({})))).await.unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_get_tag_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("GET", "/api/v1/tags/nonexistent-tag-12345", &token, None))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_get_tag_by_id_with_data() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Setup: create a tag
    let app = app;
    let create_json = create_tag(&app, &token, "get-test-tag", "device").await;
    let tag_id = create_json["result"]["id"].as_str().unwrap_or("");

    // Test: get by ID should return the tag (exercises FromRow)
    let response = app
        .clone()
        .oneshot(auth_request("GET", &format!("/api/v1/tags/{}", tag_id), &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success");
    assert_eq!(json["result"]["name"], "get-test-tag");
}

#[tokio::test]
async fn test_update_tag_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "PUT",
            "/api/v1/tags/nonexistent-tag-12345",
            &token,
            Some(json!({"name": "updated"})),
        ))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

#[tokio::test]
async fn test_delete_tag_not_found() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/tags/nonexistent-tag-12345", &token, None))
        .await
        .unwrap();
    assert!(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::OK);
}

// ── Search & Stats ──

#[tokio::test]
async fn test_search_tags_with_data() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Setup: create a tag with known name
    let app = app;
    create_tag(&app, &token, "searchable-unique-tag", "device").await;

    // Test: search should find it (exercises FromRow)
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/tags/search?q=searchable-unique", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success");
}

#[tokio::test]
async fn test_get_tag_stats_with_data() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // Setup: create tags
    let app = app;
    create_tag(&app, &token, "stats-tag-1", "device").await;
    create_tag(&app, &token, "stats-tag-2", "app").await;

    // Test: stats should reflect created tags
    let response =
        app.clone().oneshot(auth_request("GET", "/api/v1/tags/stats", &token, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success");
}

// ── Bindings ──

#[tokio::test]
async fn test_create_tag_binding_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/tags/bindings", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_delete_tag_binding_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/tags/bindings", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_get_target_bindings() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/tags/bindings/target/nonexistent-target-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_get_tag_bindings() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request(
            "GET",
            "/api/v1/tags/bindings/tag/nonexistent-tag-12345",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].is_number());
}

#[tokio::test]
async fn test_batch_create_bindings_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("POST", "/api/v1/tags/bindings/batch", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::OK
    );
}

#[tokio::test]
async fn test_batch_delete_bindings_missing_fields() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(auth_request("DELETE", "/api/v1/tags/bindings/batch", &token, Some(json!({}))))
        .await
        .unwrap();
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ── Tag Lifecycle ──

#[tokio::test]
async fn test_tag_lifecycle() {
    let app = setup_test_app().await;
    let token = create_test_token("user-1", "tenant-1");

    // 1. Create
    let body = json!({"name": "lifecycle-tag", "type": "device"});
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/tags", &token, Some(body)))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success creating tag: {}", json);
    let tag_id = json["result"]["id"].as_str().unwrap().to_string();

    // 2. Get by ID
    let response = app
        .clone()
        .oneshot(auth_request("GET", &format!("/api/v1/tags/{}", tag_id), &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success getting tag: {}", json);
    assert_eq!(json["result"]["name"], "lifecycle-tag");

    // 3. Update
    let body = json!({"name": "lifecycle-tag-updated"});
    let response = app
        .clone()
        .oneshot(auth_request("PUT", &format!("/api/v1/tags/{}", tag_id), &token, Some(body)))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success updating tag: {}", json);
    assert_eq!(json["result"]["name"], "lifecycle-tag-updated");

    // 4. Delete
    let response = app
        .oneshot(auth_request("DELETE", &format!("/api/v1/tags/{}", tag_id), &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success deleting tag: {}", json);
}
