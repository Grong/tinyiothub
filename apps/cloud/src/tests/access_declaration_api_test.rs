//! E4a access declaration API tests: driver install, declaration listing,
//! authorization, workspace scoping and thing-creation materialization.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRef},
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

pub(crate) fn access_service(db: Arc<tinyiothub_storage::Db>, data_dir: PathBuf) -> Arc<AccessService> {
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
        Arc::new(tinyiothub_runtime::access::demand::DemandTracker::new()),
    ))
}

/// Build a test app with the E4a access service wired to an in-memory DB and
/// a temp package store. `user-1` is an admin.
async fn setup_in(workspace: &str) -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    // Mirror the production router's upload limit (router.rs): the default
    // 2 MiB body limit rejects a debug Linux cdylib (measured ~13 MiB)
    // before the manifest part is read.
    setup_in_with_limit(workspace, 50 * 1024 * 1024).await
}

async fn setup_in_with_limit(workspace: &str, body_limit: usize) -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
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
        .layer(DefaultBodyLimit::max(body_limit))
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

pub(crate) fn demo_package() -> Option<tempfile::TempDir> {
    let lib = demo_cdylib()?;
    let dir = tempfile::tempdir().ok()?;
    std::fs::write(dir.path().join("driver.manifest.json"), DEMO_MANIFEST).ok()?;
    let ext = lib.extension().and_then(|e| e.to_str()).unwrap_or("so");
    std::fs::copy(&lib, dir.path().join(format!("driver.{ext}"))).ok()?;
    Some(dir)
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
async fn multipart_parse_failure_is_not_reported_as_missing_manifest() {
    let ws = unique_workspace("ws-body-limit");
    let (app, _pool, _data) = setup_in_with_limit(&ws, 256).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    let response = app.oneshot(install_request(&token, DEMO_MANIFEST)).await.unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unexpected: {body}");
    assert_ne!(
        body["error"].as_str().unwrap_or_default(),
        "missing manifest field",
        "a multipart parse failure must not be reported as a missing field: {body}"
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

// ──────────────────────────────────────────────
// P2: UI invoke declaration validation + migration switch + backfill
// ──────────────────────────────────────────────

fn legacy_thing_request(name: &str, ws: &str) -> tinyiothub_core::models::thing::CreateThingRequest {
    tinyiothub_core::models::thing::CreateThingRequest {
        name: name.to_string(),
        display_name: None,
        category: None,
        address: None,
        description: None,
        position: None,
        driver_name: Some("simulator".to_string()),
        device_model: None,
        protocol_type: None,
        factory_name: None,
        linked_data: None,
        driver_options: None,
        parent_id: None,
        linked_gateway: None,
        fingerprint: None,
        template_id: None,
        workspace_id: Some(ws.to_string()),
    }
}

async fn register_action(pool: &sqlx::SqlitePool, thing_id: &str, action: &str) {
    sqlx::query("INSERT INTO thing_actions (id, thing_id, name, display_name) VALUES (?, ?, ?, ?)")
        .bind(format!("act-{thing_id}-{action}"))
        .bind(thing_id)
        .bind(action)
        .bind(action)
        .execute(pool)
        .await
        .expect("register action");
}

async fn invoke_ui(app: &Router, token: &str, thing_id: &str, action: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/things/{thing_id}/actions/{action}/invoke"),
            token,
            Some(json!({"params": {}})),
        ))
        .await
        .unwrap();
    response_parts(response).await
}

#[tokio::test]
async fn ui_invoke_rejects_action_not_declared_by_fixed_driver() {
    let ws = unique_workspace("ws-ui-decl");
    let (app, pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    // Declarative driver (builtin simulator declares no tools) → materialized
    // declaration exists, so any registered action must be rejected.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/things",
            &token,
            Some(json!({"name": "ui-decl-thing", "thingType": "device", "driverName": "simulator"})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {body}");
    let thing_id = body["result"]["id"].as_str().expect("thing id").to_string();
    register_action(&pool, &thing_id, "reboot").await;

    let (status, body) = invoke_ui(&app, &token, &thing_id, "reboot").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "undeclared action must be rejected: {body}"
    );
    assert!(
        body["msg"].as_str().unwrap_or_default().contains("not declared"),
        "rejection must name the undeclared tool: {body}"
    );
}

#[tokio::test]
async fn ui_invoke_allows_undeclared_thing_only_when_enforcement_disabled() {
    let ws = unique_workspace("ws-ui-legacy");
    let (app, pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);

    // Legacy path: insert through Db (no declaration materialization).
    let db = tinyiothub_storage::Db::new(pool.clone());
    let thing = db
        .create_thing(Some(&ws), &legacy_thing_request("legacy-thing", &ws))
        .await
        .expect("legacy thing");
    register_action(&pool, &thing.id, "reboot").await;

    // Default enforcement ON → fail closed.
    let (status, body) = invoke_ui(&app, &token, &thing.id, "reboot").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "missing declaration must be rejected while enforcement is on: {body}"
    );
    assert!(
        body["msg"].as_str().unwrap_or_default().contains("no declaration"),
        "rejection must explain the missing declaration: {body}"
    );

    db.set_workspace_declaration_enforcement(&ws, false)
        .await
        .expect("disable enforcement");

    let (status, body) = invoke_ui(&app, &token, &thing.id, "reboot").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "legacy invoke must be allowed while enforcement is off: {body}"
    );
    assert_eq!(body["result"]["status"], json!("confirmation_required"));
}

#[tokio::test]
async fn rematerialize_backfills_things_without_declarations() {
    let ws = unique_workspace("ws-backfill");
    let (app, pool, _data) = setup_in(&ws).await;
    let token = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);
    let db = tinyiothub_storage::Db::new(pool.clone());

    let thing = db
        .create_thing(Some(&ws), &legacy_thing_request("backfill-thing", &ws))
        .await
        .expect("legacy thing");

    let response = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/drivers/rematerialize", &token, None))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "rematerialize failed: {body}");
    assert_eq!(body["bound"], json!(1), "one legacy thing must be bound: {body}");
    assert_eq!(body["failed"], json!([]));
    let declarations = db.list_device_declarations(&ws).await.unwrap();
    assert!(
        declarations.iter().any(|d| d.thing_id == thing.id),
        "backfill must persist the device declaration"
    );

    // Idempotent: a second default run skips the already-declared thing.
    let response = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/drivers/rematerialize", &token, None))
        .await
        .unwrap();
    let (_, body) = response_parts(response).await;
    assert_eq!(body["bound"], json!(0));
    assert_eq!(body["skipped"], json!(1));

    // all=true rebinds even already-declared things.
    let response = app
        .oneshot(json_request(
            "POST",
            "/api/v1/drivers/rematerialize?all=true",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (_, body) = response_parts(response).await;
    assert_eq!(body["bound"], json!(1));
}

async fn seed_declarative_template(pool: &sqlx::SqlitePool, id: &str, driver: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO template_categories (name, display_name, created_at)
         VALUES ('devices', '{\"zh\":\"设备\"}', '2026-01-01 00:00:00')",
    )
    .execute(pool)
    .await
    .expect("seed template category");
    let properties = r#"[{"name":"tpl_prop","display_name":{"zh":"模板属性"},"data_type":"number","unit":"","is_read_only":false,"is_required":false}]"#;
    let actions = r#"[{"name":"tpl_cmd","display_name":{"zh":"模板命令"},"is_required":false}]"#;
    sqlx::query(
        r#"INSERT INTO thing_templates
           (id, name, display_name, description, version, author, category, manufacturer,
            thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
            events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at)
           VALUES (?, ?, '{"zh":"t"}', NULL, '1.0.0', 'test', 'devices', 'test',
                   'device', NULL, ?, '[]', '{"default_name_pattern":"{name}","required_fields":[]}', ?, ?,
                   '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now'))"#,
    )
    .bind(id)
    .bind(id)
    .bind(driver)
    .bind(properties)
    .bind(actions)
    .execute(pool)
    .await
    .expect("insert template");
}

fn template_input(
    name: &str,
    ws: &str,
    template_id: &str,
) -> crate::domains::thing::template::types::ThingCreationInput {
    crate::domains::thing::template::types::ThingCreationInput {
        name: name.to_string(),
        display_name: None,
        description: None,
        position: None,
        address: None,
        driver_name: None,
        driver_options: None,
        parent_id: None,
        template_id: Some(template_id.to_string()),
        property_values: Default::default(),
        enabled_commands: vec![],
        tenant_id: None,
        workspace_id: Some(ws.to_string()),
    }
}

#[tokio::test]
async fn mcp_template_creation_materializes_declaration_without_template_capabilities() {
    if skip_without_demo_cdylib("mcp_template_creation_materializes_declaration_without_template_capabilities") {
        return;
    }
    let ws = unique_workspace("ws-mcp-template");
    let (_app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", &ws).await;
    seed_declarative_template(&pool, "tpl-decl", "demo-declarative").await;

    let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
    let data = tempfile::tempdir().expect("temp data dir");
    let access = access_service(db.clone(), data.path().to_path_buf());
    let src = demo_package().expect("demo package");
    access.install_dir(&ws, src.path()).await.expect("install demo package");
    let mut svc = crate::domains::driver::legacy::service::ThingService::new(db.clone()).for_workspace(ws.clone());
    svc.set_declaration_hook(Arc::new(crate::domains::driver::declaration::AccessDeclarationHook {
        access,
    }));
    let engine = crate::domains::thing::template::TemplateEngine::new(
        db.clone(),
        Arc::new(crate::domains::thing::template::TemplateValidator::new()),
    );

    let thing = svc
        .create_thing_from_template(
            &engine,
            "tpl-decl",
            &template_input("mcp-template-thing", &ws, "tpl-decl"),
        )
        .await
        .expect("template creation");
    assert_eq!(thing.driver_name.as_deref(), Some("demo-declarative"));

    // Fixed-capability driver: template rows must not pollute the declaration.
    let copied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thing_properties WHERE thing_id = ? AND name = 'tpl_prop'")
            .bind(&thing.id)
            .fetch_one(&pool)
            .await
            .expect("count properties");
    assert_eq!(copied, 0, "declarative driver must not copy template properties");
    let copied_cmd: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thing_actions WHERE thing_id = ? AND name = 'tpl_cmd'")
            .bind(&thing.id)
            .fetch_one(&pool)
            .await
            .expect("count actions");
    assert_eq!(copied_cmd, 0, "declarative driver must not copy template commands");

    let declarations = db.list_device_declarations(&ws).await.expect("list declarations");
    assert!(
        declarations.iter().any(|d| d.thing_id == thing.id),
        "MCP template creation must materialize the declaration"
    );
}

#[tokio::test]
async fn mcp_template_materialize_failure_rolls_back_the_created_thing() {
    let ws = unique_workspace("ws-mcp-template-fail");
    let (_app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", &ws).await;
    seed_declarative_template(&pool, "tpl-decl-fail", "simulator").await;

    let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
    let mut svc = crate::domains::driver::legacy::service::ThingService::new(db.clone()).for_workspace(ws.clone());
    svc.set_declaration_hook(Arc::new(FailingMaterializeHook));
    let engine = crate::domains::thing::template::TemplateEngine::new(
        db.clone(),
        Arc::new(crate::domains::thing::template::TemplateValidator::new()),
    );

    let err = svc
        .create_thing_from_template(
            &engine,
            "tpl-decl-fail",
            &template_input("mcp-template-rollback", &ws, "tpl-decl-fail"),
        )
        .await
        .expect_err("materialization must fail");
    assert!(err.to_string().contains("describe exploded"), "{err}");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE name = 'mcp-template-rollback'")
        .fetch_one(&pool)
        .await
        .expect("count things");
    assert_eq!(count, 0, "failed materialization must roll back the thing");
}

#[tokio::test]
async fn enforcement_switch_round_trip_and_admin_gate() {
    let ws = unique_workspace("ws-enforce");
    let (app, _pool, _data) = setup_in(&ws).await;
    let admin = create_test_token_with_workspace(ADMIN_USER, "tenant-1", &ws);
    let non_admin = create_test_token_with_workspace(NON_ADMIN_USER, "tenant-1", &ws);

    let response = app
        .clone()
        .oneshot(json_request("GET", "/api/v1/drivers/enforcement", &admin, None))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "get enforcement failed: {body}");
    assert_eq!(body["enabled"], json!(true), "default must be fail-closed");

    let response = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/v1/drivers/enforcement",
            &admin,
            Some(json!({"enabled": false})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "set enforcement failed: {body}");
    assert_eq!(body["enabled"], json!(false));

    let response = app
        .clone()
        .oneshot(json_request("GET", "/api/v1/drivers/enforcement", &admin, None))
        .await
        .unwrap();
    let (_, body) = response_parts(response).await;
    assert_eq!(body["enabled"], json!(false));

    let response = app
        .oneshot(json_request("GET", "/api/v1/drivers/enforcement", &non_admin, None))
        .await
        .unwrap();
    let (status, _) = response_parts(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-admin must be rejected");
}
