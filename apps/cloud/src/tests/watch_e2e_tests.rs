//! E1 Task 11 收尾：watch 值守端到端——
//! 注册 watch（温度>30，真实 API）→ 驱动读数越限（`consume_driver_reading`
//! 共享管线，模拟驱动读数流入的同一入口）→ 2s 内 TriggerSource::Watch 的
//! run 落 agent_runs（生产持久化订阅者投影）+ hub_ledger 落 watch_hit；
//! 重启（全新 AgentRuntime/manager 重建装配）→ watch 网从 watch_rules
//! 重织 → 再越限再命中（值守无人工介入继续，账本可回放全程）。
//!
//! 装配逐行对应生产 service_manager（E1 Task 8 bootstrap 硬顺序）：
//! set_watch_deps（HubLedgerAdapter/WatchRuleStoreAdapter/ThingCacheAdapter
//! 全真适配器 + 生产 event_bus）先于 start。
//!
//! 取舍记录：
//! - 数据源入口：模拟器（attach_simulated_driver）产值随机不可控，无法
//!   确定性构造"越限"时刻；本测试经 `DataServer::consume_driver_reading`
//!   （DataServer::run 消费任务调用的同一 pub 入口）注入受控值——管线
//!   （apply_property_values → cache_and_publish → EventBus → WatchEngine
//!   → WatchTrigger → manager → run → agent_runs）全真，数据源受控
//!   （park_derived_e2e_test 同款取舍）。无 wrapper 物走 X4 device_cache
//!   回退直写——生产代码路径，事件形状与 wrapper 路径一致。
//! - 确定性无裸 sleep：引擎就绪屏障用 WatchCommand::Summary——与 Upsert
//!   同通道 FIFO，oneshot 应答即注册/重织已生效；事件到位以 Summary 回读
//!   LiveState 值收敛为证。2s 上限是 spec §6 的秒级唤醒断言。

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{Router, body::Body, http::Request};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use tinyiothub_agent::pool::ProviderFactory;
use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse};
use tinyiothub_agent::runtime::{
    events::AgentEventBus,
    runtime::{AgentRuntime, RuntimeDeps},
    thing_agent::{ThingAgentManager, ThingAgentManagerConfig, WatchCommand, WatchDeps},
};
use tinyiothub_core::driver::{DriverReading, ResultValue};
use tinyiothub_core::models::thing::{Thing, ThingStatus};
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_policy::autonomy::{AutonomyMode, AutonomyPolicy};
use tinyiothub_runtime::data_server::{DataServer, DriverCache};
use tinyiothub_runtime::ports::ThingCacheSource;
use tinyiothub_runtime::watch::live_state::ThingLiveSummary;
use tinyiothub_storage::Db;

use crate::bootstrap::{build_agent_snapshot, reconcile_zombie_runs};
use crate::domains::agent::host::{
    autonomous_factory::AutonomousAgentFactory, persist::persist::run_persistence_subscriber,
    thing_agent_host::CloudThingAgentHost, tools::ThingToolContext,
};
use crate::domains::event::{bus::ThingEventBus, router::ThrottleState};
use crate::shared::runtime_ports::{HubLedgerAdapter, ThingCacheAdapter, WatchRuleStoreAdapter};
use crate::state::AppState;
use crate::test_utils::{auth_header, create_test_token, response_parts, seed_test_workspace, test_app_state_on_pool};

const WS: &str = "ws-watch-e2e";
const THING: &str = "dev-watch-e2e";
const SUMMARY: &str = "温度越限已知悉，无需动作";

// ── scripted model provider (no network) ───────────────────────
//
// 任何 prompt 直接回最终文本（无工具调用）——watch run 以
// outcome=no_action_needed 完成。被测对象是 watch 命中 → 唤醒 → 落库
// 链路，不是工具编排（agent_loop_e2e_tests 同款取舍）。
#[derive(Clone, Default)]
struct E2eScriptedProvider;

#[async_trait::async_trait]
impl tinyiothub_agent::port::provider::ModelProvider for E2eScriptedProvider {
    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        Ok(ChatResponse {
            text: Some(SUMMARY.to_string()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        })
    }
}

impl Attributable for E2eScriptedProvider {
    fn role(&self) -> Role {
        Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
    }
    fn alias(&self) -> &str {
        "E2eScriptedProvider"
    }
}

// ── fixture ────────────────────────────────────────────────────

/// 临时文件库 + 多连接：后台 loop / 持久化订阅者 / HTTP handler 并发访问，
/// :memory: 每连接独立会分叉（agent_loop_e2e_tests 同款取舍）。
async fn e2e_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:///{}/watch-e2e.db?mode=rwc", dir.path().display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("temp file pool");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("migrations");
    (pool, dir)
}

/// 场景种子：workspace + 物 + 温度属性（DB 行 + device_cache 真相——
/// LiveState/engine 以 cache 为真相源，dispatch 按 cache 过滤工作区）。
async fn seed_scene(pool: &sqlx::SqlitePool, app_state: &AppState) {
    seed_test_workspace(pool, "tenant-1", WS).await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')")
        .bind(THING)
        .bind("E2E Watch Thing")
        .bind(WS)
        .execute(pool)
        .await
        .expect("insert device");
    sqlx::query(
        "INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES ('prop-temp', ?, '温度', 'float')",
    )
    .bind(THING)
    .execute(pool)
    .await
    .expect("register property");

    app_state.device_cache.insert(Thing {
        id: THING.into(),
        name: "E2E Watch Thing".into(),
        workspace_id: Some(WS.into()),
        status: ThingStatus::Online,
        properties: Some(vec![ThingProperty {
            id: "prop-temp".into(),
            thing_id: THING.into(),
            name: "温度".into(),
            data_type: Some("float".into()),
            ..ThingProperty::default()
        }]),
        ..Thing::default()
    });
}

/// 一次装配（生产 service_manager 同款顺序）：快照 → bus 订阅先于 restore →
/// restore → 僵尸 reconcile → 持久化订阅者 → set_watch_deps → 注入
/// app_state.thing_agents → start。重启 = 本函数再跑一遍（同库新装配）。
struct AgentStack {
    runtime: Arc<AgentRuntime>,
    shutdown: CancellationToken,
    subscriber: tokio::task::JoinHandle<()>,
}

async fn assemble(
    pool: &sqlx::SqlitePool,
    app_state: &mut AppState,
    cache_src: &Arc<dyn ThingCacheSource>,
) -> AgentStack {
    let db = Arc::new(Db::new(pool.clone()));
    let snapshot = build_agent_snapshot(&db).await.expect("snapshot build");
    let agent_events = Arc::new(AgentEventBus::new(256));
    let persist_rx = agent_events.subscribe();

    let thing_bus = Arc::new(ThingEventBus::new());
    let provider = E2eScriptedProvider;
    let provider_factory: ProviderFactory =
        Arc::new(move || Ok(Box::new(provider.clone()) as Box<dyn tinyiothub_agent::port::provider::ModelProvider>));
    let factory = Arc::new(AutonomousAgentFactory::new(
        pool.clone(),
        db.clone(),
        thing_bus.clone(),
        Arc::new(ThrottleState::new(60)),
        Arc::new(tinyiothub_agent::port::memory::NoopMemory),
        Arc::new(tinyiothub_agent::port::observer::NoopObserver),
        provider_factory,
        "stub-model".to_string(),
        ThingToolContext {
            pending_actions: Some(Arc::new(dashmap::DashMap::new())),
            ..Default::default()
        },
    ));

    let deps = RuntimeDeps {
        thing_agent_host: Arc::new(CloudThingAgentHost::new(pool.clone(), thing_bus)),
        policy_repo: Arc::new(crate::domains::agent::host::ports::StorageAutonomyPolicyReader::new(
            db.clone(),
        )),
        agent_provider: factory,
        thing_agent_config: ThingAgentManagerConfig {
            timer_interval: Duration::from_secs(24 * 3600),
            min_wake_level: 3,
            ..Default::default()
        },
        event_bus: app_state.event_bus.clone(),
        drop_notifier: None,
        agent_events: agent_events.clone(),
        ticket_resolutions: Arc::new(tinyiothub_agent::runtime::thing_agent::traits::NoopTicketResolutions),
    };
    let runtime = Arc::new(AgentRuntime::restore(snapshot, deps));
    reconcile_zombie_runs(&db, &runtime).await;

    let shutdown = CancellationToken::new();
    let subscriber = tokio::spawn(run_persistence_subscriber(
        runtime.clone(),
        db.clone(),
        persist_rx,
        shutdown.clone(),
    ));

    // E1 Task 8 硬顺序：set_watch_deps 先于 start——全真适配器 + 生产 event_bus。
    let manager = runtime.thing_agents().clone();
    manager.set_watch_deps(WatchDeps {
        cache: cache_src.clone(),
        ledger: Arc::new(HubLedgerAdapter::new((*app_state.db).clone())),
        rules: Arc::new(WatchRuleStoreAdapter((*app_state.db).clone())),
        events: app_state.event_bus.clone(),
    });
    app_state.thing_agents = Some(manager);
    runtime.thing_agents().start(WS);

    AgentStack {
        runtime,
        shutdown,
        subscriber,
    }
}

impl AgentStack {
    async fn retire(self) {
        self.runtime.thing_agents().stop(WS).await;
        self.shutdown.cancel();
        self.subscriber.abort();
    }
}

// ── 引擎观测屏障（确定性，无裸 sleep）─────────────────────────────

/// 经 engine 命令通道读单物实时摘要。与 Upsert 同通道 FIFO：应答即此前
/// 命令已生效；重启后首个应答即启动重织已完成（重织在 select 循环之前）。
async fn live_summary(manager: &ThingAgentManager) -> Option<ThingLiveSummary> {
    let sink = manager.watch_command_sink(WS).expect("watch 命令通道已挂载");
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<ThingLiveSummary>>();
    sink.send(WatchCommand::Summary {
        thing_id: THING.into(),
        reply: tx,
    })
    .await
    .expect("summary 命令投递");
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .expect("engine 2s 内应答")
        .expect("engine 应答")
}

/// 轮询 LiveState 温度值收敛——证明对应 PropertyChange 事件已被 engine 消费
/// （事件与命令不同通道，仅靠命令屏障无法断言事件序）。
async fn wait_live_temp(manager: &ThingAgentManager, want: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let got = live_summary(manager).await.and_then(|s| s.values.get("温度").cloned());
        if got.as_deref() == Some(want) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "超时等待 LiveState 温度={want}（最近 {got:?}）"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ── 数据注入与 DB 断言 ─────────────────────────────────────────

/// 温度上报入口：与 DataServer::run() 消费任务同一函数（真实共享管线；
/// 返回即缓存已更新、事件已发布进 bus）。
async fn ingest_temp(
    driver_cache: &DriverCache,
    cache_src: &Arc<dyn ThingCacheSource>,
    app_state: &AppState,
    value: &str,
) {
    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: THING.to_string(),
            values: vec![ResultValue::new(
                "温度".to_string(),
                "float".to_string(),
                Some(value.to_string()),
            )],
        },
        driver_cache,
        cache_src,
        app_state.event_bus.clone(),
    )
    .await;
}

async fn watch_run_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agent_runs WHERE workspace_id = ? AND trigger_type = 'watch'")
        .bind(WS)
        .fetch_one(pool)
        .await
        .expect("count watch runs")
}

async fn ledger_hit_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM hub_ledger WHERE workspace_id = ? AND kind = 'watch_hit'")
        .bind(WS)
        .fetch_one(pool)
        .await
        .expect("count ledger hits")
}

/// spec §6 秒级唤醒断言：deadline 内轮询到目标 watch run 计数，返回实际耗时
///（调用方在上报越限值后立即调用——计时起点即上报时刻）。
async fn wait_watch_runs(pool: &sqlx::SqlitePool, want: i64, deadline: Duration) -> Duration {
    let t0 = Instant::now();
    loop {
        if watch_run_count(pool).await == want {
            return t0.elapsed();
        }
        assert!(t0.elapsed() < deadline, "超时等待 watch run 数={want}（{deadline:?}）");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ── HTTP 辅助 ─────────────────────────────────────────────────

fn build_router(app_state: &AppState) -> Router {
    let api_router = crate::api::create_router(app_state);
    Router::new().nest("/api", api_router).with_state(app_state.clone())
}

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    match body {
        Some(b) => builder.body(Body::from(b.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

// ── 用例 ─────────────────────────────────────────────────────

// Value card
// - 覆盖路径：真实 API 注册 watch（温度>30，persistent）→ 命令通道写穿 →
//   共享管线读数越限 → WatchEngine 命中 → hub_ledger 落 watch_hit →
//   WatchTrigger → WakeSignal{Watch} → 全管线 run 落 agent_runs
//   （trigger_type='watch'，2s 内）→ 重启重建装配 → watch 网从
//   watch_rules 重织（GET 持久源仍在）→ 再越限再命中（值守无人工介入继续）。
// - 真实回归场景：接线漏一环（写穿未进 engine / 事件未分发 / trigger 未
//   注册 / 重织未跑）——值守规则静默不命中，用户以为有值守实际没有。
// - 失败信号：2s 内无 watch run、账本缺 watch_hit、重启后不再命中即断口。
#[tokio::test]
async fn watch_hit_wakes_run_within_2s_and_restart_reweaves() {
    let (pool, _dir) = e2e_pool().await;
    let mut app_state = test_app_state_on_pool(pool.clone()).await;
    seed_scene(&pool, &app_state).await;
    let cache_src: Arc<dyn ThingCacheSource> = Arc::new(ThingCacheAdapter(app_state.device_cache.clone()));
    // 空 driver_cache：无 wrapper 物走 X4 device_cache 回退直写（生产路径）。
    let driver_cache: DriverCache = DriverCache::new(16);

    // policy=Off：门控 timer 首 tick，测试窗口内仅 watch run 落库（watch run
    // 无工具调用，不过 invoke_action 策略门，Off 不影响被测链路）。
    app_state
        .db
        .save_autonomy_policy(
            WS,
            &AutonomyPolicy {
                mode: AutonomyMode::Off,
                allowed_actions: vec!["*".to_string()],
                denied_actions: vec![],
                max_actions_per_run: 3,
                max_actions_per_hour: 30,
            },
            "test",
        )
        .await
        .expect("save policy");

    // ── 阶段一：注册 → 越限 → 2s 内 watch run ─────────────────────
    let stack = assemble(&pool, &mut app_state, &cache_src).await;
    let manager = stack.runtime.thing_agents().clone();
    let app = build_router(&app_state);
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/watch-rules"),
            &token,
            Some(json!({
                "condition": {"type": "property", "thing_id": THING, "property": "温度", "op": "gt", "value": 30.0},
                "persistent": true,
                "timeoutSecs": 600,
            })),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(response).await;
    assert_eq!(body["code"], 0, "注册 watch 必须成功: {status} {body}");
    let rule_id = body["result"]["id"].as_str().expect("server 分配 id").to_string();

    // 就绪屏障：Summary 应答 = Upsert 已进 engine 注册表（同通道 FIFO）。
    live_summary(&manager).await;

    // 未越限基线：25 落到 LiveState 后不命中（且强制 armed=true）。
    ingest_temp(&driver_cache, &cache_src, &app_state, "25").await;
    wait_live_temp(&manager, "25").await;
    assert_eq!(watch_run_count(&pool).await, 0, "未越限不得产生 watch run");

    // 越限：上报时刻起计时，2s 内 watch run 落库（spec §6 秒级唤醒）。
    ingest_temp(&driver_cache, &cache_src, &app_state, "31").await;
    let elapsed = wait_watch_runs(&pool, 1, Duration::from_secs(2)).await;
    assert!(elapsed < Duration::from_secs(2), "秒级唤醒: {elapsed:?}");

    // 账本可回放：watch_hit 落行（rule_id 对得上）。
    assert_eq!(ledger_hit_count(&pool).await, 1, "账本落 watch_hit 一行");
    let (payload,): (String,) = sqlx::query_as(
        "SELECT payload FROM hub_ledger WHERE workspace_id = ? AND kind = 'watch_hit' ORDER BY rowid DESC LIMIT 1",
    )
    .bind(WS)
    .fetch_one(&pool)
    .await
    .expect("latest ledger hit");
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["rule_id"], json!(rule_id));
    assert_eq!(payload["matched"], json!(true));

    // ── 阶段二：重启（重建装配）→ watch 网从 watch_rules 重织 ─────
    stack.retire().await;
    assert!(manager.watch_command_sink(WS).is_none(), "stop 丢弃命令通道");

    let stack2 = assemble(&pool, &mut app_state, &cache_src).await;
    let manager2 = stack2.runtime.thing_agents().clone();
    let app2 = build_router(&app_state);

    // 持久源仍在（重启不丢规则）。
    let response = app2
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/workspaces/{WS}/watch-rules"),
            &token,
            None,
        ))
        .await
        .unwrap();
    let (_status, body) = response_parts(response).await;
    let rules = body["result"]["rules"].as_array().expect("rules 数组");
    assert_eq!(rules.len(), 1, "重启后持久源规则仍在: {body}");
    assert_eq!(rules[0]["id"], json!(rule_id));

    // 重织屏障：Summary 应答 = 启动重织已完成（重织在 select 循环之前）。
    live_summary(&manager2).await;

    // rearm：重启后 cache 真相温度=31 已越限，only-new-data 语义可能使
    // 重织注册未武装——先落 25（不成立 → rearm），再越限。
    ingest_temp(&driver_cache, &cache_src, &app_state, "25").await;
    wait_live_temp(&manager2, "25").await;

    // 再越限：重织的 watch 网 2s 内再次唤醒（值守无人工介入继续）。
    ingest_temp(&driver_cache, &cache_src, &app_state, "32").await;
    wait_watch_runs(&pool, 2, Duration::from_secs(2)).await;
    assert_eq!(ledger_hit_count(&pool).await, 2, "账本可回放全程：两次命中各一行");

    stack2.retire().await;
}
