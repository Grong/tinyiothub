//! Seed → profile E2E (spec §6 USER FLOWS): an app booted through the real
//! `Db::connect` + `bootstrap::run_seeds` path serves the demo thing profile.
//!
//! Case 1: seed switch on (default) — `GET /api/v1/things/device-env-01/profile`
//! returns ≥5 properties and ≥2 actions.
//! Case 2: seed switch off — no demo things are seeded; a user-created device
//! with no properties yields empty arrays from the same endpoint.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, test_app_state_on_pool, test_settings,
};

/// Boot the real path: Db::connect (pool + migrations + FK enforcement) on a
/// temp file, then bootstrap::run_seeds with the given demo switch.
async fn boot_app(demo_data: bool) -> (axum::Router, sqlx::SqlitePool, std::path::PathBuf) {
    let mut settings = test_settings();
    settings.seed.demo_data = demo_data;

    let path = std::env::temp_dir().join(format!("tih-seed-e2e-{}-{}.db", std::process::id(), demo_data));
    let _ = std::fs::remove_file(&path);

    let db_config = tinyiothub_storage::DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", path.display()),
        max_connections: 1,
        min_connections: 0,
        acquire_timeout_secs: 30,
        idle_timeout_secs: 600,
    };
    let db = tinyiothub_storage::Db::connect(&db_config).await.expect("Db::connect");
    crate::bootstrap::run_seeds(&db, &settings).await.expect("run_seeds");

    let pool = db.pool().clone();
    let app_state = test_app_state_on_pool(pool.clone()).await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool, path)
}

fn profile_request(token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/api/v1/things/device-env-01/profile")
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

fn profile_json(body: &Value) -> &Value {
    assert_eq!(body["code"], 0, "expected success code, got: {}", body);
    &body["result"]
}

#[tokio::test]
async fn seeded_app_serves_demo_thing_profile() {
    let (app, _pool, path) = boot_app(true).await;
    let token = create_test_token_with_workspace("user-1", "tenant-default-001", "ws-default-001");

    let response = app.oneshot(profile_request(&token)).await.unwrap();
    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    let result = profile_json(&json);
    let properties = result["properties"].as_array().expect("properties array");
    assert!(
        properties.len() >= 5,
        "env01 must expose its 5 seed properties, got {}",
        properties.len()
    );
    let actions = result["actions"].as_array().expect("actions array");
    assert!(
        actions.len() >= 2,
        "env01 must expose its 2 seed actions, got {}",
        actions.len()
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn demo_seed_off_yields_empty_profile_arrays() {
    let (app, pool, path) = boot_app(false).await;

    // 开关关闭时不播种演示设备；模拟用户自建的无属性设备。
    sqlx::query(
        "INSERT INTO things (id, name, display_name, workspace_id, tenant_id)
         VALUES ('device-env-01', 'user_created_env', '用户自建设备', 'ws-default-001', 'tenant-default-001')",
    )
    .execute(&pool)
    .await
    .expect("insert bare device");

    let token = create_test_token_with_workspace("user-1", "tenant-default-001", "ws-default-001");
    let response = app.oneshot(profile_request(&token)).await.unwrap();
    let (status, json) = response_parts(response).await;

    assert_eq!(status, StatusCode::OK);
    let result = profile_json(&json);
    assert_eq!(
        result["properties"].as_array().map(Vec::len),
        Some(0),
        "no seed → empty properties"
    );
    assert_eq!(
        result["actions"].as_array().map(Vec::len),
        Some(0),
        "no seed → empty actions"
    );

    let _ = std::fs::remove_file(&path);
}
