//! E1 watch 值守规则 API 契约测试（Task 8）。
//!
//! 覆盖：注册→列表→删除生命周期、跨工作区删除 404、fail-loud 400
//! （condition 解析失败 / 引用物不存在或属他区 / 超时超 3600 / 活跃 watch
//! 超 20）。多租户：全部按 workspace 隔离。

use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-watch";
const WS_OTHER: &str = "ws-watch-other";

async fn app() -> (axum::Router, sqlx::SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_test_workspace(&pool, "tenant-1", WS_OTHER).await;
    let api = crate::api::create_router(&app_state);
    (axum::Router::new().nest("/api", api).with_state(app_state), pool)
}

/// 播种一个物（watch 条件引用存在性校验用）。
async fn seed_thing(pool: &sqlx::SqlitePool, workspace_id: &str, thing_id: &str) {
    sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES (?, ?, ?, '2026-10-20', '2026-10-20')")
        .bind(thing_id)
        .bind(thing_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("seed thing");
}

fn request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token));
    match body {
        Some(b) => builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

fn property_condition(thing_id: &str) -> Value {
    json!({"type": "property", "thing_id": thing_id, "property": "温度", "op": "gt", "value": 30.0})
}

async fn create_rule(app: &axum::Router, token: &str, ws: &str, body: Value) -> (axum::http::StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/v1/workspaces/{ws}/watch-rules"),
            token,
            Some(body),
        ))
        .await
        .unwrap();
    response_parts(response).await
}

async fn list_rules(app: &axum::Router, token: &str, ws: &str) -> Value {
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/v1/workspaces/{ws}/watch-rules"),
            token,
            None,
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "列表必须成功: {status} {json}");
    json["result"].clone()
}

async fn delete_rule(app: &axum::Router, token: &str, ws: &str, rule_id: &str) -> Value {
    let response = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/v1/workspaces/{ws}/watch-rules/{rule_id}"),
            token,
            None,
        ))
        .await
        .unwrap();
    let (_status, json) = response_parts(response).await;
    json
}

// Value card
// - 覆盖路径：POST→GET→DELETE→GET 全生命周期；condition JSON 往返不失真；
//   server 字段（id/createdAt）齐备。
// - 真实回归场景：注册后列表查不到（workspace 过滤串区）或删除不落库——
//   值守规则持久源（spec §2.1 watch_rules 表）断裂，重启重织丢失。
// - 失败信号：任一断言即断口。
#[tokio::test]
async fn watch_rule_create_list_delete_lifecycle() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    let token = create_test_token("user-1", "tenant-1");

    let (status, json) = create_rule(
        &app,
        &token,
        WS,
        json!({"condition": property_condition("t1"), "persistent": true, "timeoutSecs": 600}),
    )
    .await;
    assert_eq!(json["code"], 0, "注册必须成功: {status} {json}");
    let rule = &json["result"];
    let rule_id = rule["id"].as_str().expect("server 分配 id").to_string();
    assert_eq!(rule["workspaceId"], WS);
    assert_eq!(rule["condition"], property_condition("t1"), "condition 往返不失真");
    assert_eq!(rule["persistent"], true);
    assert_eq!(rule["timeoutSecs"], 600);
    assert!(rule["createdAt"].as_str().is_some());

    let listed = list_rules(&app, &token, WS).await;
    let rules = listed["rules"].as_array().expect("rules 数组");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["id"], rule_id);

    let deleted = delete_rule(&app, &token, WS, &rule_id).await;
    assert_eq!(deleted["code"], 0, "删除必须成功: {deleted}");

    let listed = list_rules(&app, &token, WS).await;
    assert_eq!(listed["rules"].as_array().unwrap().len(), 0, "删除后列表须为空");
}

/// 多租户隔离：跨工作区删除落空 404（SQL 按 workspace 限定），本工作区规则不受影响。
#[tokio::test]
async fn delete_cross_workspace_returns_404() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = create_rule(&app, &token, WS, json!({"condition": property_condition("t1")})).await;
    assert_eq!(json["code"], 0);
    let rule_id = json["result"]["id"].as_str().unwrap().to_string();

    let deleted = delete_rule(&app, &token, WS_OTHER, &rule_id).await;
    assert_eq!(deleted["code"], 404, "跨工作区删除必须 404: {deleted}");

    let listed = list_rules(&app, &token, WS).await;
    assert_eq!(listed["rules"].as_array().unwrap().len(), 1, "本工作区规则不受影响");

    // 同区删除不存在的 id 同样 404
    let deleted = delete_rule(&app, &token, WS, "wr_ghost").await;
    assert_eq!(deleted["code"], 404, "删除不存在的规则必须 404: {deleted}");
}

/// fail-loud 400：condition serde 解析失败（未知 type / 缺字段）。
#[tokio::test]
async fn invalid_condition_returns_400() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    let token = create_test_token("user-1", "tenant-1");

    for bad in [
        json!({"type": "bogus", "thing_id": "t1"}),
        json!({"type": "property", "thing_id": "t1"}), // 缺 property/op/value
        json!("not-an-object"),
    ] {
        let (_status, json) = create_rule(&app, &token, WS, json!({"condition": bad})).await;
        assert_eq!(json["code"], 400, "非法 condition 必须 400: {json}");
    }
    assert_eq!(
        list_rules(&app, &token, WS).await["rules"].as_array().unwrap().len(),
        0,
        "被拒规则不得落库"
    );
}

/// fail-loud 400：timeout_secs 超上限 3600 / 为 0。
#[tokio::test]
async fn timeout_out_of_range_returns_400() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    let token = create_test_token("user-1", "tenant-1");

    for timeout in [0, 3601] {
        let (_status, json) = create_rule(
            &app,
            &token,
            WS,
            json!({"condition": property_condition("t1"), "timeoutSecs": timeout}),
        )
        .await;
        assert_eq!(json["code"], 400, "timeout={timeout} 必须 400: {json}");
    }
}

/// fail-loud 400：引用物不存在 / 引用他工作区的物（多租户隔离）。
#[tokio::test]
async fn unknown_or_foreign_thing_returns_400() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    seed_thing(&pool, WS_OTHER, "t-foreign").await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = create_rule(&app, &token, WS, json!({"condition": property_condition("ghost")})).await;
    assert_eq!(json["code"], 400, "引用不存在物必须 400: {json}");

    let (_status, json) = create_rule(&app, &token, WS, json!({"condition": property_condition("t-foreign")})).await;
    assert_eq!(json["code"], 400, "引用他区物必须 400: {json}");
}

/// fail-loud 400：每工作区活跃 watch 超 20 拒绝注册（spec §2.2 限额）。
#[tokio::test]
async fn active_watch_limit_20_returns_400() {
    let (app, pool) = app().await;
    seed_thing(&pool, WS, "t1").await;
    let token = create_test_token("user-1", "tenant-1");

    // 直插 20 条占满额度（含他区 1 条，验证限额按工作区计）。
    for i in 0..20 {
        let row = tinyiothub_storage::watch::WatchRuleRow {
            id: format!("r{i}"),
            workspace_id: WS.into(),
            kind: "property".into(),
            condition: property_condition("t1").to_string(),
            persistent: 0,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
        };
        tinyiothub_storage::watch::insert_rule(&pool, &row).await.unwrap();
    }
    let foreign = tinyiothub_storage::watch::WatchRuleRow {
        id: "r-foreign".into(),
        workspace_id: WS_OTHER.into(),
        kind: "property".into(),
        condition: property_condition("t1").to_string(),
        persistent: 0,
        timeout_secs: 600,
        created_at: "2026-10-20T00:00:00Z".into(),
    };
    tinyiothub_storage::watch::insert_rule(&pool, &foreign).await.unwrap();

    let (_status, json) = create_rule(&app, &token, WS, json!({"condition": property_condition("t1")})).await;
    assert_eq!(json["code"], 400, "第 21 条必须 400: {json}");

    // 他区额度独立：WS_OTHER 只有 1 条，仍可注册（需先播种他区物）。
    seed_thing(&pool, WS_OTHER, "t2").await;
    let (_status, json) = create_rule(&app, &token, WS_OTHER, json!({"condition": property_condition("t2")})).await;
    assert_eq!(json["code"], 0, "他区额度独立，不得被本区占满影响: {json}");
}
