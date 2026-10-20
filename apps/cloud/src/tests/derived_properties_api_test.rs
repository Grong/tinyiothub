//! 属性定义管理 API（spec §四）：POST/PUT/DELETE /things/admin/{id}/properties
//! ——派生校验（语法定位/上限/环/跨 workspace）、GET 派生运行状态注入、
//! PUT value 拒绝派生、UNIQUE 冲突 409，以及 Task 7 评审 ⚠️ 要求的
//! notify_config_changed 全路径（创建/更新 → 索引重建 + 热更新生效）实证。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use tinyiothub_core::models::event::{Event, EventLevel, EventSource, RichContent, ThingEventType};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_runtime::driver::derived_manager::DerivedDriverManager;

use crate::shared::runtime_ports::{DbDerivedPropertyQueries, ThingCacheAdapter};
use crate::state::AppState;
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_json, response_parts, seed_test_workspace,
    setup_test_app_with_pool,
};

const WS: &str = "ws-default-001";

/// 测试应用 + 已播种 workspace（/api/v1/things 的 FK things.workspace_id →
/// workspaces 要求，同 device_handler_tests 夹具）。
async fn setup_app() -> (Router, sqlx::SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    let api_router = crate::api::create_router(&app_state);
    let app = Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool)
}

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };
    builder.body(Body::from(body_str)).unwrap()
}

fn token() -> String {
    create_test_token_with_workspace("user-1", "tenant-1", WS)
}

async fn create_thing(app: &Router, token: &str, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/things",
            token,
            Some(json!({ "name": name })),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "创建物 {name} 失败: {json}");
    json["result"]["id"].as_str().unwrap().to_string()
}

async fn post_property(app: &Router, token: &str, thing_id: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/admin/{thing_id}/properties"),
            token,
            Some(body),
        ))
        .await
        .unwrap();
    response_parts(response).await
}

async fn get_properties(app: &Router, token: &str, thing_id: &str) -> Value {
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/admin/{thing_id}/properties"),
            token,
            None,
        ))
        .await
        .unwrap();
    let json = response_json(response).await;
    assert_eq!(json["code"], 0, "GET properties 失败: {json}");
    json["result"].clone()
}

/// 装配带 DerivedDriverManager 的测试应用（与 ServiceManager 生产装配同构：
/// 真 DB queries + 真 ThingCache + 真 EventBus；DataServer 仅取 value_sink
/// 句柄，不跑 run 循环）。
async fn setup_app_with_manager() -> (Router, AppState, Arc<DerivedDriverManager>) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    let data_server = tinyiothub_runtime::DataServer::new(
        Arc::new(ThingCacheAdapter(app_state.device_cache.clone())),
        app_state.event_bus.clone(),
    );
    let manager = Arc::new(DerivedDriverManager::new(
        Arc::new(DbDerivedPropertyQueries((*app_state.db).clone())),
        Arc::new(ThingCacheAdapter(app_state.device_cache.clone())),
        data_server.value_sink(),
        app_state.event_bus.clone(),
    ));
    manager.start().await.expect("derived manager start");
    app_state.derived_manager = Some(manager.clone());
    let api_router = crate::api::create_router(&app_state);
    let app = Router::new().nest("/api", api_router).with_state(app_state.clone());
    (app, app_state, manager)
}

fn cache_prop(id: &str, thing_id: &str, name: &str, value: Option<&str>) -> ThingProperty {
    let mut p = ThingProperty {
        id: id.to_string(),
        thing_id: thing_id.to_string(),
        name: name.to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    };
    if let Some(v) = value {
        p.set_current_value(v.to_string());
    }
    p
}

fn cache_thing(id: &str, name: &str, props: Vec<ThingProperty>) -> Thing {
    let mut t = Thing {
        id: id.to_string(),
        name: name.to_string(),
        ..Thing::default()
    };
    t.properties = Some(props);
    t
}

/// 确定性轮询派生状态（禁裸 sleep；E5：订阅在 POST 响应返回前已建立）。
async fn wait_status(
    manager: &DerivedDriverManager,
    prop_id: &str,
    want_stale: bool,
) -> tinyiothub_runtime::driver::derived::DerivedStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(s) = manager.derived_status(prop_id)
            && s.stale == want_stale
        {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "超时等待 derived_status stale={want_stale}（{prop_id}）"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ── 创建 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_derived_property_ok() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;

    let (status, json) = post_property(&app, &token, &meter, json!({"name": "用电数", "data_type": "float"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "total_power", "expression": "{电表1.用电数} * 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let created = &json["result"];
    assert_eq!(
        created["source"], "derived",
        "source 必须显式写穿 derived（Task 4 评审）"
    );
    assert_eq!(created["expression"], "{电表1.用电数} * 2");
    assert_eq!(created["data_type"], "number", "派生 data_type 缺省 number");
    assert_eq!(created["is_read_only"], 1);
    let refs = created["refs"].as_str().expect("refs 恒非空 JSON");
    assert!(
        refs.contains(&format!("\"thing_id\":\"{meter}\"")),
        "refs 应含解析后物 id: {refs}"
    );
    assert!(refs.contains("\"property_name\":\"用电数\""), "{refs}");

    let props = get_properties(&app, &token, &owner).await;
    let row = props
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "total_power")
        .unwrap();
    assert_eq!(row["source"], "derived");
}

#[tokio::test]
async fn create_derived_rejects_bad_expression_400_with_position() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let owner = create_thing(&app, &token, "总表").await;

    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "bad", "expression": "{电表1.用电数} 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["code"], 400);
    assert!(json["msg"].as_str().unwrap().contains("位置"), "应含语法定位: {json}");
}

#[tokio::test]
async fn create_derived_rejects_caps_400() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let owner = create_thing(&app, &token, "总表").await;

    // F-C：表达式 > 1000 字符
    let long_expr = "1".repeat(1001);
    let (status, json) = post_property(&app, &token, &owner, json!({"name": "long", "expression": long_expr})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(json["msg"].as_str().unwrap().contains("超长"), "{json}");

    // F-C：> 50 个引用
    let many_refs = (0..51).map(|i| format!("{{M{i}.x}}")).collect::<Vec<_>>().join("+");
    let (status, json) = post_property(&app, &token, &owner, json!({"name": "many", "expression": many_refs})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(json["msg"].as_str().unwrap().contains("引用过多"), "{json}");
}

#[tokio::test]
async fn create_derived_rejects_cycle_400() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let a = create_thing(&app, &token, "A").await;
    let b = create_thing(&app, &token, "B").await;

    let (status, json) = post_property(&app, &token, &a, json!({"name": "total", "data_type": "float"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let a_total = json["result"]["id"].as_str().unwrap().to_string();

    // B.x 派生 {A.total}（既有边 B → A）
    let (status, json) = post_property(&app, &token, &b, json!({"name": "x", "expression": "{A.total}"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    // A.total 改派生 {B.x}（新边 A → B）→ A→B→A 成环
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{a}/properties/{a_total}"),
            &token,
            Some(json!({"expression": "{B.x}"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(json["msg"].as_str().unwrap().contains("引用成环"), "应含环路径: {json}");
}

#[tokio::test]
async fn create_derived_rejects_cross_workspace_404_or_400() {
    let (app, pool) = setup_app().await;
    // 同名物只存在于另一个 workspace（D3：跨 workspace 引用必须拒绝）
    seed_test_workspace(&pool, "tenant-1", "ws-other-001").await;
    let token_ws2 = create_test_token_with_workspace("user-1", "tenant-1", "ws-other-001");
    let meter2 = create_thing(&app, &token_ws2, "电表X").await;
    let (status, json) = post_property(&app, &token_ws2, &meter2, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let token = token();
    let owner = create_thing(&app, &token, "总表").await;
    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "steal", "expression": "{电表X.用电数}"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(json["msg"].as_str().unwrap().contains("物 电表X 不存在"), "{json}");
}

// ── 更新 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_expression_revalidates() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    for name in ["a", "b"] {
        let (status, json) = post_property(&app, &token, &meter, json!({"name": name, "data_type": "float"})).await;
        assert_eq!(status, StatusCode::OK, "{json}");
    }
    let (status, json) = post_property(&app, &token, &owner, json!({"name": "calc", "expression": "{电表1.a}"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    // 无效表达式 → 400，原定义不变
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            Some(json!({"expression": "{幽灵.x}"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(json["msg"].as_str().unwrap().contains("物 幽灵 不存在"), "{json}");

    // 合法新表达式 → 全量重新校验并写穿 refs
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            Some(json!({"expression": "{电表1.b} + 1"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"]["expression"], "{电表1.b} + 1");

    let props = get_properties(&app, &token, &owner).await;
    let row = props.as_array().unwrap().iter().find(|p| p["id"] == prop_id).unwrap();
    assert_eq!(row["expression"], "{电表1.b} + 1");
    let refs = row["refs"].as_str().unwrap();
    assert!(refs.contains("\"property_name\":\"b\""), "GET 应见新 refs: {refs}");
    assert!(!refs.contains("\"property_name\":\"a\""), "{refs}");
}

// ── 删除 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_derived_property() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"], true);

    let props = get_properties(&app, &token, &owner).await;
    assert!(
        !props.as_array().unwrap().iter().any(|p| p["id"] == prop_id),
        "DELETE 后 GET 不应再出现"
    );

    // 再删 → 404
    let response = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (status, _) = response_parts(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── PUT value 拒绝派生（T3） ─────────────────────────────────────────

#[tokio::test]
async fn put_value_rejects_derived() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}/value"),
            &token,
            Some(json!({"value": "5"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert!(
        json["msg"].as_str().unwrap().contains("派生属性不支持手动写值"),
        "{json}"
    );
}

// ── UNIQUE 冲突 ───────────────────────────────────────────────────────

#[tokio::test]
async fn duplicate_name_conflict_409() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let owner = create_thing(&app, &token, "总表").await;

    let (status, json) = post_property(&app, &token, &owner, json!({"name": "dup"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = post_property(&app, &token, &owner, json!({"name": "dup"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(json["code"], 409);
}

// ── GET 派生运行状态注入（Manager 真实装配） ─────────────────────────

#[tokio::test]
async fn get_properties_includes_derived_status() {
    let (app, app_state, _manager) = setup_app_with_manager().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);

    // 缓存：引用属性无当前值 → 首值补偿后标 stale「等待数据」
    app_state.device_cache.insert(cache_thing(
        &meter,
        "电表1",
        vec![cache_prop(&format!("prop-{meter}"), &meter, "用电数", None)],
    ));
    app_state.device_cache.insert(cache_thing(&owner, "总表", vec![]));

    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    let deadline = Instant::now() + Duration::from_secs(5);
    let row = loop {
        let props = get_properties(&app, &token, &owner).await;
        let row = props
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == prop_id)
            .unwrap()
            .clone();
        if row["derived_status"]["stale"] == true {
            break row;
        }
        assert!(Instant::now() < deadline, "超时等待 GET 注入 derived_status.stale=true");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let reason = row["derived_status"]["stale_reason"].as_str().unwrap();
    assert!(reason.contains("等待数据"), "{reason}");
    assert!(row["derived_status"]["last_eval_at"].is_string(), "{row}");
}

// ── Task 7 评审 ⚠️：notify_config_changed 全路径 ─────────────────────
// 创建 → 索引重建 + 首值生效（stale=false 说明新表达式真实求值）；
// PUT 换表达式 → 热更新生效（事件触发重算后按新表达式求值：新引用缺值 →
// stale 原因点名新属性——若热更新未生效，重算仍走旧表达式则保持 stale=false）。

#[tokio::test]
async fn notify_config_changed_full_path_hot_update_takes_effect() {
    let (app, app_state, manager) = setup_app_with_manager().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "备用"})).await;
    assert_eq!(status, StatusCode::OK);

    // 缓存：用电数=21；备用 故意不进缓存（求值时缺值）
    app_state.device_cache.insert(cache_thing(
        &meter,
        "电表1",
        vec![cache_prop(&format!("prop-{meter}"), &meter, "用电数", Some("21"))],
    ));
    app_state.device_cache.insert(cache_thing(&owner, "总表", vec![]));

    // ① 创建 → notify_config_changed → 索引重建 + 首值补偿求值
    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数} * 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    let s = wait_status(&manager, &prop_id, false).await;
    assert!(s.last_eval_at.is_some(), "创建后表达式应已真实求值");

    // ② PUT 换表达式 → notify → 驱动 defs 热更新
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            Some(json!({"expression": "{电表1.用电数} + {电表1.备用}"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    // ③ 属性变更事件触发重算——若热更新生效，新表达式因 备用 缺值而 stale
    let event = Event::new_device_event(
        ThingEventType::PropertyChange,
        EventLevel::Info,
        EventSource::device_property(meter.clone(), format!("prop-{meter}"), "test".to_string()),
        RichContent::new_text("t".to_string(), "c".to_string()),
    )
    .expect("构造测试事件");
    app_state.event_bus.publish(event).await.expect("发布属性变更事件");

    let s = wait_status(&manager, &prop_id, true).await;
    let reason = s.stale_reason.unwrap_or_default();
    // I-2 后求值全程走 thing_id，stale 原因点名 id 级引用
    assert!(
        reason.contains("备用") && reason.contains(&meter),
        "热更新后应按新表达式求值（缺 备用 值，引用以 thing_id 标识）: {reason}"
    );
}

// ── Task 8 评审 Important：MCP 物管理面不得绕过 notify 钩子 ──────────
// MCP delete_thing 删除被派生引用的物 → notify_thing_deleted 同步标 stale
// （"引用缺失"）——若钩子缺失，派生会继续按缓存旧值出数直到进程重启。

#[tokio::test]
async fn mcp_delete_thing_triggers_notify_marks_derived_stale() {
    use axum::extract::FromRef;

    use crate::domains::mcp::McpState;
    use crate::domains::mcp::handlers::{McpAuthContext, with_mcp_context};
    use crate::domains::mcp::tool_registry::ToolHandler;
    use crate::domains::mcp::tools::thing::DeleteThingHandler;

    let (app, app_state, manager) = setup_app_with_manager().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);

    app_state.device_cache.insert(cache_thing(
        &meter,
        "电表1",
        vec![cache_prop(&format!("prop-{meter}"), &meter, "用电数", Some("21"))],
    ));
    app_state.device_cache.insert(cache_thing(&owner, "总表", vec![]));

    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数} * 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();
    wait_status(&manager, &prop_id, false).await;

    // 经 MCP 工具删除被引用的电表（活管理面路径）
    let mcp_state = Arc::new(McpState::from_ref(&app_state));
    let handler = DeleteThingHandler::new(Some(mcp_state));
    let ctx = McpAuthContext::for_jwt(WS.to_string(), "user-1".to_string());
    let result = with_mcp_context(ctx, || handler.execute(json!({"id": meter.clone()}))).await;
    assert!(result.is_ok(), "MCP delete_thing 失败: {result:?}");

    // notify_thing_deleted 是 execute 内同步 await——返回即应已标 stale
    let s = manager.derived_status(&prop_id).expect("删除后状态条目仍在");
    assert!(s.stale, "MCP 删除引用物后派生应标 stale: {s:?}");
    let reason = s.stale_reason.unwrap_or_default();
    assert!(reason.contains("已删除"), "{reason}");
}

// ── 评审 Minor 1：PUT value 先验物归属，跨 workspace 派生 id 不泄漏存在性 ──

#[tokio::test]
async fn put_value_cross_workspace_derived_id_no_leak() {
    let (app, pool) = setup_app().await;
    seed_test_workspace(&pool, "tenant-1", "ws-other-001").await;
    let token_ws2 = create_test_token_with_workspace("user-1", "tenant-1", "ws-other-001");
    let meter2 = create_thing(&app, &token_ws2, "电表X").await;
    let owner2 = create_thing(&app, &token_ws2, "总表X").await;
    let (status, _) = post_property(&app, &token_ws2, &meter2, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, json) = post_property(
        &app,
        &token_ws2,
        &owner2,
        json!({"name": "calc", "expression": "{电表X.用电数}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let calc_id = json["result"]["id"].as_str().unwrap().to_string();

    // ws1 视角：物/属性都存在但不在本 workspace——必须报"不存在"而非 400 派生拒绝
    let token = token();
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner2}/properties/{calc_id}/value"),
            &token,
            Some(json!({"value": "5"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let msg = json["msg"].as_str().unwrap();
    assert!(msg.contains("不存在"), "跨 workspace 应报不存在: {msg}");
    assert!(!msg.contains("派生"), "不得泄漏派生属性存在性: {msg}");
}

// ── 评审 Minor 2：非派生 → 派生转换时 data_type 未给则补 'number' ─────

#[tokio::test]
async fn update_to_derived_defaults_data_type_number() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数"})).await;
    assert_eq!(status, StatusCode::OK);

    // 非派生创建：data_type 缺省 'string'
    let (status, json) = post_property(&app, &token, &owner, json!({"name": "calc"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"]["data_type"], "string");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();

    // PUT 转派生（未显式给 data_type）→ 应补 'number'，与创建路径一致
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{owner}/properties/{prop_id}"),
            &token,
            Some(json!({"expression": "{电表1.用电数} * 2"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"]["source"], "derived");
    assert_eq!(json["result"]["data_type"], "number", "转派生 data_type 应补 number");
    assert_eq!(json["result"]["is_read_only"], 1);
}

// ── 终审 C-1：profile 端点（详情页真实数据源）必须带派生字段 + 活注入运行状态 ──
// R2 钉死形态：此前 load_properties 字段清单缺 source/expression/refs、
// derivedStatus 无处可来——前端派生徽标/stale 灰点在生产永不出现，vitest 恰好
// 绕过真实端点。本测试走真实路由 GET /things/{id}/profile 全程断言。

#[tokio::test]
async fn profile_includes_derived_fields_and_live_status() {
    let (app, app_state, manager) = setup_app_with_manager().await;
    let token = token();
    let meter = create_thing(&app, &token, "电表1").await;
    let owner = create_thing(&app, &token, "总表").await;
    let (status, _) = post_property(&app, &token, &meter, json!({"name": "用电数", "data_type": "float"})).await;
    assert_eq!(status, StatusCode::OK);

    app_state.device_cache.insert(cache_thing(
        &meter,
        "电表1",
        vec![cache_prop(&format!("prop-{meter}"), &meter, "用电数", Some("21"))],
    ));
    app_state.device_cache.insert(cache_thing(&owner, "总表", vec![]));

    let (status, json) = post_property(
        &app,
        &token,
        &owner,
        json!({"name": "calc", "expression": "{电表1.用电数} * 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let prop_id = json["result"]["id"].as_str().unwrap().to_string();
    wait_status(&manager, &prop_id, false).await;

    let get_profile_row = async |app: &Router, prop_id: &str| {
        let response = app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/things/{owner}/profile"),
                &token,
                None,
            ))
            .await
            .unwrap();
        let json = response_json(response).await;
        assert_eq!(json["code"], 0, "GET profile 失败: {json}");
        json["result"]["properties"]
            .as_array()
            .expect("profile.properties 为数组")
            .iter()
            .find(|p| p["id"] == prop_id)
            .unwrap_or_else(|| panic!("profile 中找不到派生属性 {prop_id}: {json}"))
            .clone()
    };

    // ① 派生行必须带 source/expression/refs + derivedStatus（前端徽标/表达式
    //    展开/stale 灰点的全部数据依据）
    let row = get_profile_row(&app, &prop_id).await;
    assert_eq!(row["source"], "derived", "{row}");
    assert_eq!(row["expression"], "{电表1.用电数} * 2", "{row}");
    assert!(row["refs"].as_str().unwrap().contains(&meter), "{row}");
    assert_eq!(row["derivedStatus"]["stale"], false, "{row}");
    assert!(row["derivedStatus"]["lastEvalAt"].is_string(), "{row}");

    // ② derivedStatus 是活注入而非一次性快照：引用值清空 + 事件触发 →
    //    Manager 标 stale → profile 同步反映
    let mut m = app_state.device_cache.get(&meter).expect("电表 cached");
    m.properties.as_mut().unwrap()[0].current_value = None;
    app_state.device_cache.update(m);
    let event = Event::new_device_event(
        ThingEventType::PropertyChange,
        EventLevel::Info,
        EventSource::device_property(meter.clone(), format!("prop-{meter}"), "test".to_string()),
        RichContent::new_text("t".to_string(), "c".to_string()),
    )
    .expect("构造测试事件");
    app_state.event_bus.publish(event).await.expect("发布属性变更事件");
    wait_status(&manager, &prop_id, true).await;

    let row = get_profile_row(&app, &prop_id).await;
    assert_eq!(row["derivedStatus"]["stale"], true, "{row}");
    assert!(
        row["derivedStatus"]["staleReason"]
            .as_str()
            .unwrap()
            .contains("等待数据"),
        "{row}"
    );
}

// ── X4 收尾（Task 3）：I-1 限制取消——带物理驱动（driver_name 非空）的物可配派生 ──
// X4 引擎单一化后 Manager 不注册 wrapper（全 workspace 单订阅直发 sink），
// wrapper 顶撞结构性不存在——派生计算值与驱动采集值可共存同一 Thing（混合模式，
// 共存语义 E2E 见 park_derived_e2e_test
// physical_thing_derived_property_coexists_with_driver_values）。

#[tokio::test]
async fn derived_allowed_on_thing_with_physical_driver() {
    let (app, _pool) = setup_app().await;
    let token = token();
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/things",
            &token,
            Some(json!({ "name": "物理表", "driverName": "modbus" })),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "创建物理驱动物失败: {json}");
    let physical = json["result"]["id"].as_str().unwrap().to_string();

    // 创建路径：POST 派生 → 200（I-1 取消，不再 400）
    let (status, json) = post_property(
        &app,
        &token,
        &physical,
        json!({"name": "calc", "expression": "descendant_count()"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"]["source"], "derived");
    assert_eq!(json["result"]["is_read_only"], 1);

    // 更新路径：普通属性 PUT 转派生 → 200
    let (status, json) = post_property(&app, &token, &physical, json!({"name": "plain"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let pid = json["result"]["id"].as_str().unwrap().to_string();
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/admin/{physical}/properties/{pid}"),
            &token,
            Some(json!({"expression": "descendant_count()"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["result"]["source"], "derived");
}
