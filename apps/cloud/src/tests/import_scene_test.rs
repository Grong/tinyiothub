//! import 场景包闭环集成测试（真实 SQLite）。
//!
//! 覆盖：T8 导出形态的场景包 JSON（含附加 warnings 顶层键）走 import 注册为
//! workspace 组合模板（device_info 存原文 → is_composition）→ 可再实例化（round-trip）；
//! 名称冲突自动重命名；entity 模板 import 回归不变（红线）。

use std::collections::HashMap;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::SqlitePool;
use tinyiothub_storage::Db;
use tower::ServiceExt;

use crate::domains::marketplace::scene_instantiator::{InstantiateParams, SceneInstantiator};
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const TENANT: &str = "tenant-import";
const WS: &str = "ws-import";

/// T8 导出形态的场景包 JSON：parameters/children/resources/dashboard 全量 + 附加 warnings 顶层键。
const EXPORTED_SCENE_JSON: &str = r#"{
    "name": "smart_floor_export",
    "display_name": {"zh": "导出楼层", "en": "Exported Floor"},
    "description": {"zh": "导出再导入闭环"},
    "version": "1.0.0",
    "category": "scenes",
    "thing_category": "floor",
    "parameters": [
        {"name": "room_count", "type": "int", "default": 3, "min": 1, "max": 10, "display_name": {"zh": "房间数"}}
    ],
    "device_info": {"default_name_pattern": "{scene_name}"},
    "properties": [],
    "resources": [{"name": "floor_plan", "type": "image", "uri": "builtin://scenes/smart_floor/floor_plan.png"}],
    "dashboard": {"widgets": []},
    "alarm_rules": [],
    "children": [
        {"key": "room", "category": "room", "count_param": "room_count",
         "device_info": {"default_name_pattern": "{index}室"}}
    ],
    "warnings": ["导出时的 warning：导入应忽略该附加键"]
}"#;

/// 经典 DTDL entity 模板（回归红线 fixture）。
const ENTITY_DTDL_JSON: &str = r#"{
    "@context": "dtmi:dtdl:context;2",
    "@type": "Interface",
    "displayName": "ImportRegressSensor",
    "contents": [
        {"@type": "Telemetry", "name": "temperature", "schema": "double"}
    ]
}"#;

async fn setup_app() -> (axum::Router, SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, TENANT, WS).await;
    // entity import 的 category 硬编码 "imported"（thing_templates.category 有 FK），
    // 与 scenes 一并播种（system.sql 未含 imported —— 见 task-9 报告 concerns）
    for cat in ["scenes", "imported"] {
        sqlx::query("INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES (?, '{\"zh\":\"分类\"}', '2026-01-01 00:00:00')")
            .bind(cat)
            .execute(&pool)
            .await
            .expect("seed template category");
    }
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool)
}

fn post_json(uri: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn import_template(app: axum::Router, token: &str, body: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(post_json("/api/v1/things/import/dtdl", token, body))
        .await
        .unwrap();
    response_parts(response).await
}

async fn template_row(pool: &SqlitePool, id: &str) -> (Option<String>, i64, String, String) {
    sqlx::query_as("SELECT workspace_id, is_builtin, category, device_info FROM thing_templates WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("template row")
}

#[tokio::test]
async fn import_scene_template_round_trips() {
    let (app, pool) = setup_app().await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let (status, body) = import_template(app, &token, EXPORTED_SCENE_JSON).await;
    assert_eq!(status, StatusCode::CREATED, "import scene failed: {body}");
    let template_id = body["result"]["id"].as_str().expect("template id").to_string();
    assert_eq!(body["result"]["name"], "smart_floor_export");

    // thing_templates 新行：当前 workspace、非内置、device_info 原文含 children
    let (workspace_id, is_builtin, category, device_info) = template_row(&pool, &template_id).await;
    assert_eq!(workspace_id.as_deref(), Some(WS));
    assert_eq!(is_builtin, 0);
    assert_eq!(category, "scenes");

    let stored: Value = serde_json::from_str(&device_info).expect("device_info JSON");
    assert!(
        !stored["children"].as_array().map(|a| a.is_empty()).unwrap_or(true),
        "device_info 应含非空 children: {stored}"
    );
    assert_eq!(
        stored["parameters"][0]["name"], "room_count",
        "device_info 应存原文含 parameters"
    );
    assert!(stored.get("dashboard").is_some(), "device_info 应存原文含 dashboard");
    assert_eq!(stored["resources"][0]["name"], "floor_plan");

    // 组合模板判定
    let db = Db::new(pool.clone());
    let template = db
        .find_thing_template_by_id(&template_id, WS)
        .await
        .expect("find template")
        .expect("template exists");
    assert!(template.is_composition(), "应为组合模板");

    // round-trip：导入的模板可再实例化（1 楼层 + 2 房间）
    let params = InstantiateParams {
        scene_name: "回灌楼层".to_string(),
        parent_id: None,
        parameter_values: HashMap::from([("room_count".to_string(), 2i64)]),
        dry_run: false,
    };
    let outcome = SceneInstantiator::instantiate(&db, WS, &template_id, &params)
        .await
        .expect("re-instantiate imported scene template");
    assert_eq!(outcome.node_count, 3, "1 楼层 + 2 房间");
}

#[tokio::test]
async fn import_scene_template_name_conflict_renames() {
    let (app, pool) = setup_app().await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let (status, body) = import_template(app.clone(), &token, EXPORTED_SCENE_JSON).await;
    assert_eq!(status, StatusCode::CREATED, "first import: {body}");
    assert_eq!(body["result"]["name"], "smart_floor_export");

    // 同名再导入 → 自动重命名而非 409
    let (status, body) = import_template(app, &token, EXPORTED_SCENE_JSON).await;
    assert_eq!(status, StatusCode::CREATED, "second import: {body}");
    let renamed = body["result"]["name"].as_str().unwrap_or_default();
    assert_ne!(renamed, "smart_floor_export", "冲突应重命名");
    assert!(
        renamed.starts_with("smart_floor_export"),
        "重命名应保留原名前缀: {renamed}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_templates WHERE workspace_id = ? AND name LIKE 'smart_floor_export%'",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .expect("count renamed");
    assert_eq!(count, 2);
}

#[tokio::test]
async fn import_entity_template_unchanged() {
    let (app, pool) = setup_app().await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let (status, body) = import_template(app.clone(), &token, ENTITY_DTDL_JSON).await;
    assert_eq!(status, StatusCode::CREATED, "import entity failed: {body}");
    let template_id = body["result"]["id"].as_str().expect("template id").to_string();
    assert_eq!(body["result"]["name"], "ImportRegressSensor");

    // entity 行为不变：device_info 仍为 "{}"，非组合模板，category 仍为 imported
    let (_ws, is_builtin, category, device_info) = template_row(&pool, &template_id).await;
    assert_eq!(is_builtin, 0);
    assert_eq!(category, "imported");
    assert_eq!(device_info, "{}", "entity 模板 device_info 应保持 {{}}");

    let db = Db::new(pool.clone());
    let template = db
        .find_thing_template_by_id(&template_id, WS)
        .await
        .expect("find template")
        .expect("template exists");
    assert!(!template.is_composition());

    // entity 路径名称冲突仍 409（现有 import 行为不变）
    let (status, _body) = import_template(app, &token, ENTITY_DTDL_JSON).await;
    assert_eq!(status, StatusCode::CONFLICT, "entity 重名应 409");
}
