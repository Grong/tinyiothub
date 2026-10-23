//! 心跳配置/任务 handler 写穿测试（D11-⑤ + 终审 I-1 + D13，T15 读源切换后）。
//!
//! heartbeat_logs_tests 覆盖 /logs 读侧；本文件覆盖 config/tasks 写侧：
//! handler 先写 DB → 更新统一循环共享表（manager）→ notify，以及 get_config
//! 从统一循环共享表读 enabled/interval/last_tick（T15 起唯一读源）。

use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use tower::ServiceExt;

use tinyiothub_agent::runtime::runtime::{AgentRuntime, RuntimeDeps};
use tinyiothub_agent::runtime::snapshot::RestoreSnapshot;
use tinyiothub_agent::runtime::thing_agent::ThingAgentManager;
use tinyiothub_storage::Db;
use tinyiothub_storage::heartbeat::WorkspaceHeartbeatConfig;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_admin_role, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-hb-cfg";

fn empty_snapshot() -> RestoreSnapshot {
    RestoreSnapshot {
        heartbeat: vec![],
        recent_runs: vec![],
        problem_meta: vec![],
        recent_run_meta: Default::default(),
    }
}

/// 接线统一循环 manager 的测试 app（thing_agents=None 时 update_config /
/// update_tasks 一律「心跳服务未启用」，写穿路径无从谈起）。
async fn app_with_manager() -> (axum::Router, sqlx::SqlitePool, std::sync::Arc<ThingAgentManager>) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_admin_role(&pool, "user-1").await;
    let runtime = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
    let manager = runtime.thing_agents().clone();
    app_state.thing_agents = Some(manager.clone());
    app_state.heartbeat_last_ticks = Some(manager.heartbeat_last_ticks());
    let api = crate::api::create_router(&app_state);
    (
        axum::Router::new().nest("/api", api).with_state(app_state),
        pool,
        manager,
    )
}

fn put(uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .unwrap()
}

async fn call(app: &axum::Router, req: Request<Body>) -> Value {
    let response = app.clone().oneshot(req).await.unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "请求必须成功: {status} {json}");
    json["result"].clone()
}

// Value card
// - 覆盖路径：PUT /heartbeat/config 的 D11-⑤ 写序（DB 先写 → 统一循环共享表
//   intervals/enabled 写穿 → notify）+ 部分更新归并（只给一字段时另一字段
//   读当前值不被重置）+ get_config 读统一循环共享表（T15 唯一读源）。
// - 真实回归场景：归并逻辑写成 req.enabled.unwrap_or(false)——只改间隔的
//   请求把 enabled 静默翻回 false，巡检停摆；或写穿顺序颠倒/漏写 enabled
//   表，trigger 判定集与 DB 持久值分叉。
// - 失败信号：get_config 回读与 manager 共享表/DB 行三方不一致即断口。
// - 覆盖理由：manager/trigger 层测试不经过 handler 归并与 DB 写序；既有
//   测试文件没有任何 config handler 用例。
#[tokio::test]
async fn update_config_partial_update_merges_and_writes_through() {
    let (app, pool, manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    let db = Db::new(pool.clone());
    db.save_heartbeat_config(WS, &WorkspaceHeartbeatConfig::validated(true, 30).expect("seed config"))
        .await
        .expect("seed heartbeat config");

    // 只改间隔：enabled 必须保持 true（归并当前值，不被 None 重置）。
    let result = call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"interval_minutes": 45}),
        ),
    )
    .await;
    assert_eq!(result["intervalMinutes"], 45);
    assert_eq!(result["enabled"], true, "部分更新不得重置 enabled");

    // 三方一致：DB 行 / manager 共享表 / get_config 回读。
    let persisted = db
        .load_heartbeat_config(WS)
        .await
        .expect("load")
        .expect("persisted config");
    assert_eq!(persisted.interval_minutes, 45);
    assert!(persisted.enabled);
    assert_eq!(manager.heartbeat_interval_minutes(WS), Some(45), "intervals 共享表写穿");
    assert!(manager.heartbeat_enabled(WS));

    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(cfg["intervalMinutes"], 45);
    assert_eq!(cfg["enabled"], true);

    // 只改开关：interval 必须保持 45；enabled=false 写穿 trigger 判定集。
    let result = call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"enabled": false}),
        ),
    )
    .await;
    assert_eq!(result["enabled"], false);
    assert_eq!(result["intervalMinutes"], 45, "部分更新不得重置 interval");
    assert!(!manager.heartbeat_enabled(WS), "enabled 共享表写穿（终审 I-1）");
    let persisted = db
        .load_heartbeat_config(WS)
        .await
        .expect("load")
        .expect("persisted config");
    assert!(!persisted.enabled);
    assert_eq!(persisted.interval_minutes, 45);

    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(cfg["enabled"], false);
}

// Value card
// - 覆盖路径：PUT /heartbeat/tasks 的 D11-⑤ 写序（replace DB → 回读 →
//   set_heartbeat_tasks 写穿统一循环任务表 → notify）+ get_config 的
//   last_tick 读统一循环 last_ticks 共享表（D13 实时读，T15 唯一读源）。
// - 真实回归场景：写穿漏调 set_heartbeat_tasks——DB 已改但 trigger/prompt
//   仍读旧任务表，下一次 tick 巡检过期任务集；last_tick 读错表则前端
//   「最近巡检」恒 null。
// - 失败信号：manager.heartbeat_tasks 与 DB/get_tasks 不一致即断口；
//   get_config.lastTick 与注入值不符即读源错误。
// - 覆盖理由：manager 层写穿测试不经 handler 的 DB 先写与回读步骤；
//   last_tick 透出此前只有 manager 层写入测试，无 handler 读侧断言。
#[tokio::test]
async fn update_tasks_writes_through_and_get_config_exposes_last_tick() {
    let (app, _pool, manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");

    call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/tasks"),
            &token,
            json!({"tasks": [
                {"priority": "P1", "text": "巡检设备在线率", "paused": false},
                {"priority": "P2", "text": "检查固件版本", "paused": true},
            ]}),
        ),
    )
    .await;

    let tasks = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/tasks"), &token)).await;
    assert_eq!(tasks.as_array().expect("tasks array").len(), 2);

    // handler 回读 DB 后写穿统一循环任务表（trigger/prompt 读侧同一真源）。
    let shared = manager.heartbeat_tasks(WS);
    assert_eq!(shared.len(), 2, "任务表写穿统一循环共享表");
    assert!(shared.iter().any(|t| t.text == "巡检设备在线率" && !t.paused));
    assert!(shared.iter().any(|t| t.text == "检查固件版本" && t.paused));

    // D13：last_tick 读统一循环共享表（无 tick 过为 null）。
    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert!(cfg["lastTick"].is_null(), "未 tick 过 lastTick 须为 null");

    let tick_at = chrono::Utc::now();
    manager.heartbeat_last_ticks().insert(WS.to_string(), tick_at);
    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(
        cfg["lastTick"].as_str().expect("lastTick string"),
        tick_at.to_rfc3339(),
        "lastTick 必须来自统一循环 last_ticks 共享表"
    );
}

// Value card
// - 覆盖路径：PUT /heartbeat/config 带 rate_limit_per_key_per_min → DB
//   heartbeat_config JSON 落键 + manager 同键限速共享表写穿（spec §3 热改
//   生效）+ get_config 回读透出；不带该键的部分更新不得重置既有值。
// - 真实回归场景：写穿漏调 set_wake_rate_limit_per_min——DB 已改但
//   scheduler 仍按旧限速；或 validated() 重建配置把既有 rate_limit 抹掉。
// - 失败信号：manager.wake_rate_limit_per_min / DB 行 / get_config 三方
//   不一致即断口。
// - 覆盖理由：manager 访问器单测不经 handler 归并与 DB 写序。
#[tokio::test]
async fn update_config_rate_limit_writes_through_and_survives_partial_update() {
    let (app, pool, manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    let db = Db::new(pool.clone());

    let result = call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"rate_limit_per_key_per_min": 7}),
        ),
    )
    .await;
    assert_eq!(result["rateLimitPerKeyPerMin"], 7);
    assert_eq!(manager.wake_rate_limit_per_min(WS), Some(7), "限速共享表写穿");
    let persisted = db
        .load_heartbeat_config(WS)
        .await
        .expect("load")
        .expect("persisted config");
    assert_eq!(persisted.rate_limit_per_key_per_min, Some(7), "DB JSON 落键");

    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(cfg["rateLimitPerKeyPerMin"], 7, "get_config 透出限速");

    // 部分更新（只改 enabled）不得重置既有 rate_limit。
    call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"enabled": false}),
        ),
    )
    .await;
    assert_eq!(manager.wake_rate_limit_per_min(WS), Some(7), "部分更新不得重置限速");
    let persisted = db
        .load_heartbeat_config(WS)
        .await
        .expect("load")
        .expect("persisted config");
    assert_eq!(persisted.rate_limit_per_key_per_min, Some(7));
}
