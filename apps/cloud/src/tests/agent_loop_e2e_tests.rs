//! Task 10 (Phase 1 收口，D6)：全链路 E2E——
//! fake LLM 跑一轮真实 thing_agent loop → manager 发射 AgentEvent →
//! 生产持久化订阅者（Task 8 真件，非测试替身）投影 agent_runs →
//! HTTP 读 API 返回一致数据。
//!
//! 接线逐行对应生产 service_manager / Task 9 启动契约（D11-①③）：
//! build_agent_snapshot → bus 订阅先于 restore → AgentRuntime::restore →
//! reconcile_zombie_runs → run_persistence_subscriber → thing_agents.start。
//!
//! 与 thing_agent_loop_tests 的差异：那里挂的是手写的持久化替身，HTTP 层
//! 从未进链路；本测试用生产 run_persistence_subscriber + 生产路由树
//! （与 loop 共享同一文件库），断言事件 → DB → API 三段一致。
//!
//! 确定性：无裸 sleep 依赖——事件经 timeout recv 捕获，DB 投影经真实时间
//! 轮询收敛；timer 首 tick 停在 30s 合并窗口内，测试窗口内仅指令 run 落库。

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use zeroclaw::providers::{ChatRequest, ChatResponse};
use zeroclaw_api::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};

use tinyiothub_policy::autonomy::{AutonomyMode, AutonomyPolicy};
use tinyiothub_storage::Db;

use crate::bootstrap::{build_agent_snapshot, reconcile_zombie_runs};
use crate::domains::agent::host::{
    autonomous_factory::AutonomousAgentFactory, persist::run_persistence_subscriber,
    thing_agent_host::CloudThingAgentHost, tools::ThingToolContext,
};
use crate::domains::event::{bus::ThingEventBus, router::ThrottleState};
use crate::test_utils::{auth_header, create_test_token, response_parts, seed_test_workspace, test_app_state_on_pool};
use tinyiothub_agent::pool::ProviderFactory;
use tinyiothub_agent::runtime::{
    event::bus::AiEventPublisher,
    events::{AgentEventBus, AgentEventKind},
    heartbeat::types::HeartbeatConfig,
    runtime::{AgentRuntime, RuntimeDeps},
    thing_agent::{DirectiveSink, Priority, ThingAgentManagerConfig, TriggerSource, WakeSignal},
};

const WS: &str = "ws-e2e";
const THING: &str = "dev-e2e";
const DIRECTIVE: &str = "检查车间温度";
const SUMMARY: &str = "车间温度正常，无需动作";

// ── scripted model provider (no network) ───────────────────────
//
// 一轮收尾：任何 prompt 都直接回最终文本（无工具调用）——指令 run 以
// outcome=no_action_needed 完成，summary 即该文本。链路的被测对象是
// run → event → DB → API，不是工具编排（后者由 thing_agent_loop_tests 覆盖）。
#[derive(Clone, Default)]
struct E2eScriptedProvider {
    calls: Arc<Mutex<usize>>,
}

impl E2eScriptedProvider {
    fn call_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait::async_trait]
impl zeroclaw::providers::traits::ModelProvider for E2eScriptedProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        Ok(SUMMARY.into())
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        *self.calls.lock().unwrap() += 1;
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
/// :memory: 每连接独立会分叉（thing_agent_loop_tests 同款取舍）。
async fn e2e_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:///{}/e2e.db?mode=rwc", dir.path().display());
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

/// 场景种子：tenant/workspace + 物模型（设备/动作/属性，与 loop 夹具一致）
/// + user-1 admin 角色（agent 读 API 的守卫要求，见 agent_tasks.rs）。
async fn seed_scene(pool: &sqlx::SqlitePool) {
    seed_test_workspace(pool, "tenant-1", WS).await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')")
        .bind(THING)
        .bind("E2E Device")
        .bind(WS)
        .execute(pool)
        .await
        .expect("insert device");
    sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES ('act-set_fan', ?, 'set_fan')")
        .bind(THING)
        .execute(pool)
        .await
        .expect("register action");
    sqlx::query(
        "INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES ('prop-temp', ?, 'temp', 'float')",
    )
    .bind(THING)
    .execute(pool)
    .await
    .expect("register property");

    sqlx::query(
        "INSERT OR IGNORE INTO roles (id, name, description, is_administrator) \
         VALUES ('role-admin', '系统管理员', '拥有系统所有权限', 1)",
    )
    .execute(pool)
    .await
    .expect("seed admin role");
    sqlx::query("INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES ('ur-user-1', 'user-1', 'role-admin')")
        .execute(pool)
        .await
        .expect("grant admin");
}

fn act_policy() -> AutonomyPolicy {
    AutonomyPolicy {
        mode: AutonomyMode::Act,
        allowed_actions: vec!["*".to_string()],
        denied_actions: vec![],
        max_actions_per_run: 3,
        max_actions_per_hour: 30,
    }
}

/// 真实时间轮询（20ms × 500 = 10s 上限），等异步投影收敛。
async fn wait_for(
    what: &str,
    mut cond: impl FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>,
) {
    for _ in 0..500 {
        if cond().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn run_count(pool: &sqlx::SqlitePool, run_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("count runs")
}

fn auth_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .unwrap()
}

// ── D6: full chain ─────────────────────────────────────────────

#[tokio::test]
async fn thing_agent_run_flows_event_to_db_to_read_api() {
    let (pool, _dir) = e2e_pool().await;
    // AppState 先建：其构造会 seed user-1（user_roles 的 FK 目标）。
    let app_state = test_app_state_on_pool(pool.clone()).await;
    seed_scene(&pool).await;
    let db = Arc::new(Db::new(pool.clone()));

    let policy_repo = db.clone();
    policy_repo
        .save_autonomy_policy(WS, &act_policy(), "test")
        .await
        .expect("save policy");

    // 真实启动顺序（Task 9）：快照 → bus 外建、订阅先于 restore → restore →
    // 僵尸 reconcile。persist_rx 是持久化订阅者的 receiver（生产契约：
    // restore 前取得，restore 期间及之后的事件不丢）；watch_rx 是本测试的
    // 事件断言出口。
    let snapshot = build_agent_snapshot(&db).await.expect("snapshot build");
    let agent_events = Arc::new(AgentEventBus::new(256));
    let persist_rx = agent_events.subscribe();
    let mut watch_rx = agent_events.subscribe();

    // 真实 manager 组件：CloudThingAgentHost + Db 门面（autonomy 委托）+
    // AutonomousAgentFactory（scripted provider，无网络）。
    let thing_bus = Arc::new(ThingEventBus::new());
    let provider = E2eScriptedProvider::default();
    let provider_factory: ProviderFactory = {
        let provider = provider.clone();
        Arc::new(move || Ok(Box::new(provider.clone()) as Box<dyn zeroclaw::providers::traits::ModelProvider>))
    };
    let observer: Arc<dyn zeroclaw::observability::Observer> = Arc::from(zeroclaw::observability::create_observer(
        &zeroclaw::config::schema::ObservabilityConfig {
            backend: zeroclaw::config::schema::ObservabilityBackend::None,
            ..Default::default()
        },
    ));
    let factory = Arc::new(AutonomousAgentFactory::new(
        pool.clone(),
        policy_repo.clone(),
        thing_bus.clone(),
        Arc::new(ThrottleState::new(60)),
        Arc::new(zeroclaw::memory::NoneMemory::new("e2e")),
        observer,
        provider_factory,
        "stub-model".to_string(),
        ThingToolContext {
            pending_actions: Some(Arc::new(dashmap::DashMap::new())),
            ..Default::default()
        },
    ));

    let event_bus = Arc::new(tinyiothub_runtime::EventBus::new());
    let deps = RuntimeDeps {
        event_publisher: Arc::new(AiEventPublisher::new(event_bus.clone())),
        heartbeat_config: HeartbeatConfig::default(),
        thing_agent_host: Arc::new(CloudThingAgentHost::new(pool.clone(), thing_bus)),
        policy_repo: Arc::new(crate::domains::agent::host::ports::StorageAutonomyPolicyReader::new(
            policy_repo,
        )),
        agent_provider: factory,
        thing_agent_config: ThingAgentManagerConfig {
            timer_interval: Duration::from_secs(24 * 3600),
            min_wake_level: 3,
            // 30s 合并窗口：timer 首 tick 停在窗口内，测试时长（≪30s）内仅
            // 指令 run 落库——"仅 1 行"的断言因此确定。
            merge_window: Duration::from_secs(30),
        },
        event_bus,
        drop_notifier: None,
        agent_events: agent_events.clone(),
    };
    let runtime = Arc::new(AgentRuntime::restore(snapshot, deps));
    reconcile_zombie_runs(&db, &runtime).await;

    // 生产持久化订阅者（Task 8 真件）。
    let shutdown = CancellationToken::new();
    let subscriber = tokio::spawn(run_persistence_subscriber(
        runtime.clone(),
        db.clone(),
        persist_rx,
        shutdown.clone(),
    ));

    // HTTP 应用与 loop 共享同一文件库（生产路由树，含 auth 中间件）。
    let api_router = crate::api::create_router(&app_state);
    let app = Router::new().nest("/api", api_router).with_state(app_state);
    let token = create_test_token("user-1", "tenant-1");

    // 驱动一轮真实 loop：用户指令 → DirectiveSink → manager → runner → fake LLM。
    runtime.thing_agents().start(WS);
    let sink: &dyn DirectiveSink = runtime.thing_agents().as_ref();
    sink.enqueue(WakeSignal {
        workspace_id: WS.to_string(),
        priority: Priority::High,
        source: TriggerSource::UserDirective {
            user_id: "user-1".to_string(),
            text: DIRECTIVE.to_string(),
            session_key: None,
            source: None,
            problem_key: None,
        },
        dedup_key: None,
    })
    .expect("directive accepted");

    // 断言 1（事件出口）：manager 发射 RunRecorded，载荷即本轮 run 报告。
    let report = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match watch_rx.recv().await {
                Ok(ev) => {
                    if let AgentEventKind::RunRecorded { report, .. } = ev.kind {
                        break report;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => panic!("agent event bus closed"),
            }
        }
    })
    .await
    .expect("RunRecorded emitted within 5s");
    assert_eq!(report.workspace_id, WS);
    assert_eq!(report.summary, SUMMARY);
    assert!(provider.call_count() >= 1, "fake LLM 至少被调用一轮");

    // 断言 2（DB 投影）：生产订阅者把同一 run 落进 agent_runs。
    wait_for("run projected to agent_runs", || {
        let pool = pool.clone();
        let run_id = report.run_id.clone();
        Box::pin(async move { run_count(&pool, &run_id).await == 1 })
    })
    .await;

    // 断言 3（HTTP 读 API）：与事件/DB 同一行，字段一致。
    let (status, json) = response_parts(
        app.oneshot(auth_get(&format!("/api/v1/workspaces/{WS}/agent/runs"), &token))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "list runs: {json}");
    let runs = json["result"]["runs"].as_array().expect("runs array");
    assert_eq!(
        runs.len(),
        1,
        "仅指令 run 一行（timer 首 tick 停在 30s 合并窗口）: {runs:?}"
    );
    assert_eq!(runs[0]["id"].as_str().unwrap(), report.run_id);
    assert_eq!(runs[0]["triggerType"], "user");
    assert_eq!(runs[0]["outcome"], "no_action_needed");
    assert_eq!(runs[0]["summary"], SUMMARY);

    shutdown.cancel();
    subscriber.abort();
}
