//! SceneInstantiator 集成测试（真实 SQLite）。
//!
//! 覆盖：单事务整树落库、dry-run 只读、失败整体回滚、名称冲突后缀、
//! 配额拒绝、非组合模板拒绝、并发同名兜底。

use std::collections::HashMap;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_storage::Db;
use tower::ServiceExt;

use crate::domains::marketplace::error::MarketplaceError;
use crate::domains::marketplace::scene_instantiator::{InstantiateParams, SceneInstantiator};
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const TENANT: &str = "tenant-scene";
const WS: &str = "ws-scene";

/// 场景包 device_info JSON（smart_campus 类）：园区 → 楼 → 层 → 温湿度传感器。
/// 根带 knowledge + resource；传感器节点带引用 temperature 属性的告警规则。
const SCENE_PACK_JSON: &str = r#"{
    "name": "smart_campus",
    "display_name": {"zh": "智慧园区"},
    "category": "scenes",
    "thing_category": "campus",
    "parameters": [
        {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 10, "display_name": {}},
        {"name": "floor_count", "type": "int", "default": 2, "min": 1, "max": 15, "display_name": {}}
    ],
    "device_info": {"default_name_pattern": "{scene_name}"},
    "default_knowledge": "园区知识库内容",
    "resources": [{"name": "园区平面图", "type": "image", "uri": "file://campus-map.png"}],
    "children": [
        {"key": "building", "category": "building", "count_param": "building_count",
         "device_info": {"default_name_pattern": "{index}号楼"},
         "children": [
             {"key": "floor", "category": "floor", "count_param": "floor_count",
              "device_info": {"default_name_pattern": "{index}F"},
              "children": [
                  {"key": "sensor", "template_ref": "temperature_humidity_sensor",
                   "device_info": {"default_name_pattern": "温湿度传感器{index}"},
                   "alarm_rules": [
                       {"name": "高温告警", "rule_type": "threshold",
                        "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
                        "alarm_level": "warning", "property_ref": "temperature"}
                   ]}
              ]}
         ]}
    ]
}"#;

/// 传感器设备模板列内容（temperature_humidity_sensor 类）。
const SENSOR_PROPERTIES: &str = r#"[
    {"name": "temperature", "display_name": {"zh": "温度"}, "description": null,
     "data_type": "float", "unit": "°C", "min_value": null, "max_value": null,
     "default_value": null, "is_read_only": true, "is_required": true, "validation_rules": null},
    {"name": "humidity", "display_name": {"zh": "湿度"}, "description": null,
     "data_type": "float", "unit": "%", "min_value": null, "max_value": null,
     "default_value": null, "is_read_only": true, "is_required": true, "validation_rules": null}
]"#;

const SENSOR_ACTIONS: &str = r#"[
    {"name": "reboot", "display_name": {"zh": "重启"}, "description": null,
     "parameters": null, "parameter_schema": null, "is_required": false}
]"#;

async fn setup_db() -> (SqlitePool, Db) {
    let (_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, TENANT, WS).await;
    let db = Db::new(pool.clone());
    (pool, db)
}

async fn seed_category(pool: &SqlitePool, name: &str) {
    sqlx::query("INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES (?, '{\"zh\":\"分类\"}', '2026-01-01 00:00:00')")
        .bind(name)
        .execute(pool)
        .await
        .expect("seed template category");
}

/// 向 thing_templates 插入一行模板，返回模板 id。
async fn seed_template(
    pool: &SqlitePool,
    name: &str,
    category: &str,
    device_info: &str,
    properties: &str,
    actions: &str,
) -> String {
    seed_category(pool, category).await;
    let id = format!("tpl-{name}");
    sqlx::query(
        "INSERT INTO thing_templates (id, name, display_name, version, category, \
         tags, device_info, properties, actions, events, is_builtin, is_active, workspace_id, created_at, updated_at) \
         VALUES (?, ?, '{\"zh\":\"模板\"}', '1.0.0', ?, '[]', ?, ?, ?, '[]', 0, 1, ?, \
         '2026-01-01 00:00:00', '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .bind(name)
    .bind(category)
    .bind(device_info)
    .bind(properties)
    .bind(actions)
    .bind(WS)
    .execute(pool)
    .await
    .expect("seed thing template");
    id
}

/// 播种标准场景包 + 传感器设备模板，返回场景包模板 id。
async fn seed_standard_templates(pool: &SqlitePool) -> String {
    seed_template(
        pool,
        "temperature_humidity_sensor",
        "sensors",
        r#"{"default_name_pattern": "th_{index}", "required_fields": []}"#,
        SENSOR_PROPERTIES,
        SENSOR_ACTIONS,
    )
    .await;
    seed_template(pool, "smart_campus", "scenes", SCENE_PACK_JSON, "[]", "[]").await
}

fn params(building_count: i64, floor_count: i64, dry_run: bool) -> InstantiateParams {
    InstantiateParams {
        scene_name: "测试园区".to_string(),
        parent_id: None,
        parameter_values: HashMap::from([
            ("building_count".to_string(), building_count),
            ("floor_count".to_string(), floor_count),
        ]),
        dry_run,
    }
}

async fn thing_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ?")
        .bind(WS)
        .fetch_one(pool)
        .await
        .expect("count things")
}

// building=2, floor=2 时：1 园区 + 2 楼 + 4 层 + 4 传感器 = 11
#[tokio::test]
async fn instantiate_creates_full_tree_in_one_tx() {
    let (pool, db) = setup_db().await;
    let scene_id = seed_standard_templates(&pool).await;

    let outcome = SceneInstantiator::instantiate(&db, WS, &scene_id, &params(2, 2, false))
        .await
        .expect("instantiate should succeed");

    assert_eq!(outcome.node_count, 11);
    let root_id = outcome.root_thing_id.clone().expect("root thing id");
    assert!(outcome.tree_preview.contains("测试园区"));

    // 行数 == node_count，且所有节点 template_id = 场景包 id
    assert_eq!(thing_count(&pool).await, 11);
    let with_template: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ? AND template_id = ?")
            .bind(WS)
            .bind(&scene_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(with_template, 11);

    // 根节点：名字 = scene_name，thing_type 由展开结果映射（campus → space）
    let (root_name, root_type, root_linked): (String, String, Option<String>) =
        sqlx::query_as("SELECT name, thing_type, linked_data FROM things WHERE id = ?")
            .bind(&root_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(root_name, "测试园区");
    assert_eq!(root_type, "space");
    // linked_data 顶层键合并：knowledge 必须落库
    let linked = root_linked.expect("root linked_data");
    assert!(linked.contains("knowledge"), "linked_data missing knowledge: {linked}");
    assert!(linked.contains("园区知识库内容"));

    // thing_type 映射：2 楼 building、4 层 space、4 传感器 device
    let buildings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ? AND thing_type = 'building'")
            .bind(WS)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(buildings, 2);
    let devices: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ? AND thing_type = 'device'")
            .bind(WS)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(devices, 4);

    // 层级：每层的父是楼，每个传感器的父是层
    let floor_parent_ok: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM things f JOIN things b ON f.parent_id = b.id \
         WHERE f.workspace_id = ? AND f.category = 'floor' AND b.category = 'building'",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(floor_parent_ok, 4);
    let sensor_parent_ok: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM things s JOIN things f ON s.parent_id = f.id \
         WHERE s.workspace_id = ? AND s.thing_type = 'device' AND f.category = 'floor'",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sensor_parent_ok, 4);

    // 属性/命令已建：4 传感器 × 2 属性 + 1 命令
    let props: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties p JOIN things t ON p.thing_id = t.id WHERE t.workspace_id = ?",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(props, 8);
    let temp_props: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties p JOIN things t ON p.thing_id = t.id \
         WHERE t.workspace_id = ? AND p.name = 'temperature'",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(temp_props, 4);
    let cmds: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_actions c JOIN things t ON c.thing_id = t.id WHERE t.workspace_id = ?",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cmds, 4);

    // 资源：根的平面图
    let resources: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources WHERE thing_id = ?")
        .bind(&root_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(resources, 1);

    // 告警规则：4 条，且 property_ref 已解析为真实 property_id
    let rules: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_alarm_rules r JOIN things t ON r.thing_id = t.id WHERE t.workspace_id = ?",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rules, 4);
    let rules_with_prop: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_alarm_rules r JOIN things t ON r.thing_id = t.id \
         WHERE t.workspace_id = ? AND r.property_id IS NOT NULL",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rules_with_prop, 4);
}

#[tokio::test]
async fn instantiate_dry_run_writes_nothing() {
    let (pool, db) = setup_db().await;
    let scene_id = seed_standard_templates(&pool).await;

    let outcome = SceneInstantiator::instantiate(&db, WS, &scene_id, &params(2, 2, true))
        .await
        .expect("dry-run should succeed");

    assert_eq!(outcome.node_count, 11);
    assert!(outcome.root_thing_id.is_none());
    assert!(outcome.tree_preview.contains("测试园区"));
    assert!(outcome.tree_preview.contains("1号楼"));
    assert_eq!(thing_count(&pool).await, 0);
}

#[tokio::test]
async fn instantiate_rolls_back_on_mid_failure() {
    let (pool, db) = setup_db().await;
    // 楼节点带无法反序列化为 AlarmCondition 的告警条件 → 落库中途失败
    let bad_scene = r#"{
        "name": "bad_campus",
        "display_name": {"zh": "坏园区"},
        "category": "scenes",
        "thing_category": "campus",
        "device_info": {"default_name_pattern": "{scene_name}"},
        "resources": [{"name": "图", "type": "image", "uri": "file://x.png"}],
        "children": [
            {"key": "building", "category": "building",
             "device_info": {"default_name_pattern": "{index}号楼"},
             "alarm_rules": [
                 {"name": "坏规则", "rule_type": "threshold", "condition": {"type": "not_a_condition"}}
             ]}
        ]
    }"#;
    let scene_id = seed_template(&pool, "bad_campus", "scenes", bad_scene, "[]", "[]").await;

    let err = SceneInstantiator::instantiate(&db, WS, &scene_id, &params(1, 1, false))
        .await
        .expect_err("bad alarm condition must fail");
    assert!(
        matches!(err, MarketplaceError::Validation(_)),
        "expected Validation error, got: {err:?}"
    );

    // 整体回滚：无半棵树（根+楼已建也一并撤销），子表同样无残留
    assert_eq!(thing_count(&pool).await, 0);
    let resources: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources WHERE workspace_id = ?")
        .bind(WS)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(resources, 0);
    let rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM thing_alarm_rules WHERE workspace_id = ?")
        .bind(WS)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rules, 0);
}

#[tokio::test]
async fn instantiate_resolves_name_conflicts_with_suffix() {
    let (pool, db) = setup_db().await;
    let scene_id = seed_standard_templates(&pool).await;
    // 预占 "1号楼"
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('pre-1', '1号楼', ?, 'building')")
        .bind(WS)
        .execute(&pool)
        .await
        .expect("pre-occupy name");

    let outcome = SceneInstantiator::instantiate(&db, WS, &scene_id, &params(1, 1, false))
        .await
        .expect("instantiate should succeed with suffix");

    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM things WHERE workspace_id = ? AND name = '1号楼-2'")
        .bind(WS)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(exists.is_some(), "expected suffixed name 1号楼-2");
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.contains("1号楼") && w.contains("1号楼-2")),
        "expected conflict warning, got: {:?}",
        outcome.warnings
    );
}

#[tokio::test]
async fn instantiate_rejects_over_quota() {
    let (pool, db) = setup_db().await;
    let scene_id = seed_standard_templates(&pool).await;
    sqlx::query("UPDATE subscription_plans SET thing_limit = 5 WHERE id = 'plan_free'")
        .execute(&pool)
        .await
        .expect("set thing limit");

    let err = SceneInstantiator::instantiate(&db, WS, &scene_id, &params(2, 2, false))
        .await
        .expect_err("over quota must fail");
    assert!(
        matches!(err, MarketplaceError::Validation(_)),
        "expected Validation error, got: {err:?}"
    );
    assert_eq!(thing_count(&pool).await, 0);
}

#[tokio::test]
async fn instantiate_rejects_non_composition_template() {
    let (pool, db) = setup_db().await;
    // entity 模板：device_info 为 ThingInfo JSON（无 children）
    let entity_id = seed_template(
        &pool,
        "plain_sensor",
        "sensors",
        r#"{"default_name_pattern": "s_{index}", "required_fields": []}"#,
        SENSOR_PROPERTIES,
        "[]",
    )
    .await;

    let err = SceneInstantiator::instantiate(&db, WS, &entity_id, &params(1, 1, false))
        .await
        .expect_err("non-composition template must be rejected");
    assert!(
        matches!(err, MarketplaceError::InvalidConfig(_)),
        "expected InvalidConfig error, got: {err:?}"
    );
    assert_eq!(thing_count(&pool).await, 0);
}

#[tokio::test]
async fn instantiate_concurrent_same_name_gets_suffix_not_500() {
    let (pool, db) = setup_db().await;
    let scene_id = seed_standard_templates(&pool).await;

    let p1 = params(1, 1, false);
    let p2 = params(1, 1, false);
    let (r1, r2) = tokio::join!(
        SceneInstantiator::instantiate(&db, WS, &scene_id, &p1),
        SceneInstantiator::instantiate(&db, WS, &scene_id, &p2),
    );

    let o1 = r1.expect("first instantiation must succeed");
    let o2 = r2.expect("second instantiation must succeed (suffix or retry, never 500)");
    assert_eq!(o1.node_count, 4);
    assert_eq!(o2.node_count, 4);

    // 两棵树 8 节点全部落库；两个根名一正一后缀
    assert_eq!(thing_count(&pool).await, 8);
    let roots: Vec<(String,)> = sqlx::query_as("SELECT name FROM things WHERE workspace_id = ? AND parent_id IS NULL")
        .bind(WS)
        .fetch_all(&pool)
        .await
        .unwrap();
    let names: std::collections::HashSet<&str> = roots.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(names.len(), 2, "expected two distinct root names, got: {names:?}");
    assert!(names.contains("测试园区"));
    assert!(names.contains("测试园区-2"));
}

/// C1 回归：用真实 seed_system 播种的内置 smart_campus 必须能端到端实例化。
/// dry-run 不解析 condition，只有真实落库路径触发 AlarmCondition 反序列化——
/// 内置模板曾携带非法 change condition 导致提交必炸，此用例防回归。
#[tokio::test]
async fn instantiate_builtin_smart_campus_from_real_seed() {
    let (_state, pool) = setup_test_app_with_pool().await;
    let db = Db::new(pool.clone());
    tinyiothub_storage::seed::seed_system(&db).await.expect("seed system");
    seed_test_workspace(&pool, TENANT, WS).await;

    // building=1, floor=1：1 园区 + 1 楼 + 1 层 + 2 传感器 = 5
    let outcome = SceneInstantiator::instantiate(&db, WS, "builtin_smart_campus", &params(1, 1, false))
        .await
        .expect("builtin smart_campus must instantiate from real seed");

    assert_eq!(outcome.node_count, 5);
    assert!(outcome.root_thing_id.is_some());
    assert_eq!(thing_count(&pool).await, 5);

    // 根节点「能耗异常」+ 2 个传感器「高温告警」= 3 条规则全部落库
    let rules: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_alarm_rules r JOIN things t ON r.thing_id = t.id WHERE t.workspace_id = ?",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rules, 3);
    // 根节点的 change 规则（能耗异常，无 property_ref）
    let root_rules: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_alarm_rules WHERE thing_id = ? AND rule_type = 'change'",
    )
    .bind(outcome.root_thing_id.as_deref().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(root_rules, 1);
}

// ──────────────────────────────────────────────────────────────
// Marketplace API（列表 is_composition / 详情 / instantiate 端点）
// ──────────────────────────────────────────────────────────────

async fn setup_app() -> (axum::Router, SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, TENANT, WS).await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool)
}

fn api_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

#[tokio::test]
async fn api_instantiate_happy_path() {
    let (app, pool) = setup_app().await;
    let scene_id = seed_standard_templates(&pool).await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "POST",
            &format!("/api/v1/marketplace/thing-templates/{scene_id}/instantiate"),
            &token,
            Some(json!({
                "sceneName": "测试园区",
                "parameterValues": {"building_count": 2, "floor_count": 2}
            })),
        ))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got: {body}");
    assert_eq!(body["code"], 0);
    let result = &body["result"];
    assert_eq!(result["nodeCount"], 11);
    assert!(result["rootThingId"].is_string(), "rootThingId missing: {result}");
    let preview = result["treePreview"].as_str().unwrap_or_default();
    assert!(preview.contains("测试园区 (campus)"), "treePreview: {preview}");
    assert_eq!(thing_count(&pool).await, 11);
}

#[tokio::test]
async fn api_instantiate_dry_run() {
    let (app, pool) = setup_app().await;
    let scene_id = seed_standard_templates(&pool).await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "POST",
            &format!("/api/v1/marketplace/thing-templates/{scene_id}/instantiate"),
            &token,
            Some(json!({
                "sceneName": "测试园区",
                "parameterValues": {"building_count": 2, "floor_count": 2},
                "dryRun": true
            })),
        ))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got: {body}");
    let result = &body["result"];
    assert_eq!(result["nodeCount"], 11);
    assert!(
        result["rootThingId"].is_null(),
        "dry-run must not return rootThingId: {result}"
    );
    assert_eq!(thing_count(&pool).await, 0, "dry-run must not write");
}

#[tokio::test]
async fn api_list_marks_composition() {
    let (app, pool) = setup_app().await;
    let scene_id = seed_standard_templates(&pool).await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    // 全量列表：场景包项 isComposition=true, parameterCount=2；设备模板为 false
    let response = app
        .clone()
        .oneshot(api_request("GET", "/api/v1/marketplace/thing-templates", &token, None))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got: {body}");
    let items = body["result"]["data"].as_array().expect("data array");
    let scene = items.iter().find(|i| i["id"] == scene_id).expect("scene pack in list");
    assert_eq!(scene["isComposition"], true);
    assert_eq!(scene["parameterCount"], 2);
    let sensor = items
        .iter()
        .find(|i| i["id"] == "tpl-temperature_humidity_sensor")
        .expect("sensor template in list");
    assert_eq!(sensor["isComposition"], false);
    assert_eq!(sensor["parameterCount"], 0);

    // ?composition=true 只返回场景包
    let response = app
        .oneshot(api_request(
            "GET",
            "/api/v1/marketplace/thing-templates?composition=true",
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got: {body}");
    let items = body["result"]["data"].as_array().expect("data array");
    assert!(!items.is_empty(), "composition filter must return scene pack");
    assert!(
        items.iter().all(|i| i["isComposition"] == true),
        "composition=true must only return scene packs: {items:?}"
    );
    assert!(items.iter().any(|i| i["id"] == scene_id));
}

#[tokio::test]
async fn api_detail_returns_parameters() {
    let (app, pool) = setup_app().await;
    let scene_id = seed_standard_templates(&pool).await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "GET",
            &format!("/api/v1/marketplace/thing-templates/{scene_id}"),
            &token,
            None,
        ))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got: {body}");
    let result = &body["result"];
    assert_eq!(result["isComposition"], true);
    let params = result["parameters"].as_array().expect("parameters array");
    let building = params
        .iter()
        .find(|p| p["name"] == "building_count")
        .expect("building_count parameter");
    assert_eq!(building["min"], 1);
    assert_eq!(building["max"], 10);
    assert_eq!(building["default"], 2);
    assert_eq!(result["structureSummary"]["parameterCount"], 2);
    // 园区 → 楼 → 层 → 传感器 = 4 层
    assert_eq!(result["structureSummary"]["maxDepth"], 4);
}

#[tokio::test]
async fn api_instantiate_entity_template_400() {
    let (app, pool) = setup_app().await;
    let entity_id = seed_template(
        &pool,
        "plain_sensor_api",
        "sensors",
        r#"{"default_name_pattern": "s_{index}", "required_fields": []}"#,
        SENSOR_PROPERTIES,
        "[]",
    )
    .await;
    let token = create_test_token_with_workspace("user-1", TENANT, WS);

    let response = app
        .oneshot(api_request(
            "POST",
            &format!("/api/v1/marketplace/thing-templates/{entity_id}/instantiate"),
            &token,
            Some(json!({"sceneName": "x"})),
        ))
        .await
        .unwrap();

    let (status, body) = response_parts(response).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "entity template instantiate must be 400, got: {body}"
    );
    assert_eq!(thing_count(&pool).await, 0);
}
