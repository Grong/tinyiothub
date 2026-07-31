//! Thing-agent closed-loop integration tests (T15)
//!
//! Real migrated SQLite DB + real `route_thing_event` routing + real
//! AutonomousAgentFactory (with a scripted model provider, no network) +
//! real ThingAgentManager wiring:
//!
//! 1. MQTT 路径同款 `route_thing_event` 上报 warning 事件 → 广播 → T7 触发器
//!    → T8 合并窗口 → Run（T10 prompt → T11 工厂 → T9 runner）→ invoke_action
//!    过 T4 策略门 → 模拟驱动下发（status simulated/executed）→ T12 落库
//!    verified=true（read_property 回读）→ T13 回推（无会话 → 告警）。
//! 2. 同物 30s 内 5 事件 → 仅 1 次唤醒（合并窗口）。
//! 3. actor='agent' 的事件不唤醒（共振防护 O21）——包括 agent 自己动作产生的事件。
//! 4. T14 用户指令 → DirectiveSink → Run → T13 append_message 回推会话。
//!
//! 时间控制：真实时间 + 亚秒合并窗口（ThingAgentManagerConfig.merge_window）。
//! 不用 tokio 暂停时钟：auto-advance 会在每个 runtime 空闲点（sqlx-sqlite
//! 工作线程往返）跳到最近的定时器，时钟竞速无法控制（acquire 超时误触发、
//! 24h 巡检 tick 狂奔、事件窗口被时钟跳跃撕裂）。

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::Row;
use tinyiothub_ai::{
    policy::autonomy::{AutonomyMode, AutonomyPolicy, PolicyRepository},
    thing_agent::{
        DirectiveSink, EnqueueError, Runner, ThingAgentManager, ThingAgentManagerConfig,
        TriggerSource, WakeSignal,
    },
};
use tinyiothub_core::models::event::EventLevel;
use zeroclaw::providers::{ChatRequest, ChatResponse, ToolCall};
use zeroclaw_api::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};

use crate::{
    modules::{
        agent::{
            agent_runs_repo::SqliteAgentRunsRepository,
            autonomous_factory::{AutonomousAgentFactory, ProviderFactory},
            policy_repo::SqlitePolicyRepository,
            thing_agent_host::CloudThingAgentHost,
        },
        event::{
            bus::ThingEventBus,
            router::{ThingEventInput, ThrottleState, route_thing_event},
        },
    },
    test_utils::seed_test_workspace,
};

const WS: &str = "ws-loop";
const THING: &str = "dev-1";
const EVENT_KEY: &str = "thing:dev-1:event:temp_high";

// ── scripted model provider (no network) ───────────────────────
//
// Content-aware script, robust to interleaved timer/directive turns:
// - a completed read_property result  → final summary text;
// - a completed invoke_action result  → call read_property (回读验证);
// - the current user prompt mentions temp_high → call invoke_action;
// - anything else (timer 巡检 etc.) → plain "done" text.
#[derive(Clone, Default)]
struct LoopScriptedProvider {
    calls: Arc<Mutex<usize>>,
}

impl LoopScriptedProvider {
    fn call_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait::async_trait]
impl zeroclaw::providers::traits::ModelProvider for LoopScriptedProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        Ok("done".into())
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        *self.calls.lock().unwrap() += 1;
        let any = |needle: &str| request.messages.iter().any(|m| m.content.contains(needle));
        let current_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or_default();

        let text = |t: &str| ChatResponse {
            text: Some(t.to_string()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        };
        let call = |id: &str, name: &str, args: serde_json::Value| ChatResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: args.to_string(),
                extra_content: None,
            }],
            usage: None,
            reasoning_content: None,
        };

        if any("\"currentValue\"") || any("currentValue") {
            Ok(text("已开启风扇并确认温度回落"))
        } else if any("simulated") || any("\"executed\"") {
            // invoke_action 已下发（模拟/真实驱动）→ 回读验证。
            // 工具结果在消息流中的序列化间距不可依赖，匹配裸子串。
            Ok(call(
                "c-read",
                "read_property",
                serde_json::json!({"thingId": THING, "propertyName": "temp"}),
            ))
        } else if current_user.contains("temp_high") {
            Ok(call(
                "c-invoke",
                "invoke_action",
                serde_json::json!({"thingId": THING, "actionName": "set_fan", "params": {"speed": 3}}),
            ))
        } else {
            Ok(text("done"))
        }
    }
}

impl Attributable for LoopScriptedProvider {
    fn role(&self) -> Role {
        Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
    }
    fn alias(&self) -> &str {
        "LoopScriptedProvider"
    }
}

// ── fixture ────────────────────────────────────────────────────

async fn test_pool(name: &str) -> (sqlx::SqlitePool, tempfile::TempDir) {
    // 临时文件库 + 多连接：测试任务与后台 loop（触发器/run 链路/回推）并发
    // 访问数据库，单连接池会把并发查询串成等待。
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:///{}/{name}.db?mode=rwc", dir.path().display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("temp file pool");
    crate::shared::persistence::migrations::run_migrations(&pool).await.expect("migrations");
    (pool, dir)
}

async fn seed_device(pool: &sqlx::SqlitePool) {
    seed_test_workspace(pool, "tenant-1", WS).await;
    sqlx::query(
        "INSERT INTO devices (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')",
    )
    .bind(THING)
    .bind("Loop Device")
    .bind(WS)
    .execute(pool)
    .await
    .expect("insert device");
    sqlx::query(
        "INSERT INTO thing_actions (id, device_id, name) VALUES ('act-set_fan', ?, 'set_fan')",
    )
    .bind(THING)
    .execute(pool)
    .await
    .expect("register action");
    sqlx::query("INSERT INTO thing_properties (id, device_id, name, data_type) VALUES ('prop-temp', ?, 'temp', 'float')")
        .bind(THING)
        .execute(pool)
        .await
        .expect("register property");
}

struct LoopFixture {
    pool: sqlx::SqlitePool,
    bus: Arc<ThingEventBus>,
    manager: Arc<ThingAgentManager>,
    provider: LoopScriptedProvider,
    _dir: tempfile::TempDir,
}

async fn fixture(name: &str) -> LoopFixture {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let (pool, dir) = test_pool(name).await;
    seed_device(&pool).await;

    let policy_repo = Arc::new(SqlitePolicyRepository::new(pool.clone()));
    policy_repo
        .save_autonomy(
            WS,
            &AutonomyPolicy {
                mode: AutonomyMode::Act,
                allowed_actions: vec!["*".to_string()],
                denied_actions: vec![],
                max_actions_per_run: 3,
                max_actions_per_hour: 30,
            },
            "test",
        )
        .await
        .expect("save policy");

    let bus = Arc::new(ThingEventBus::new());
    let provider = LoopScriptedProvider::default();
    let provider_factory: ProviderFactory = {
        let provider = provider.clone();
        Arc::new(move || {
            Ok(Box::new(provider.clone()) as Box<dyn zeroclaw::providers::traits::ModelProvider>)
        })
    };
    let observer: Arc<dyn zeroclaw::observability::Observer> = Arc::from(
        zeroclaw::observability::create_observer(&zeroclaw::config::schema::ObservabilityConfig {
            backend: zeroclaw::config::schema::ObservabilityBackend::None,
            ..Default::default()
        }),
    );
    let factory = Arc::new(AutonomousAgentFactory::new(
        pool.clone(),
        policy_repo.clone(),
        bus.clone(),
        Arc::new(ThrottleState::new(60)),
        Arc::new(zeroclaw::memory::NoneMemory::new("loop-test")),
        observer,
        provider_factory,
        "stub-model".to_string(),
    ));

    let manager = Arc::new(ThingAgentManager::new(
        Arc::new(CloudThingAgentHost::new(pool.clone(), bus.clone())),
        policy_repo,
        factory,
        Arc::new(SqliteAgentRunsRepository::new(pool.clone())),
        Arc::new(Runner::new()),
        ThingAgentManagerConfig {
            // 定时巡检不干扰断言（首 tick 停在合并窗口；按 dedup_key 过滤）。
            timer_interval: Duration::from_secs(24 * 3600),
            min_wake_level: 3,
            // 亚秒合并窗口：真实时间下测试快速收敛。
            merge_window: Duration::from_millis(150),
        },
    ));

    LoopFixture { pool, bus, manager, provider, _dir: dir }
}

fn warning_event() -> ThingEventInput {
    ThingEventInput {
        thing_id: THING.to_string(),
        workspace_id: WS.to_string(),
        event_name: "temp_high".to_string(),
        level: EventLevel::Warning,
        data: serde_json::json!({"value": 87.5}),
        ts: None,
        template_events: None,
    }
}

async fn route(fx: &LoopFixture, actor: &str) {
    let throttle = ThrottleState::new(60);
    let result =
        route_thing_event(&fx.pool, &throttle, None, &fx.bus, actor, warning_event()).await;
    assert!(
        !result.malformed && !result.throttled && !result.unknown_event,
        "route failed: {result:?}"
    );
}

async fn run_count(pool: &sqlx::SqlitePool, dedup_key: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_runs WHERE workspace_id = ? AND dedup_key = ?",
    )
    .bind(WS)
    .bind(dedup_key)
    .fetch_one(pool)
    .await
    .expect("count runs")
}

async fn wait_subscribed(bus: &ThingEventBus) {
    for _ in 0..10_000 {
        if bus.receiver_count() > 0 {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("thing-event trigger did not subscribe");
}

/// 真实时间轮询（20ms × 500 = 10s 上限），等异步链路收敛。
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

fn run_count_is(
    pool: &sqlx::SqlitePool,
    dedup_key: &str,
    expected: i64,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> {
    let pool = pool.clone();
    let key = dedup_key.to_string();
    Box::pin(async move { run_count(&pool, &key).await == expected })
}

// ── 1. full link: warning event → wake → run → policy-gated action ──
//    → simulated driver dispatch → RunReport verified=true ────────────

#[tokio::test]
async fn warning_event_runs_full_loop_and_persists_verified_report() {
    let fx = fixture("loop_full_link").await;
    fx.manager.start(WS);
    wait_subscribed(&fx.bus).await;

    route(&fx, "device").await;

    // 150ms 合并窗口到期 → 唤醒 → Run 全链路（无定时器依赖，轮询收敛）。
    wait_for("run persisted", || run_count_is(&fx.pool, EVENT_KEY, 1)).await;

    // T12 落库：acted + verified（invoke 后 read_property 回读）。
    let row = sqlx::query(
        "SELECT id, outcome, verified, summary, trigger_type, trigger_context, report FROM agent_runs WHERE dedup_key = ?",
    )
    .bind(EVENT_KEY)
    .fetch_one(&fx.pool)
    .await
    .expect("run row");
    let run_id = row.get::<String, _>("id");
    assert_eq!(row.get::<String, _>("outcome"), "acted");
    assert!(row.get::<bool, _>("verified"), "read_property 回读 → verified=true");
    assert_eq!(row.get::<String, _>("summary"), "已开启风扇并确认温度回落");
    assert_eq!(row.get::<String, _>("trigger_type"), "thing");
    assert_eq!(row.get::<String, _>("trigger_context"), EVENT_KEY);

    // 动作清单：set_fan 经策略门下发到模拟驱动（DataServer 缺省 → simulated）。
    let report: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("report")).expect("report json");
    assert_eq!(report["action_count"], 1);
    assert_eq!(report["actions"][0]["action_name"], "set_fan");
    assert_eq!(report["actions"][0]["thing_id"], THING);
    assert_eq!(report["actions"][0]["verified"], true);
    let status = report["actions"][0]["result"]["success"]["status"].as_str().unwrap_or_default();
    assert!(
        status == "simulated" || status == "executed",
        "命令必须真实下发（模拟驱动），got: {status}"
    );

    // T6 硬交接：agent 动作以 actor='agent' 落 events 表。
    let (actor, subtype): (String, String) = sqlx::query_as(
        "SELECT actor, event_subtype FROM events WHERE device_id = ? AND actor = 'agent'",
    )
    .bind(THING)
    .fetch_one(&fx.pool)
    .await
    .expect("agent action event");
    assert_eq!(actor, "agent");
    assert_eq!(subtype, "set_fan");

    // 共振防护：agent 自己产生的事件（set_fan 动作回写）没有引发第二次唤醒。
    assert_eq!(run_count(&fx.pool, EVENT_KEY).await, 1, "agent 动作事件不得再唤醒");

    // T13 回推：无用户会话、无 admin 活跃会话 → 告警回退（events 表，按
    // run_id 定位，排除定时巡检 run 的同类告警）。
    let alerts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_subtype = 'thing_agent_alert' AND content LIKE '%' || ? || '%'",
    )
    .bind(&run_id)
    .fetch_one(&fx.pool)
    .await
    .expect("count alerts");
    assert_eq!(alerts, 1, "无会话时 run 报告应回退为告警");

    // 显式共振防护：actor='agent' 的 warning 事件（即便同名同级别）不唤醒。
    // 等待时长 > 合并窗口 + run 耗时，足以暴露一次错误唤醒。
    route(&fx, "agent").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        run_count(&fx.pool, EVENT_KEY).await,
        1,
        "actor=agent 的 warning 事件不得唤醒（O21）"
    );

    // 总数断言（排除已知的定时巡检 run）：agent 动作/事件若误唤醒，run 会
    // 落在别的 dedup_key 上，只有总数断言才能封死共振防护。
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_runs WHERE workspace_id = ? AND (dedup_key IS NULL OR dedup_key != ?)",
    )
    .bind(WS)
    .bind(format!("timer:{WS}"))
    .fetch_one(&fx.pool)
    .await
    .expect("count all non-timer runs");
    assert_eq!(total, 1, "全表仅事件 run 一行：任何 key 上都不得有误唤醒");

    assert!(fx.provider.call_count() >= 3, "LLM 至少三轮：invoke → read → 总结");
}

// ── 2. merge window: 5 events in 30s → exactly 1 wake ───────────────

#[tokio::test]
async fn five_events_in_30s_merge_into_one_wake() {
    let fx = fixture("loop_merge").await;
    fx.manager.start(WS);
    wait_subscribed(&fx.bus).await;

    for _ in 0..5 {
        route(&fx, "device").await;
    }

    wait_for("merged run", || run_count_is(&fx.pool, EVENT_KEY, 1)).await;

    // 窗口已关闭：等待远超一个窗口周期，没有重复唤醒。
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(run_count(&fx.pool, EVENT_KEY).await, 1, "30s 内 5 事件仅 1 次唤醒");
}

// ── 3. user directive → run → push_chat_message 回推（T13/T14）───────

#[tokio::test]
async fn user_directive_runs_and_pushes_assistant_message() {
    let fx = fixture("loop_directive").await;
    fx.manager.start(WS);

    const SESSION: &str = "agent:ws-loop:default/s1";
    crate::modules::agent::chat::history::ensure_session(&fx.pool, SESSION, WS, "default")
        .await
        .expect("ensure session");

    let sink: &dyn DirectiveSink = fx.manager.as_ref();
    sink.enqueue(WakeSignal {
        workspace_id: WS.to_string(),
        priority: tinyiothub_ai::thing_agent::Priority::High,
        source: TriggerSource::UserDirective {
            user_id: "u1".to_string(),
            text: "把车间温度降到 26 度".to_string(),
            session_key: Some(SESSION.to_string()),
            source: None,
        },
        dedup_key: None,
    })
    .expect("directive accepted");

    // 用户指令不进合并窗口 → 立即执行。链路全是唤醒驱动（无定时器），
    // yield 轮询即可（暂停时钟下 sleep 轮询反而会跳进 30s 巡检窗口）。
    let mut pushed = false;
    for _ in 0..20_000 {
        let messages = crate::modules::agent::chat::history::list_messages(&fx.pool, SESSION, 10)
            .await
            .expect("list messages");
        if messages.iter().any(|(role, _)| role == "assistant") {
            pushed = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(pushed, "directive run must push an assistant message");

    let messages = crate::modules::agent::chat::history::list_messages(&fx.pool, SESSION, 10)
        .await
        .expect("list messages");
    let assistant =
        messages.iter().find(|(role, _)| role == "assistant").expect("assistant message");
    assert!(assistant.1.contains("done"), "回推内容含结果摘要: {}", assistant.1);
    assert!(assistant.1.contains("已验证"), "回推内容含 verified 徽标: {}", assistant.1);

    let (trigger_type, outcome): (String, String) =
        sqlx::query_as("SELECT trigger_type, outcome FROM agent_runs WHERE workspace_id = ? AND trigger_type = 'user'")
            .bind(WS)
            .fetch_one(&fx.pool)
            .await
            .expect("user run row");
    assert_eq!(trigger_type, "user");
    assert_eq!(outcome, "no_action_needed");

    // 未知工作区指令 → Closed（端点映射为错误而非静默）。
    let err = sink.enqueue(WakeSignal {
        workspace_id: "ws-nope".to_string(),
        priority: tinyiothub_ai::thing_agent::Priority::High,
        source: TriggerSource::UserDirective {
            user_id: "u1".to_string(),
            text: "hi".to_string(),
            session_key: None,
            source: None,
        },
        dedup_key: None,
    });
    assert_eq!(err, Err(EnqueueError::Closed));
}
