//! Task 14（Q1/F2）：shadow 双跑等价对照——确定性 harness 内两路径共用同一
//! scripted LLM 输出（确定性接缝在 mock pool / scripted provider），白名单比对
//! status / summary / proposals 工具名集合 / trace 工具名序列，并锁定 shadow 期
//! 「统一路径只产报告」的出口形态。
//!
//! 白名单语义与生产观测面（apps/cloud shadow_observer.rs `compare_pair`，T13）
//! 对齐：新路径 report.summary 内嵌 healing report JSON，经 `parse_healing_report`
//! 还原 HeartbeatStatus / summary / proposals 后与旧路径 HeartbeatResult 逐项比对；
//! trace 比工具名有序序列；proposals 比工具名无序集合。
//!
//! 两路径装配（模板出处）：
//! - 旧路径：`run_heartbeat_tick`（loop_.rs）+ MockPool（AgentPoolLike 夹具，
//!   模板同 loop_.rs tests），结果经 AiEventPublisher → EventBus 的
//!   HeartbeatCompleted 事件捕获；
//! - 新路径：ThingAgentManager 统一循环全管线（heartbeat trigger 启动即首 tick →
//!   scheduler 合并窗 → run_pipeline → Runner），ScriptedProvider 经 rig 适配器
//!   喂同一文本（模板同 manager.rs tests 的 stub_manager/ScriptedProvider），
//!   报告经 AgentEventBus 的 RunRecorded 事件捕获（订阅先于 start，不丢事件）。

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinyiothub_agent::memory::metrics::Metrics;
use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
use tinyiothub_agent::port::runtime::AgentLoopConfig;
use tinyiothub_agent::runtime::agent::pool::{AgentPoolLike, AgentRunOutput};
use tinyiothub_agent::runtime::event::bus::AiEventPublisher;
use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventBus, AgentEventKind};
use tinyiothub_agent::runtime::heartbeat::loop_::run_heartbeat_tick;
use tinyiothub_agent::runtime::heartbeat::report::parse_healing_report;
use tinyiothub_agent::runtime::thing_agent::registry::RunRegistry;
use tinyiothub_agent::runtime::thing_agent::runner::{AgentHandle, RunContextInner};
use tinyiothub_agent::runtime::thing_agent::traits::{
    AutonomyPolicyReader, NoopTicketResolutions, ThingAgentHost, ThingEventSignal,
};
use tinyiothub_agent::runtime::thing_agent::{
    AutonomousAgentProvider, HeartbeatDeps, Runner, ThingAgentManager, ThingAgentManagerConfig,
};
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatResult, HeartbeatTask, TrustConfig};
use tinyiothub_core::policy::{AutonomyMode, AutonomyPolicy};

const WS: &str = "ws_01";

// ── 旧路径夹具（模板：loop_.rs tests 的 MockPool）─────────────────────

struct MockPool {
    output: AgentRunOutput,
}

#[async_trait::async_trait]
impl AgentPoolLike for MockPool {
    async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
        Ok("agent".into())
    }
    async fn send_message(&self, _workspace_id: &str, _prompt: &str) -> anyhow::Result<AgentRunOutput> {
        Ok(self.output.clone())
    }
    async fn shutdown(&self) {}
    fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
    fn cleanup_idle(&self) -> usize {
        0
    }
}

/// 捕获 HeartbeatCompleted 事件载荷（loop_.rs tests 的 RecordingHandler 形态）。
struct RecordingHandler {
    seen: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl tinyiothub_core::event::EventHandler for RecordingHandler {
    async fn handle(&self, event: &tinyiothub_core::models::event::Event) -> tinyiothub_core::error::Result<()> {
        self.seen.lock().unwrap().push(event.content().to_plain_text());
        Ok(())
    }
    fn name(&self) -> &str {
        "recording"
    }
    fn should_handle(&self, _event: &tinyiothub_core::models::event::Event) -> bool {
        true
    }
}

fn sample_task() -> HeartbeatTask {
    HeartbeatTask {
        id: 1,
        workspace_id: WS.into(),
        priority: "high".into(),
        text: "check devices".into(),
        paused: false,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// 旧路径：同一 scripted LLM 输出喂 MockPool，跑一个 heartbeat tick，从
/// HeartbeatCompleted 事件载荷取回 HeartbeatResult。
async fn run_old_tick_with_mock(llm_output: &str) -> HeartbeatResult {
    let bus = Arc::new(tinyiothub_runtime::EventBus::new());
    let seen = Arc::new(RecordingHandler {
        seen: Mutex::new(Vec::new()),
    });
    bus.register_handler(seen.clone());
    let publisher = AiEventPublisher::new(bus);
    let metrics = Metrics::new();

    let pool: Arc<dyn AgentPoolLike> = Arc::new(MockPool {
        output: AgentRunOutput {
            text: llm_output.into(),
            tool_calls: vec![],
            trace: vec![],
        },
    });

    let task = sample_task();
    run_heartbeat_tick(WS, &[&task], &TrustConfig::default(), &pool, &publisher, &metrics)
        .await
        .expect("old tick succeeds");
    publisher.shutdown().await;

    let seen = seen.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "tick publishes exactly one HeartbeatCompleted");
    // plain text 形如 "AiEvent\n<json payload>"（RichContent 标题前缀）。
    let json_start = seen[0].find('{').expect("payload contains JSON");
    let payload: serde_json::Value = serde_json::from_str(&seen[0][json_start..]).expect("event payload is JSON");
    serde_json::from_value(payload["HeartbeatCompleted"]["result"].clone()).expect("HeartbeatResult payload")
}

// ── 新路径夹具（模板：manager.rs tests 的 ScriptedProvider/stub_manager）──

/// 固定回复 scripted LLM 输出、永不调工具；`fail` 为故障注入开关。
#[derive(Clone, Default)]
struct ScriptedProvider {
    reply: String,
    fail: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl ModelProvider for ScriptedProvider {
    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        if self.fail.load(Ordering::Relaxed) {
            anyhow::bail!("scripted provider failure (test injection)");
        }
        Ok(ChatResponse {
            text: Some(self.reply.clone()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        })
    }
}

impl Attributable for ScriptedProvider {
    fn role(&self) -> Role {
        Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
    }
    fn alias(&self) -> &str {
        "ScriptedProvider"
    }
}

/// 单一共享 loop（rig 适配器包 ScriptedProvider）的 agent provider 桩。
struct ScriptedAgentProvider {
    handle: AgentHandle,
    _dir: tempfile::TempDir,
}

impl ScriptedAgentProvider {
    fn new(llm: Arc<ScriptedProvider>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let llm_for_factory = Arc::clone(&llm);
        let cfg = AgentLoopConfig {
            model_name: "stub-model".to_string(),
            prompt_builder: tinyiothub_agent::port::prompt::SystemPromptBuilder::with_defaults(),
            tools: vec![],
            memory: Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            observer: Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            workspace_dir: dir.path().to_path_buf(),
            security_summary: None,
            provider_factory: Arc::new(move || Ok(Box::new(llm_for_factory.as_ref().clone()))),
            conversation_memory: true,
        };
        let handle = tinyiothub_agent::adapters::rig::loop_::rig_loop_factory(cfg).expect("build scripted loop");
        Self { handle, _dir: dir }
    }
}

#[async_trait::async_trait]
impl AutonomousAgentProvider for ScriptedAgentProvider {
    async fn get_or_create(
        &self,
        _workspace_id: &str,
        _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        Ok(Arc::clone(&self.handle))
    }
    fn invalidate(&self, _workspace_id: &str) {}
}

struct StubHost {
    tx: tokio::sync::broadcast::Sender<ThingEventSignal>,
}

impl StubHost {
    fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(64);
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ThingAgentHost for StubHost {
    fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
        self.tx.subscribe()
    }
    async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
        Ok(vec![])
    }
    async fn push_chat_message(&self, _session_key: &str, _content: &str, _run_id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn notify_alert(&self, _workspace_id: &str, _payload: serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }
    async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

/// Act 模式内存策略桩（配额对齐 manager.rs stub_policy_reader 播种值）。
struct StubPolicyReader;

#[async_trait::async_trait]
impl AutonomyPolicyReader for StubPolicyReader {
    async fn load_autonomy(&self, _workspace_id: &str) -> anyhow::Result<Option<AutonomyPolicy>> {
        Ok(Some(AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec![],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }))
    }
}

/// 新路径产物：RunRecorded 载荷的 RunReport + 观测证据（内存 registry 真源 +
/// 本 run 在事件总线上的全部事件种类）。
struct NewPathOutcome {
    report: RunReport,
    registry: RunRegistry,
    /// 本次观测窗口内总线上出现过的 AgentEventKind 判别名（去重、按到达序）。
    event_kinds: Vec<&'static str>,
}

/// 新路径：同一 scripted LLM 输出喂统一循环全管线。任务表播种与旧路径相同的
/// 单条任务 → heartbeat trigger 启动即首 tick（确定性：paused time + 30s 合并窗
/// advance），从 RunRecorded 事件取回 RunReport。
///
/// shadow 期出口形态断言（本 harness 的选定形态）：harness 只订阅事件总线做
/// 观测，**不接线任何 RunRecorded 落库订阅者**（生产落库订阅者是 T8 的 cloud
/// 侧组件，agent crate 内无持久化端口）；断言①报告的唯一进程内驻留是内存
/// registry 真相源，②总线上本 run 只有 RunStarted/RunRecorded 两类事件——
/// 无提案/持久化旁路事件（proposals 仅以内嵌 JSON 形态留在 summary 里）。
async fn run_new_pipeline_with_script(llm_output: &str) -> NewPathOutcome {
    let host = Arc::new(StubHost::new());
    let registry = RunRegistry::new();
    let events = Arc::new(AgentEventBus::new(64));
    let mut rx = events.subscribe();
    let llm = Arc::new(ScriptedProvider {
        reply: llm_output.to_string(),
        fail: Arc::new(AtomicBool::new(false)),
    });
    let agents = Arc::new(ScriptedAgentProvider::new(llm));
    let heartbeat = HeartbeatDeps::default();
    heartbeat.tasks.insert(WS.to_string(), vec![sample_task()]);
    let manager = Arc::new(ThingAgentManager::new(
        host,
        Arc::new(StubPolicyReader),
        agents,
        registry.clone(),
        events,
        Arc::new(Runner::new()),
        ThingAgentManagerConfig {
            timer_interval: Duration::from_secs(24 * 3600),
            min_wake_level: 3,
            merge_window: tinyiothub_agent::runtime::thing_agent::scheduler::MERGE_WINDOW,
            heartbeat_default_interval: Duration::from_secs(24 * 3600),
        },
        Arc::new(NoopTicketResolutions),
        heartbeat,
    ));
    manager.start(WS);
    assert!(manager.is_running(WS));

    // 让 trigger→forward→merger 链拾起启动 tick，再推进合并窗 flush。
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;

    // 等 heartbeat 源 run 进内存 registry 真相源（timer 源首 tick 同窗flush，
    // 按 trigger 前缀过滤——与 shadow_observer 的 trigger 全等配对同一道理）。
    let heartbeat_trigger = format!("heartbeat:{WS}");
    for _ in 0..20_000 {
        if registry.recent(WS, 10).iter().any(|r| r.trigger == heartbeat_trigger) {
            break;
        }
        tokio::task::yield_now().await;
    }

    // 排空观测窗口内的总线事件。
    let mut report = None;
    let mut event_kinds: Vec<&'static str> = Vec::new();
    let mut drain_guard = 0;
    loop {
        match rx.try_recv() {
            Ok(AgentEvent { kind, .. }) => {
                let name = match &kind {
                    AgentEventKind::RunStarted { .. } => "RunStarted",
                    AgentEventKind::RunRecorded { .. } => "RunRecorded",
                    AgentEventKind::HeartbeatResultReady { .. } => "HeartbeatResultReady",
                    AgentEventKind::TrustConfigChanged { .. } => "TrustConfigChanged",
                    AgentEventKind::HeartbeatTasksChanged { .. } => "HeartbeatTasksChanged",
                    AgentEventKind::DlqEntryAdded { .. } => "DlqEntryAdded",
                };
                if !event_kinds.contains(&name) {
                    event_kinds.push(name);
                }
                if let AgentEventKind::RunRecorded { report: r, .. } = kind
                    && r.trigger == heartbeat_trigger
                {
                    report = Some(*r);
                }
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                // registry 已见报告但事件还没排到——让出再试。
                if report.is_some() || drain_guard > 20_000 {
                    break;
                }
                drain_guard += 1;
                tokio::task::yield_now().await;
            }
            Err(e) => panic!("event bus receiver failed: {e}"),
        }
    }

    NewPathOutcome {
        report: report.expect("heartbeat RunRecorded observed on the bus"),
        registry,
        event_kinds,
    }
}

// ── 白名单比对（compare_pair 语义）──────────────────────────────────

fn proposal_tool_set(result: &HeartbeatResult) -> BTreeSet<String> {
    result.proposals.iter().map(|p| p.tool_name.clone()).collect()
}

/// 一场脚本输出的双跑对照：逐项断言白名单 + shadow 出口形态。
async fn assert_shadow_equiv(llm_output: &str) {
    let old = run_old_tick_with_mock(llm_output).await;
    let new = run_new_pipeline_with_script(llm_output).await;

    // 前置不变量：统一管线把 LLM 心跳 JSON 逐字透传进 report.summary——
    // 白名单第 1/2/3 项（经 parse_healing_report 还原）有意义的根基。
    assert_eq!(
        new.report.summary, llm_output,
        "unified pipeline must carry the LLM heartbeat JSON verbatim into report.summary"
    );

    let parsed_new = parse_healing_report(&new.report.summary, WS);

    // 白名单 1：status（新路径经内嵌 JSON 还原 HeartbeatStatus，与旧路径直接可比）。
    assert_eq!(old.status, parsed_new.status, "status drift");

    // 白名单 2：summary（确定性 harness 同一 scripted 输出 → 逐字等价；
    // 生产 observer 因真实 LLM 方差只做长度级提示）。
    assert_eq!(old.summary, parsed_new.summary, "summary drift");

    // 白名单 3：proposals 工具名集合（无序、去重）。
    let new_tools: BTreeSet<String> = parsed_new.proposals.iter().map(|p| p.tool_name.clone()).collect();
    assert_eq!(proposal_tool_set(&old), new_tools, "proposals tool-set drift");

    // 白名单 4：trace 工具名序列（有序）。harness 两侧均无工具调用（stub loop
    // 零工具 / MockPool 空轨迹）→ 双双为空；非空轨迹的有序等价由生产 shadow
    // observer（真实 LLM）覆盖。
    let old_trace: Vec<&str> = old.trace.iter().map(|t| t.name.as_str()).collect();
    let new_trace: Vec<&str> = new.report.trace.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(old_trace, new_trace, "trace tool-sequence drift");

    // 附加 parity（非白名单，T6 装配点语义对齐 loop task_count）：同播 1 条任务。
    assert_eq!(old.task_count, 1);
    assert_eq!(new.report.task_count, Some(1));

    // shadow 期出口形态：统一路径只产报告。
    assert_eq!(
        new.event_kinds,
        vec!["RunStarted", "RunRecorded"],
        "unified path emits no proposal/persistence side-channel events"
    );
    assert!(
        new.registry
            .recent(WS, 10)
            .iter()
            .any(|r| r.run_id == new.report.run_id),
        "report lives only in the in-memory registry truth source"
    );
}

/// 场景 A：正常巡检——complete、无提案、无工具调用。
#[tokio::test(start_paused = true)]
async fn shadow_equiv_complete_no_proposals() {
    let llm_output = r#"{"status":"complete","summary":"一切正常","proposals":[]}"#;
    assert_shadow_equiv(llm_output).await;
}

/// 场景 B：带提案的部分巡检——partial、两个提案工具、executed_actions 自报
/// （旧路径框架侧记录覆盖自报，不在白名单内，仅验证解析不炸）。
#[tokio::test(start_paused = true)]
async fn shadow_equiv_partial_with_proposals() {
    let llm_output = r#"{"status":"partial","summary":"一台设备温度偏高","executed_actions":[{"tool_name":"read_property","thing_id":"d1","success":true,"details":"26.5"}],"proposals":[{"tool_name":"write_properties","thing_id":"d1","summary":"调低设定温度","reason":"超阈"},{"tool_name":"reboot_thing","thing_id":"d2","summary":"重启卡死设备","reason":"无响应"}]}"#;
    assert_shadow_equiv(llm_output).await;
}
