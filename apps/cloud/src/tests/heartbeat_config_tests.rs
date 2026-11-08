//! 心跳配置/任务 handler 写穿测试（D11-⑤ + 终审 I-1 + D13，T15 读源切换后）。
//!
//! heartbeat_logs_tests 覆盖 /logs 读侧；本文件覆盖 config/tasks 写侧：
//! handler 先写 DB → 更新统一循环共享表（manager）→ notify，以及 get_config
//! 从统一循环共享表读 enabled/interval/last_tick（T15 起唯一读源）。

use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::domains::agent::agent_runtime::{AgentRuntime, RuntimeDeps};
use crate::domains::agent::snapshot::RestoreSnapshot;
use crate::domains::agent::thing_agent::ThingAgentManager;
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

    // I2：legacy interval 同步到 assistant_schedules（TriggerRouter 的判定集）。
    let schedule = db
        .get_assistant_schedule(WS)
        .await
        .expect("load schedule")
        .expect("schedule row");
    assert_eq!(schedule.interval_seconds, 45 * 60, "interval 分钟 → schedule 秒");
    assert!(schedule.enabled, "legacy enabled 同步到 schedule");
    assert!(schedule.next_wake_at.is_some(), "interval 变更须重算 next_wake_at");

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

    // I2：legacy pause 落到 authoritative schedule。
    let schedule = db
        .get_assistant_schedule(WS)
        .await
        .expect("load schedule")
        .expect("schedule row");
    assert!(!schedule.enabled, "enabled=false 必须同步到 assistant_schedules");
    assert_eq!(schedule.interval_seconds, 45 * 60);
}

// Value card
// - 覆盖路径：PUT /heartbeat/tasks 一版兼容（写入忽略 → 200 空列表 +
//   deprecated:true）+ get_config 的 last_tick 读统一循环 last_ticks
//   共享表（D13 实时读，T15 唯一读源）。
// - 真实回归场景：旧前端在 Batch 5 删除前仍会 PUT tasks——必须 200 而非
//   运行时报错；若写穿残留，旧任务表会再次驱动巡检；last_tick 读错表则
//   前端「最近巡检」恒 null。
// - 失败信号：PUT 返回非 200/非 deprecated，或 DB/manager 任务表被改写，
//   或 get_config.lastTick 与注入值不符即断口。
#[tokio::test]
async fn update_tasks_is_deprecated_noop_and_get_config_exposes_last_tick() {
    let (app, pool, manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    let db = Db::new(pool);

    let result = call(
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
    assert_eq!(result["deprecated"], true, "PUT tasks must be deprecated: {result}");
    assert_eq!(result["tasks"], json!([]), "PUT tasks must return an empty list");

    // 写入被忽略：DB 与统一循环共享表都保持空。
    let tasks = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/tasks"), &token)).await;
    assert_eq!(tasks.as_array().expect("tasks array").len(), 0);
    assert!(
        manager.heartbeat_tasks(WS).is_empty(),
        "write must not reach the shared table"
    );
    assert!(
        db.list_heartbeat_tasks(WS).await.expect("list tasks").is_empty(),
        "write must not reach the DB"
    );

    // D13：last_tick 读统一循环共享表（无 tick 过为 null）。
    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert!(cfg["lastTick"].is_null(), "未 tick 过 lastTick 须为 null");

    let tick_at = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).expect("timestamp");
    db.mark_assistant_wake(WS, tick_at + chrono::Duration::seconds(300), tick_at, "completed")
        .await
        .expect("mark wake");
    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(
        cfg["lastTick"].as_str().expect("lastTick string"),
        tick_at.to_rfc3339(),
        "lastTick 必须来自 assistant_schedules.last_wake_at"
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

// Value card
// - 覆盖路径：GET /heartbeat/config 的 enabled/interval 读 authoritative
//   assistant_schedules（不是 legacy 内存表）。
// - 真实回归场景：schedule 被 /agent/schedule 或旧 PUT 改成暂停/更长间隔后，
//   legacy 内存表未同步（跨重启）——GET 必须反映真正会被 tick 派发的值。
// - 失败信号：GET 返回 default enabled/300 而 schedule 是 disabled/720 即断口。
#[tokio::test]
async fn get_config_reads_schedule_as_authoritative_source() {
    let (app, pool, manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    let db = Db::new(pool);

    db.ensure_assistant_schedule(
        WS,
        &crate::domains::agent::session::wake::assistant_agent_id(WS),
        chrono::Utc::now(),
    )
    .await
    .expect("ensure schedule");
    db.update_assistant_schedule(WS, Some(false), Some(720), None, None, None)
        .await
        .expect("pause schedule");

    // 内存 legacy 表仍是默认值，不得影响读面。
    assert!(manager.heartbeat_enabled(WS), "legacy 内存表未改");
    let cfg = call(&app, get(&format!("/api/v1/workspaces/{WS}/heartbeat/config"), &token)).await;
    assert_eq!(cfg["enabled"], false);
    assert_eq!(cfg["intervalMinutes"], 12);
}

// Value card
// - 覆盖路径：AI 运维页真实线格式 `intervalMinutes`（camelCase）→ handler
//   反序列化 → assistant_schedules.interval_seconds/next_wake_at。
// - 真实回归场景：请求体只认 snake_case 时 UI 的间隔选择被静默忽略，页面
//   显示新间隔而 tick 仍按旧值。
// - 失败信号：schedule.interval_seconds 仍为默认 300 即断口。
#[tokio::test]
async fn update_config_interval_camel_case_updates_schedule_and_recomputes_next_wake() {
    let (app, pool, _manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    let before = chrono::Utc::now();
    call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"intervalMinutes": 10}),
        ),
    )
    .await;
    let after = chrono::Utc::now();

    let db = Db::new(pool);
    let row = db
        .get_assistant_schedule(WS)
        .await
        .expect("load schedule")
        .expect("schedule row");
    assert_eq!(row.interval_seconds, 600, "10 分钟必须落 600 秒");
    let next = row.next_wake_at.expect("interval 变更须重算 next_wake_at");
    // assistant_schedules 以秒精度存储：用整秒边界避免亚秒截断误判。
    let before_floor = chrono::DateTime::from_timestamp(before.timestamp(), 0).expect("timestamp");
    let after_ceil = chrono::DateTime::from_timestamp(after.timestamp() + 1, 0).expect("timestamp");
    assert!(next >= before_floor + chrono::Duration::seconds(600));
    assert!(next <= after_ceil + chrono::Duration::seconds(600));
}

#[derive(Default)]
struct CountingExecutor {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl crate::domains::agent::session::wake::WakeExecutor for CountingExecutor {
    async fn execute(
        &self,
        _schedule: tinyiothub_storage::assistant_schedule::AssistantSchedule,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> crate::domains::agent::session::wake::WakeReport {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::domains::agent::session::wake::WakeReport {
            session_key: "s".into(),
            outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
        }
    }
}

// Value card
// - 覆盖路径：legacy PUT /heartbeat/config enabled=false → assistant_schedules
//   → TriggerRouter::tick 的启用过滤。
// - 真实回归场景：AI 运维页暂停后 tick 仍派发 wake（页面「已停止」与
//   Assistant 行为矛盾）。
// - 失败信号：暂停工作区被 tick 派发（executor.calls > 0）即断口。
#[tokio::test]
async fn paused_workspace_is_not_dispatched_by_trigger_tick() {
    use crate::domains::agent::session::trigger::TriggerRouter;
    use std::sync::Arc;

    let (app, pool, _manager) = app_with_manager().await;
    let token = create_test_token("user-1", "tenant-1");
    call(
        &app,
        put(
            &format!("/api/v1/workspaces/{WS}/heartbeat/config"),
            &token,
            json!({"enabled": false}),
        ),
    )
    .await;

    let db = Arc::new(Db::new(pool));
    let executor = Arc::new(CountingExecutor::default());
    let router = Arc::new(TriggerRouter::new(db, executor.clone()));
    let handles = router.tick(chrono::Utc::now() + chrono::Duration::seconds(3600)).await;
    for handle in handles {
        handle.await.expect("tick task");
    }
    assert_eq!(
        executor.calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "paused workspace must not be dispatched"
    );
}
