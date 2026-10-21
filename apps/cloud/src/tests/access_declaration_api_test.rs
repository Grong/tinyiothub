//! E4a access declaration API tests: driver install, declaration listing and
//! thing-creation materialization.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::shared::access_ports::{DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource};
use crate::test_utils::{auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace};
use tinyiothub_runtime::access::registry::ManifestRegistry;
use tinyiothub_runtime::access::service::AccessService;

const DEMO_MANIFEST: &str = include_str!("../../../../drivers/demo-declarative/driver.manifest.json");
const WORKSPACE: &str = "ws-default-001";

/// Build a test app with the E4a access service wired to an in-memory DB and
/// a temp package store.
async fn setup_with_access() -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    let (mut app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WORKSPACE).await;

    let data = tempfile::tempdir().expect("temp data dir");
    let manifests = Arc::new(ManifestRegistry::new());
    for (_, manifest) in tinyiothub_runtime::driver::describe::builtin_manifest_jsons() {
        manifests
            .seed_builtin(manifest)
            .expect("built-in manifest must validate");
    }
    let access = Arc::new(AccessService::new(
        data.path().to_path_buf(),
        manifests,
        Arc::new(DbDriverStore(app_state.db.clone())),
        Arc::new(DbDeviceStore(app_state.db.clone())),
        Arc::new(DbProjectionStore(app_state.db.clone())),
        Arc::new(DbThingSource(app_state.db.clone())),
        Arc::new(DbAcceptanceStore(app_state.db.clone())),
    ));
    app_state.set_access(access);

    let router = Router::new()
        .nest("/api", crate::api::create_router(&app_state))
        .with_state(app_state);
    (router, pool, data)
}

fn install_request(token: &str, manifest: &str) -> Request<Body> {
    let boundary = "TINYIOTHUB-BOUNDARY";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"manifest\"\r\n\r\n\
         {manifest}\r\n--{boundary}--\r\n"
    );
    Request::builder()
        .method("POST")
        .uri("/api/v1/drivers/install")
        .header("Authorization", auth_header(token))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap()
}

fn json_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    match body {
        Some(v) => builder.body(Body::from(v.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

fn demo_entries(json: &Value) -> Vec<&Value> {
    json["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|i| i["driver"].as_str() == Some("demo-declarative"))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn installs_multipart_package_and_lists_declaration() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WORKSPACE);

    let response = app
        .clone()
        .oneshot(install_request(&token, DEMO_MANIFEST))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], json!(true), "install failed: {body}");
    assert_eq!(body["driver"], "demo-declarative");
    assert_eq!(body["version"], "1.0.0");
    assert_eq!(
        body["manifestHash"].as_str().map(str::len),
        Some(64),
        "expected sha256 manifest hash"
    );

    let response = app
        .oneshot(json_request("GET", "/api/v1/drivers/declarations", &token, None))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], json!(true), "list failed: {body}");
    let entries = demo_entries(&body);
    assert_eq!(entries.len(), 1, "declarations response: {body}");
    assert_eq!(entries[0]["dynamicCapabilities"], json!(false));
}

#[tokio::test]
async fn install_without_manifest_field_is_rejected() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WORKSPACE);

    let boundary = "TINYIOTHUB-BOUNDARY";
    let body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--{boundary}--\r\n");
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/drivers/install")
        .header("Authorization", auth_header(&token))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], json!(false));
    assert!(
        body["error"].as_str().unwrap_or_default().contains("missing manifest"),
        "unexpected error: {body}"
    );
}

#[tokio::test]
async fn reinstall_keeps_single_declaration_entry() {
    let (app, pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WORKSPACE);

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(install_request(&token, DEMO_MANIFEST))
            .await
            .unwrap();
        let (status, body) = response_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], json!(true), "install failed: {body}");
    }

    let response = app
        .oneshot(json_request("GET", "/api/v1/drivers/declarations", &token, None))
        .await
        .unwrap();
    let (_, body) = response_parts(response).await;
    assert_eq!(demo_entries(&body).len(), 1, "reinstall duplicated entry: {body}");

    let rows = tinyiothub_storage::Db::new(pool)
        .list_active_driver_declarations(WORKSPACE)
        .await
        .unwrap();
    let demo: Vec<_> = rows.iter().filter(|r| r.driver_name == "demo-declarative").collect();
    assert_eq!(demo.len(), 1, "reinstall must leave exactly one active row");
}

#[tokio::test]
async fn create_thing_materializes_fixed_driver_declaration() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WORKSPACE);

    let response = app
        .clone()
        .oneshot(install_request(&token, DEMO_MANIFEST))
        .await
        .unwrap();
    let (_, body) = response_parts(response).await;
    assert_eq!(body["success"], json!(true), "install failed: {body}");

    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/things",
            &token,
            Some(json!({
                "name": "declarative-thing",
                "thingType": "device",
                "driverName": "demo-declarative",
            })),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {body}");
    let thing_id = body["result"]["id"].as_str().expect("thing id").to_string();

    let response = app
        .oneshot(json_request(
            "GET",
            &format!("/api/v1/things/{thing_id}/profile"),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "profile failed: {body}");
    let props = body["result"]["properties"].as_array().cloned().unwrap_or_default();
    assert!(
        props.iter().any(|p| p["name"] == "level"),
        "driver state field was not materialized: {body}"
    );
    let actions = body["result"]["actions"].as_array().cloned().unwrap_or_default();
    assert!(
        actions.iter().any(|a| a["name"] == "echo"),
        "driver tool was not materialized: {body}"
    );
}

#[tokio::test]
async fn create_thing_with_unknown_driver_is_rejected_before_insert() {
    let (app, pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace("user-1", "tenant-1", WORKSPACE);

    let response = app
        .oneshot(json_request(
            "POST",
            "/api/v1/things",
            &token,
            Some(json!({"name": "orphan-thing", "driverName": "not-installed"})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "unexpected: {body}");
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("has no installed manifest"),
        "unexpected error: {body}"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE name='orphan-thing'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rejected creation must not insert a thing");
}
