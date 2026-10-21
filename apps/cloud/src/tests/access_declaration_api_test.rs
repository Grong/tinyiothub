//! E4a access declaration API tests: driver install, declaration listing,
//! authorization, workspace scoping and thing-creation materialization.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::FromRef,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::domains::thing::hooks::{CreationMode, ThingDeclarationHook};
use crate::domains::thing::service::ThingService;
use crate::domains::thing::types::CreateThingRequest;
use crate::shared::access_ports::{DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource};
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_admin_role, seed_test_workspace,
};
use tinyiothub_runtime::access::registry::ManifestRegistry;
use tinyiothub_runtime::access::service::AccessService;

const DEMO_MANIFEST: &str = include_str!("../../../../drivers/demo-declarative/driver.manifest.json");
const WORKSPACE: &str = "ws-default-001";
const ADMIN_USER: &str = "user-1";
const NON_ADMIN_USER: &str = "user-2";

/// Dynamic-capability driver that ships no binary and is not a built-in:
/// unusable, so install must reject it instead of returning success.
const DYNAMIC_NO_BINARY_MANIFEST: &str = r#"{
  "manifestVersion": "1.0",
  "identity": { "driver": "demo-dynamic", "version": "1.0.0", "protocol": "demo",
                "abiVersion": 2, "dynamicCapabilities": true },
  "config": { "type": "object" },
  "tools": [], "stateFields": [], "sources": [], "resources": [],
  "placement": { "mode": "runtime" },
  "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
}"#;

fn access_service(db: Arc<tinyiothub_storage::Db>, data_dir: PathBuf) -> Arc<AccessService> {
    let manifests = Arc::new(ManifestRegistry::new());
    for (_, manifest) in tinyiothub_runtime::driver::describe::builtin_manifest_jsons() {
        manifests
            .seed_builtin(manifest)
            .expect("built-in manifest must validate");
    }
    Arc::new(AccessService::new(
        data_dir,
        manifests,
        Arc::new(DbDriverStore(db.clone())),
        Arc::new(DbDeviceStore(db.clone())),
        Arc::new(DbProjectionStore(db.clone())),
        Arc::new(DbThingSource(db.clone())),
        Arc::new(DbAcceptanceStore(db)),
    ))
}

/// Build a test app with the E4a access service wired to an in-memory DB and
/// a temp package store. `user-1` is an admin.
async fn setup_in(workspace: &str) -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    let (mut app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", workspace).await;
    seed_admin_role(&pool, ADMIN_USER).await;
    seed_admin_role(&pool, NON_ADMIN_USER).await;
    // The negative test needs *no* admin grant; drop the one seeded for the
    // non-admin user (seed_admin_role also creates the user row).
    sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
        .bind(NON_ADMIN_USER)
        .execute(&pool)
        .await
        .expect("remove non-admin grant");

    let data = tempfile::tempdir().expect("temp data dir");
    app_state.set_access(access_service(app_state.db.clone(), data.path().to_path_buf()));

    let router = Router::new()
        .nest("/api", crate::api::create_router(&app_state))
        .with_state(app_state);
    (router, pool, data)
}

async fn setup_with_access() -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    setup_in(WORKSPACE).await
}

/// Locate the built reference cdylib so installs can carry a real binary.
fn demo_cdylib() -> Option<PathBuf> {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    [
        "libdemo_declarative.dylib",
        "libdemo_declarative.so",
        "demo_declarative.dll",
        "libdemo_declarative.dll",
    ]
    .iter()
    .map(|name| target.join(profile).join(name))
    .find(|path| path.is_file())
}

fn skip_without_demo_cdylib(test: &str) -> bool {
    if demo_cdylib().is_none() {
        eprintln!("skipping {test}: demo-declarative cdylib not built");
        return true;
    }
    false
}

fn unique_workspace(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

struct InstallBody {
    body: Vec<u8>,
    boundary: &'static str,
}

impl InstallBody {
    fn new(manifest: &str, binary: Option<&Path>) -> Self {
        const BOUNDARY: &str = "TINYIOTHUB-BOUNDARY";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"manifest\"\r\n\r\n\
                 {manifest}\r\n"
            )
            .as_bytes(),
        );
        if let Some(binary) = binary {
            body.extend_from_slice(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"binary\"; \
                     filename=\"driver.so\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&std::fs::read(binary).expect("read demo cdylib"));
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
        Self {
            body,
            boundary: BOUNDARY,
        }
    }

    fn request(self, token: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/v1/drivers/install")
            .header("Authorization", auth_header(token))
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", self.boundary),
            )
            .body(Body::from(self.body))
            .unwrap()
    }
}

fn install_request(token: &str, manifest: &str) -> Request<Body> {
    InstallBody::new(manifest, None).request(token)
}

fn install_request_with_binary(token: &str, manifest: &str) -> Request<Body> {
    let binary = demo_cdylib().expect("demo cdylib must be built for this test");
    InstallBody::new(manifest, Some(&binary)).request(token)
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
    if skip_without_demo_cdylib("installs_multipart_package_and_lists_declaration") {
        return;
    }
    let ws = unique_workspace("ws-install");
    let (app, _pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    let response = app
        .clone()
        .oneshot(install_request_with_binary(&token, DEMO_MANIFEST))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], json!(true), "install failed: {body}");
    assert_eq!(body["driver"], "demo-declarative");
    assert_eq!(body["version"], "1.0.0");
    assert_eq!(body["warnings"], json!([]));
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
async fn install_without_manifest_field_is_rejected_with_400() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", WORKSPACE);

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
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], json!(false));
    assert!(
        body["error"].as_str().unwrap_or_default().contains("missing manifest"),
        "unexpected error: {body}"
    );
}

#[tokio::test]
async fn install_rejects_unusable_dynamic_package_with_400() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", WORKSPACE);

    let response = app
        .oneshot(install_request(&token, DYNAMIC_NO_BINARY_MANIFEST))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unexpected: {body}");
    assert_eq!(
        body["success"],
        json!(false),
        "unusable package reported as installed: {body}"
    );
    assert!(
        body["error"].as_str().unwrap_or_default().contains("cannot execute"),
        "unexpected error: {body}"
    );
}

#[tokio::test]
async fn non_admin_cannot_install_or_run_acceptance() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace(NON_ADMIN_USER, "tenant-1", WORKSPACE);

    let response = app
        .clone()
        .oneshot(install_request(&token, DEMO_MANIFEST))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "unexpected: {body}");
    assert_eq!(body["success"], json!(false));

    let response = app
        .oneshot(json_request(
            "POST",
            "/api/v1/drivers/demo-declarative/acceptance",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "unexpected: {body}");
}

#[tokio::test]
async fn declaration_routes_reject_empty_workspace() {
    let (app, _pool, _data) = setup_with_access().await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", "");

    for request in [
        install_request(&token, DEMO_MANIFEST),
        json_request("GET", "/api/v1/drivers/declarations", &token, None),
        json_request("GET", "/api/v1/drivers/simulated/declaration", &token, None),
        json_request("POST", "/api/v1/drivers/simulated/acceptance", &token, None),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        let (status, body) = response_parts(response).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "unexpected: {body}");
        assert!(
            body["error"].as_str().unwrap_or_default().contains("workspace"),
            "unexpected error: {body}"
        );
    }
}

#[tokio::test]
async fn reinstall_keeps_single_declaration_entry() {
    if skip_without_demo_cdylib("reinstall_keeps_single_declaration_entry") {
        return;
    }
    let ws = unique_workspace("ws-reinstall");
    let (app, pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(install_request_with_binary(&token, DEMO_MANIFEST))
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
        .list_active_driver_declarations(&ws)
        .await
        .unwrap();
    let demo: Vec<_> = rows.iter().filter(|r| r.driver_name == "demo-declarative").collect();
    assert_eq!(demo.len(), 1, "reinstall must leave exactly one active row");
}

#[tokio::test]
async fn create_thing_materializes_fixed_driver_declaration() {
    if skip_without_demo_cdylib("create_thing_materializes_fixed_driver_declaration") {
        return;
    }
    let ws = unique_workspace("ws-materialize");
    let (app, _pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    let response = app
        .clone()
        .oneshot(install_request_with_binary(&token, DEMO_MANIFEST))
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
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", WORKSPACE);

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

struct FailingMaterializeHook;

#[async_trait::async_trait]
impl ThingDeclarationHook for FailingMaterializeHook {
    async fn prepare_creation(
        &self,
        _workspace_id: &str,
        _driver_name: &str,
        _options: Option<&str>,
    ) -> Result<CreationMode, String> {
        Ok(CreationMode::Declarative)
    }

    async fn materialize(&self, _workspace_id: &str, _thing_id: &str) -> Result<(), String> {
        Err("describe exploded".into())
    }
}

#[tokio::test]
async fn materialize_failure_rolls_back_the_created_thing() {
    let (_app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WORKSPACE).await;
    let service = ThingService::new(pool.clone());
    service.set_declaration_hook(Arc::new(FailingMaterializeHook));
    let request = CreateThingRequest {
        name: "rollback-thing".into(),
        thing_type: Some("device".into()),
        category: None,
        description: None,
        parent_id: None,
        template_id: None,
        protocol_type: None,
        driver_name: Some("demo-declarative".into()),
        driver_options: None,
        tags: None,
        workspace_id: Some(WORKSPACE.into()),
    };

    let err = service
        .create_thing(&request, Some(WORKSPACE))
        .await
        .expect_err("materialization must fail");
    assert!(err.to_string().contains("describe exploded"), "{err}");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE name='rollback-thing'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "failed materialization must not leave an orphan thing");
}

#[tokio::test]
async fn mcp_device_services_carry_the_declaration_hook() {
    let (mut app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WORKSPACE).await;
    let data = tempfile::tempdir().expect("temp data dir");
    app_state.set_access(access_service(app_state.db.clone(), data.path().to_path_buf()));

    let mcp = crate::domains::mcp::McpState::from_ref(&app_state);
    assert!(
        mcp.tenant_device_service_str(WORKSPACE).has_declaration_hook(),
        "MCP string-workspace path bypasses declaration materialization"
    );
    assert!(
        mcp.tenant_device_service(&Some(WORKSPACE.to_string()))
            .has_declaration_hook(),
        "MCP optional-workspace path bypasses declaration materialization"
    );
}
