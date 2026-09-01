//! export-as-template 反向导出集成测试（真实 SQLite）。
//!
//! 覆盖：round-trip 结构等价（实例化 → 导出 → 重解析展开）、
//! 跨 workspace 404（防 IDOR）、非模式命名保留原名 + warnings、子树 >500 拒绝（400）。

use std::collections::HashMap;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::SqlitePool;
use tinyiothub_storage::Db;
use tinyiothub_storage::scene_template::{SceneTemplateFile, expand};
use tower::ServiceExt;

use crate::domains::marketplace::scene_instantiator::{InstantiateParams, SceneInstantiator};
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const TENANT: &str = "tenant-export";
const WS: &str = "ws-export";
const WS_B: &str = "ws-export-b";

/// smart_floor 场景包：楼层 → 房间 ×N（category=room，纯空间节点）。
const SMART_FLOOR_JSON: &str = r#"{
    "name": "smart_floor",
    "display_name": {"zh": "楼层"},
    "category": "scenes",
    "thing_category": "floor",
    "parameters": [
        {"name": "room_count", "type": "int", "default": 8, "min": 1, "max": 50, "display_name": {}}
    ],
    "device_info": {"default_name_pattern": "{scene_name}"},
    "children": [
        {"key": "room", "category": "room", "count_param": "room_count",
         "device_info": {"default_name_pattern": "{index}室"}}
    ]
}"#;

async fn setup_app() -> (axum::Router, SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, TENANT, WS).await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool)
}

fn api_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

async fn seed_category(pool: &SqlitePool, name: &str) {
    sqlx::query("INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES (?, '{\"zh\":\"分类\"}', '2026-01-01 00:00:00')")
        .bind(name)
        .execute(pool)
        .await
        .expect("seed template category");
}

/// 播种 smart_floor 场景包模板，返回模板 id。
async fn seed_floor_template(pool: &SqlitePool) -> String {
    seed_category(pool, "scenes").await;
    let id = "tpl-smart_floor".to_string();
    sqlx::query(
        "INSERT INTO thing_templates (id, name, display_name, version, category, \
         tags, device_info, properties, actions, events, is_builtin, is_active, workspace_id, created_at, updated_at) \
         VALUES (?, 'smart_floor', '{\"zh\":\"楼层\"}', '1.0.0', 'scenes', '[]', ?, '[]', '[]', '[]', 0, 1, ?, \
         '2026-01-01 00:00:00', '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .bind(SMART_FLOOR_JSON)
    .bind(WS)
    .execute(pool)
    .await
    .expect("seed smart_floor template");
    id
}

/// 实例化 smart_floor（room_count=4）并返回根 thing id。
async fn instantiate_floor(pool: &SqlitePool, template_id: &str) -> String {
    let db = Db::new(pool.clone());
    let params = InstantiateParams {
        scene_name: "测试楼层".to_string(),
        parent_id: None,
        parameter_values: HashMap::from([("room_count".to_string(), 4i64)]),
        dry_run: false,
    };
    let outcome = SceneInstantiator::instantiate(&db, WS, template_id, &params)
        .await
        .expect("instantiate smart_floor");
    assert_eq!(outcome.node_count, 5, "1 楼层 + 4 房间");
    outcome.root_thing_id.expect("root thing id")
}

async fn thing_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ?")
        .bind(WS)
        .fetch_one(pool)
        .await
        .expect("count things")
}

#[tokio::test]
async fn export_round_trip_structure_equivalent() {
    let (app, pool) = setup_app().await;
    let scene_id = seed_floor_template(&pool).await;
    let root_id = instantiate_floor(&pool, &scene_id).await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "POST",
            &format!("/api/v1/things/{root_id}/export-as-template"),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body_bytes.to_vec()).expect("utf8 body");
    assert_eq!(status, StatusCode::OK, "export failed: {text}");
    assert!(
        content_disposition.contains("attachment"),
        "expected Content-Disposition download header, got: {content_disposition}"
    );

    // 命名泛化：4 个 "N室" → 单节点 count=4 + "{index}室"
    let value: Value = serde_json::from_str(&text).expect("template JSON");
    assert_eq!(value["children"][0]["count"], 4, "children: {}", value["children"]);
    let pattern = value["children"][0]["device_info"]["default_name_pattern"]
        .as_str()
        .unwrap_or_default();
    assert!(pattern.contains("{index}室"), "name pattern: {pattern}");

    // 重新解析 + 展开 → 节点数 == 原树
    let template = SceneTemplateFile::from_json(&text).expect("re-parse exported template");
    let expanded = expand(&template, "测试楼层", &HashMap::new(), &HashMap::new(), &HashMap::new())
        .expect("re-expand exported template");
    assert_eq!(expanded.node_count as i64, thing_count(&pool).await);
    let names: Vec<&str> = expanded.nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(
        names.contains(&"1室") && names.contains(&"4室"),
        "expanded names: {names:?}"
    );
}

#[tokio::test]
async fn export_rejects_other_workspace() {
    let (app, pool) = setup_app().await;
    seed_test_workspace(&pool, TENANT, WS_B).await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('exp-orphan', 'A楼', ?, 'building')")
        .bind(WS)
        .execute(&pool)
        .await
        .expect("seed thing in workspace A");

    // workspace B 的调用者导出 workspace A 的 thing → 404（防 IDOR）
    let token_b = create_test_token_with_workspace("user-2", TENANT, WS_B);
    let response = app
        .oneshot(api_request(
            "POST",
            "/api/v1/things/exp-orphan/export-as-template",
            &token_b,
        ))
        .await
        .unwrap();

    let (status, _) = response_parts(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "cross-workspace export must be 404");
}

#[tokio::test]
async fn export_keeps_non_pattern_names_with_warning() {
    let (app, pool) = setup_app().await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type, category) VALUES ('exp-root2', '综合楼', ?, 'building', 'building')")
        .bind(WS)
        .execute(&pool)
        .await
        .expect("seed root");
    for (id, name) in [("exp-c1", "会议室"), ("exp-c2", "储藏室")] {
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type, category, parent_id) VALUES (?, ?, ?, 'space', 'room', 'exp-root2')")
            .bind(id)
            .bind(name)
            .bind(WS)
            .execute(&pool)
            .await
            .expect("seed child");
    }
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "POST",
            "/api/v1/things/exp-root2/export-as-template",
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body_bytes.to_vec()).expect("utf8 body");
    assert_eq!(status, StatusCode::OK, "export failed: {text}");
    let value: Value = serde_json::from_str(&text).expect("template JSON");

    // 不泛化：两个子节点各自保留原名，无 count
    let children = value["children"].as_array().expect("children array");
    assert_eq!(children.len(), 2, "children: {children:?}");
    for child in children {
        assert!(
            child["count"].is_null(),
            "non-pattern child must not have count: {child}"
        );
    }
    let patterns: Vec<&str> = children
        .iter()
        .filter_map(|c| c["device_info"]["default_name_pattern"].as_str())
        .collect();
    assert!(
        patterns.contains(&"会议室") && patterns.contains(&"储藏室"),
        "patterns: {patterns:?}"
    );

    // warnings 非空
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(!warnings.is_empty(), "expected non-empty warnings: {value}");
}

#[tokio::test]
async fn export_rejects_oversized_subtree() {
    let (app, pool) = setup_app().await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('exp-big', '大楼', ?, 'building')")
        .bind(WS)
        .execute(&pool)
        .await
        .expect("seed root");
    // 501 个子节点（+ 根 = 502 > 500）
    sqlx::query(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 501) \
         INSERT INTO things (id, name, workspace_id, thing_type, parent_id) \
         SELECT 'exp-big-' || x, 'n' || x, ?, 'space', 'exp-big' FROM c",
    )
    .bind(WS)
    .execute(&pool)
    .await
    .expect("seed 501 children");
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request("POST", "/api/v1/things/exp-big/export-as-template", &token))
        .await
        .unwrap();

    let (status, _) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "oversized subtree must be 400");
}
