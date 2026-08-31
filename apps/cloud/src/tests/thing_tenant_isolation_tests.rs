//! Thing tenant isolation integration tests (design doc section 八, suite 3)
//!
//! Two workspaces A and B, a thing in each. Cross-workspace access must be
//! invisible (404, not 403) for CRUD/profile/ontology, attach/detach of a
//! foreign resource must fail, and a confirm token minted in workspace A
//! must be rejected (403) from workspace B.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace, setup_test_app_with_pool,
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

/// App with two seeded workspaces in one tenant.
async fn setup_two_workspaces() -> (axum::Router, sqlx::SqlitePool, crate::state::AppState) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    seed_test_workspace(&pool, "tenant-1", "ws-b").await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new()
        .nest("/api", api_router)
        .with_state(app_state.clone());
    (app, pool, app_state)
}

fn token(workspace_id: &str) -> String {
    create_test_token_with_workspace("user-1", "tenant-1", workspace_id)
}

async fn create_thing(app: &axum::Router, token: &str, workspace_id: &str, name: &str) -> String {
    let body = json!({"name": name, "thingType": "device", "workspaceId": workspace_id});
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "thing create failed: {json:?}");
    json["result"]["id"].as_str().unwrap().to_string()
}

// ──────────────────────────────────────────────
// CRUD cross-workspace → 404
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_cross_workspace_crud_returns_404() {
    let (app, _pool, _app_state) = setup_two_workspaces().await;
    let token_a = token("ws-a");
    let token_b = token("ws-b");
    let thing_a = create_thing(&app, &token_a, "ws-a", "thing-in-a").await;

    // GET from workspace B → 404 (invisible, not 403)
    let r = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{thing_a}"),
            &token_b,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-workspace GET must be 404, got {status}: {body:?}"
    );

    // PUT from workspace B → 404
    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing_a}"),
            &token_b,
            Some(json!({"displayName": "hacked"})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-workspace PUT must be 404, got {status}: {body:?}"
    );

    // DELETE from workspace B → 404
    let r = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/things/{thing_a}"),
            &token_b,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-workspace DELETE must be 404, got {status}: {body:?}"
    );

    // Thing still intact for workspace A.
    let r = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{thing_a}"),
            &token_a,
            None,
        ))
        .await
        .unwrap();
    let (status, _) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "thing must survive cross-workspace attempts");
}

// ──────────────────────────────────────────────
// Profile / ontology cross-workspace → 404
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_cross_workspace_profile_and_ontology_404() {
    let (app, _pool, _app_state) = setup_two_workspaces().await;
    let token_a = token("ws-a");
    let token_b = token("ws-b");
    let thing_a = create_thing(&app, &token_a, "ws-a", "thing-profile-a").await;

    for suffix in ["profile", "ontology"] {
        let r = app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/things/{thing_a}/{suffix}"),
                &token_b,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_parts(r).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-workspace GET /{suffix} must be 404, got {status}: {body:?}"
        );
    }
}

// ──────────────────────────────────────────────
// Attach / detach resource cross-workspace → error
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_cross_workspace_attach_detach_resource_error() {
    let (app, pool, _state) = setup_two_workspaces().await;
    let token_a = token("ws-a");
    let thing_a = create_thing(&app, &token_a, "ws-a", "thing-res-a").await;

    // Resource owned by workspace B.
    sqlx::query(
        "INSERT INTO resources (id, workspace_id, name, created_at, updated_at)
         VALUES ('res-b', 'ws-b', 'Doc B', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("insert resource in ws-b");

    // Attach B's resource to A's thing → must fail (404: resource not found in this workspace).
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/{thing_a}/resources"),
            &token_a,
            Some(json!({"resource_id": "res-b"})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-workspace attach must fail, got {status}: {body:?}"
    );

    // Detach B's resource from A's thing → must fail.
    let r = app
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/things/{thing_a}/resources/res-b"),
            &token_a,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-workspace detach must fail, got {status}: {body:?}"
    );

    // Resource untouched.
    let thing_id: Option<String> = sqlx::query_scalar("SELECT thing_id FROM resources WHERE id = 'res-b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(thing_id, None, "cross-workspace attach must not modify the resource");
}

// ──────────────────────────────────────────────
// Confirm token cross-workspace → 403
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_cross_workspace_confirm_token_403() {
    let (app, _pool, app_state) = setup_two_workspaces().await;
    let token_a = token("ws-a");
    let token_b = token("ws-b");
    let thing_a = create_thing(&app, &token_a, "ws-a", "thing-confirm-a").await;

    // Mint a pending-action token scoped to workspace A directly.
    let confirm_token = crate::domains::agent::host::tools::thing::store_pending_action(
        &app_state.pending_actions,
        thing_a.clone(),
        "reboot".to_string(),
        None,
        "ws-a".to_string(),
    );

    // Confirm from workspace B → 403 (token exists, but belongs to another workspace).
    let r = app
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/{thing_a}/actions/reboot/confirm"),
            &token_b,
            Some(json!({"token": confirm_token})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-workspace confirm must be 403, got {status}: {body:?}"
    );
}
