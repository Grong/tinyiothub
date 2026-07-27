//! Thing handler integration tests
//!
//! Tests thing CRUD endpoints using `tower::ServiceExt::oneshot()`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace,
    setup_test_app_with_pool,
};

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

/// Create a test app with a seeded workspace.
async fn setup_with_workspace(tenant_id: &str, workspace_id: &str) -> axum::Router {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, tenant_id, workspace_id).await;
    let api_router = crate::api::create_router();
    axum::Router::new().nest("/api", api_router).with_state(app_state)
}

// ──────────────────────────────────────────────
// Create Thing
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_create_thing_empty_name_rejected() {
    // Regression: the old device API rejected empty names with 422
    let app = setup_with_workspace("tenant-1", "ws-default").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default");

    for name in ["", "   "] {
        let body = json!({ "name": name, "thingType": "device" });
        let response =
            app.clone().oneshot(auth_request("POST", "/api/v1/things", &token, Some(body))).await.unwrap();
        let (status, _json) = response_parts(response).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "name {:?} must be rejected", name);
    }
}

#[tokio::test]
async fn test_create_thing() {
    let app = setup_with_workspace("tenant-1", "ws-default").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-default");

    let body = json!({
        "name": "test-thing-001",
        "thingType": "device",
        "deviceType": "sensor",
        "protocolType": "modbus",
        "workspaceId": "ws-default"
    });

    let response =
        app.oneshot(auth_request("POST", "/api/v1/things", &token, Some(body))).await.unwrap();

    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "Expected 201, got {}: {:?}", status, json);
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(json["result"].is_object(), "Expected thing object");
    assert_eq!(json["result"]["name"], "test-thing-001");
    assert_eq!(json["result"]["thingType"], "device");
}

// ──────────────────────────────────────────────
// Name Conflict (same workspace)
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_name_conflict_same_workspace() {
    let app = setup_with_workspace("tenant-1", "ws-conflict").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-conflict");

    let body = json!({
        "name": "dup-thing",
        "thingType": "device",
        "workspaceId": "ws-conflict"
    });

    // First create — should succeed
    let r1 = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body.clone())))
        .await
        .unwrap();
    let (s1, _j1) = response_parts(r1).await;
    assert_eq!(s1, StatusCode::CREATED, "First create should succeed");

    // Second create same name — should fail with 409
    let r2 = app.oneshot(auth_request("POST", "/api/v1/things", &token, Some(body))).await.unwrap();
    let (s2, j2) = response_parts(r2).await;
    assert_eq!(s2, StatusCode::CONFLICT, "Expected 409, got {}: {:?}", s2, j2);
    assert!(j2["code"].as_i64().unwrap_or(0) != 0, "Expected error code");
}

// ──────────────────────────────────────────────
// Pagination clamp
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_pagination_clamp() {
    let app = setup_with_workspace("tenant-1", "ws-paginate").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-paginate");

    // limit=500 should be clamped to 200
    let response = app
        .oneshot(auth_request("GET", "/api/v1/things?limit=500&offset=0", &token, None))
        .await
        .unwrap();

    let (_status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "Expected success code");
    assert!(
        json["result"]["limit"].as_u64().unwrap_or(999) <= 200,
        "limit should be clamped to 200"
    );
}

// ──────────────────────────────────────────────
// Parent ID cycle rejected
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_parent_id_cycle_rejected() {
    let app = setup_with_workspace("tenant-1", "ws-cycle").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-cycle");

    // Create thing A
    let body_a = json!({"name": "thing-A", "thingType": "device", "workspaceId": "ws-cycle"});
    let r_a = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body_a)))
        .await
        .unwrap();
    let (_s, j_a) = response_parts(r_a).await;
    let a_id = j_a["result"]["id"].as_str().unwrap().to_string();

    // Create thing B with parent_id = A
    let body_b = json!({
        "name": "thing-B",
        "thingType": "device",
        "parentId": &a_id,
        "workspaceId": "ws-cycle"
    });
    let r_b = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body_b)))
        .await
        .unwrap();
    let (_s, j_b) = response_parts(r_b).await;
    let b_id = j_b["result"]["id"].as_str().unwrap().to_string();

    // Try to set A's parent to B → cycle
    let update_body = json!({"parentId": &b_id});
    let r_cycle = app
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{}", a_id),
            &token,
            Some(update_body),
        ))
        .await
        .unwrap();
    let (s_cycle, j_cycle) = response_parts(r_cycle).await;
    assert_eq!(s_cycle, StatusCode::CONFLICT, "Expected 409, got {}: {:?}", s_cycle, j_cycle);
    assert!(j_cycle["code"].as_i64().unwrap_or(0) != 0, "Expected error code for cycle");
}

// ──────────────────────────────────────────────
// Delete with children rejected
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_delete_with_children_rejected() {
    let app = setup_with_workspace("tenant-1", "ws-delete").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-delete");

    // Create parent thing
    let body_parent =
        json!({"name": "parent-thing", "thingType": "space", "workspaceId": "ws-delete"});
    let r_p = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body_parent)))
        .await
        .unwrap();
    let (_s, j_p) = response_parts(r_p).await;
    let parent_id = j_p["result"]["id"].as_str().unwrap().to_string();

    // Create child thing under parent
    let body_child = json!({
        "name": "child-thing",
        "thingType": "device",
        "parentId": &parent_id,
        "workspaceId": "ws-delete"
    });
    let r_c = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body_child)))
        .await
        .unwrap();
    let (_s, j_c) = response_parts(r_c).await;
    assert!(j_c["result"]["id"].is_string(), "Child should be created successfully");

    // Try to delete parent → 409
    let r_del = app
        .oneshot(auth_request("DELETE", &format!("/api/v1/things/{}", parent_id), &token, None))
        .await
        .unwrap();
    let (s_del, j_del) = response_parts(r_del).await;
    assert_eq!(s_del, StatusCode::CONFLICT, "Expected 409, got {}: {:?}", s_del, j_del);
    assert!(j_del["code"].as_i64().unwrap_or(0) != 0, "Expected error code for children");
}

// ──────────────────────────────────────────────
// Get single thing
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_get_thing() {
    let app = setup_with_workspace("tenant-1", "ws-get").await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-get");

    // Create first
    let body = json!({"name": "get-test-thing", "thingType": "line", "workspaceId": "ws-get"});
    let r = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", &token, Some(body)))
        .await
        .unwrap();
    let (_s, j) = response_parts(r).await;
    let id = j["result"]["id"].as_str().unwrap();

    // Get by id
    let r_get = app
        .oneshot(auth_request("GET", &format!("/api/v1/things/{}", id), &token, None))
        .await
        .unwrap();
    let (s_get, j_get) = response_parts(r_get).await;
    assert_eq!(s_get, StatusCode::OK);
    assert_eq!(j_get["code"], 0);
    assert_eq!(j_get["result"]["id"], id);
    assert_eq!(j_get["result"]["name"], "get-test-thing");
    assert_eq!(j_get["result"]["thingType"], "line");
    assert!(j_get["result"]["breadcrumb"].is_array(), "Breadcrumb should be array");
}
