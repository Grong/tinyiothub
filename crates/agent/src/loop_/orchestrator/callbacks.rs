//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatRunner.signal()
//! HeartbeatCompleted --> HeartbeatTaskRepository.insert_result()
//!                    --> HeartbeatBridge.dispatch_proposals() (X6, O21)
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> HeartbeatRunner.start() + ThingAgentManager.start()
//! WorkspaceDeleted    --> HeartbeatRunner.stop() + ThingAgentManager.stop()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

use crate::loop_::event::bus::AiEventPublisher;
use crate::loop_::event::dlq::DeadLetterQueue;
use crate::loop_::event::types::AiEvent;
use crate::loop_::heartbeat::repo::HeartbeatTaskRepository;
use crate::loop_::heartbeat::runner::HeartbeatRunner;
use crate::loop_::heartbeat::types::{HeartbeatResult, SignalPriority};
use crate::loop_::thing_agent::manager::ThingAgentManager;
use crate::loop_::thing_agent::report::AgentRunsRepository;
use crate::loop_::thing_agent::traits::DirectiveSink;
use crate::loop_::thing_agent::types::{Outcome, Priority, TriggerSource, WakeSignal};
use tinyiothub_memory::service::MemoryService;
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

/// X6 心跳桥（O21 裁决）：订阅既有 `AiEvent::HeartbeatCompleted`（loop_.rs
/// 零改动），从心跳报告的结构化 proposals 提取问题，按 O11 dedup 后投递
/// `UserDirective{ source: Some("heartbeat"), priority: Normal }` 给
/// thing-agent loop 处置。
///
/// problem_key 从结构化字段派生（`{tool_name}:{device_id}`），不用自由文本
/// 摘要——LLM 措辞变化会击穿去重（O21）。
pub struct HeartbeatBridge {
    runs_repo: Arc<dyn AgentRunsRepository>,
    sink: Arc<dyn DirectiveSink>,
}

/// O11 dedup 窗口：同 problem_key 6h 内的 Run 参与抑制判定；超窗旧 Run 不
/// 抑制（复发可再处置）。
pub const PROBLEM_WINDOW_HOURS: u32 = 6;
/// O11：人工 ack 后该 problem_key 的抑制时长（7 天）。
pub const ACK_SUPPRESS_HOURS: u32 = 7 * 24;

impl HeartbeatBridge {
    pub fn new(runs_repo: Arc<dyn AgentRunsRepository>, sink: Arc<dyn DirectiveSink>) -> Self {
        Self { runs_repo, sink }
    }

    /// problem_key：结构化字段 `{tool_name}:{device_id}`；无目标设备的提案
    /// 用 "-" 占位（稳定，不随措辞变化）。
    pub fn problem_key_of(proposal: &Proposal) -> String {
        format!(
            "{}:{}",
            proposal.tool_name,
            proposal.device_id.as_deref().unwrap_or("-")
        )
    }

    /// 对心跳报告中的每个 proposal 做 O11 dedup，通过则投递心跳 directive。
    /// 投递失败（队列满/节流/loop 未启动）仅记录日志——心跳 directive 不享
    /// "排队不丢"（O5），丢弃可接受。
    pub async fn dispatch_proposals(&self, workspace_id: &str, result: &HeartbeatResult) {
        for proposal in &result.proposals {
            // 心跳已裁决（approved/rejected）的提案不二次投递。
            if proposal.status != ProposalStatus::Pending {
                continue;
            }
            let problem_key = Self::problem_key_of(proposal);
            match self.should_dispatch(workspace_id, &problem_key).await {
                Ok(true) => {
                    let signal = heartbeat_directive(workspace_id, problem_key.clone(), proposal);
                    if let Err(e) = self.sink.enqueue(signal) {
                        debug!(
                            workspace_id,
                            problem_key, error = %e, "heartbeat directive not admitted (droppable by design)"
                        );
                    } else {
                        info!(
                            workspace_id,
                            problem_key, "heartbeat proposal dispatched to thing-agent loop"
                        );
                    }
                }
                Ok(false) => {
                    debug!(workspace_id, problem_key, "heartbeat proposal suppressed by O11 dedup");
                }
                Err(e) => {
                    // fail-closed：dedup 判定依据不可用时跳过投递，宁可漏报
                    // 一次也不重复打扰（下一个 tick 会再试）。
                    warn!(workspace_id, problem_key, error = %e, "dedup query failed — proposal skipped (fail-closed)");
                }
            }
        }
    }

    /// O11 dedup（6h 窗口 + 窗口内计数 + 全 outcome 覆盖 + ack 抑制 7 天）：
    /// - 7d 窗口最近一次 Run 已 ack → 跳过（6h 窗口非空时 last(7d)==last(6h)， 6h 内 acked
    ///   由本分支覆盖；6h 空而 7d 内有 acked = 复发在 ack 抑制期内）
    /// - 6h 窗口无 Run → 放行（新问题 / 超 6h 复发）
    /// - 最近一次 failed/rejected/budget_exceeded → 跳过
    /// - acted+verified / no_action_needed → 跳过
    /// - acted+未 verified：窗口内仅 1 次 → 放行一次重试；第二次起跳过
    async fn should_dispatch(&self, workspace_id: &str, problem_key: &str) -> anyhow::Result<bool> {
        if let Some((_, _, acked)) = self
            .runs_repo
            .last_problem_run(workspace_id, problem_key, ACK_SUPPRESS_HOURS)
            .await?
            && acked
        {
            return Ok(false);
        }
        let Some((outcome, verified, _acked)) = self
            .runs_repo
            .last_problem_run(workspace_id, problem_key, PROBLEM_WINDOW_HOURS)
            .await?
        else {
            return Ok(true);
        };
        match outcome {
            Outcome::Failed | Outcome::Rejected | Outcome::BudgetExceeded | Outcome::NoActionNeeded => Ok(false),
            Outcome::Acted if verified => Ok(false),
            Outcome::Acted => {
                let n = self
                    .runs_repo
                    .count_problem_runs(workspace_id, problem_key, PROBLEM_WINDOW_HOURS)
                    .await?;
                Ok(n <= 1)
            }
        }
    }
}

/// 心跳 directive 文本：从 proposal 生成可执行指令（O2）。
fn heartbeat_directive_text(problem_key: &str, proposal: &Proposal) -> String {
    format!(
        "心跳巡检发现待处置问题 {problem_key}：{}（原因：{}；风险：{}）。请诊断并处置。",
        proposal.summary, proposal.reason, proposal.risk
    )
}

/// 心跳来源 directive（O5/O24）：Normal 优先级、dedup_key=None 不参与合并、
/// source=Some("heartbeat") 标记来源（不走 60s 同文去重、不享排队不丢）。
fn heartbeat_directive(workspace_id: &str, problem_key: String, proposal: &Proposal) -> WakeSignal {
    WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority: Priority::Normal,
        source: TriggerSource::UserDirective {
            user_id: "heartbeat".to_string(),
            text: heartbeat_directive_text(&problem_key, proposal),
            session_key: None,
            source: Some("heartbeat".to_string()),
            problem_key: Some(problem_key),
        },
        dedup_key: None,
    }
}

/// Cross-domain callback handler.
pub struct AiEventHandler {
    heartbeat_runner: Arc<HeartbeatRunner>,
    task_repo: Arc<dyn HeartbeatTaskRepository>,
    memory_service: Arc<MemoryService>,
    event_publisher: Arc<AiEventPublisher>,
    dlq: Option<Arc<dyn DeadLetterQueue>>,
    /// T15 thing-agent loop registry; None where the loop is not deployed.
    thing_agent_manager: Option<Arc<ThingAgentManager>>,
    /// T18 X6 心跳桥；None 时 HeartbeatCompleted 仅落库不投递 directive。
    heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
    shutting_down: Arc<AtomicBool>,
    retry_in_flight: Arc<AtomicUsize>,
    retry_idle: Arc<Notify>,
    shutdown_notify: Arc<Notify>,
}

impl AiEventHandler {
    pub fn new(
        heartbeat_runner: Arc<HeartbeatRunner>,
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        memory_service: Arc<MemoryService>,
        event_publisher: Arc<AiEventPublisher>,
        dlq: Option<Arc<dyn DeadLetterQueue>>,
        thing_agent_manager: Option<Arc<ThingAgentManager>>,
        heartbeat_bridge: Option<Arc<HeartbeatBridge>>,
        shutting_down: Arc<AtomicBool>,
    ) -> Self {
        Self {
            heartbeat_runner,
            task_repo,
            memory_service,
            event_publisher,
            dlq,
            thing_agent_manager,
            heartbeat_bridge,
            shutting_down,
            retry_in_flight: Arc::new(AtomicUsize::new(0)),
            retry_idle: Arc::new(Notify::new()),
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        &self.heartbeat_runner
    }

    pub fn memory_service(&self) -> &Arc<MemoryService> {
        &self.memory_service
    }

    /// Handle an AiEvent variant, dispatched by the EventBus.
    pub async fn handle_ai_event(&self, event: &Event) {
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!("AiEventHandler is shutting down, skipping event");
            return;
        }

        let _ai_event_type = match event.event_type() {
            EventType::Ai(t) => t,
            _ => return,
        };

        let payload_str = match extract_payload(event.content()) {
            Some(s) => s,
            None => {
                debug!("AiEvent has no text payload -- skipping");
                return;
            }
        };

        let ai_event: AiEvent = match serde_json::from_str(&payload_str) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to deserialize AiEvent payload");
                return;
            }
        };

        match &ai_event {
            AiEvent::AlarmCreated(alarm) => {
                let severity = alarm.severity.to_lowercase();
                if severity == "critical" || severity == "error" {
                    self.heartbeat_runner.signal(crate::loop_::heartbeat::types::HeartbeatSignal {
                        workspace_id: alarm.workspace_id.clone(),
                        reason: format!("Alarm: {}", alarm.message),
                        context: format!("device_id={}, alarm_type={}", alarm.device_id, alarm.alarm_type),
                        priority: if severity == "critical" {
                            SignalPriority::Critical
                        } else {
                            SignalPriority::High
                        },
                        device_id: Some(alarm.device_id.clone()),
                        alarm_type: Some(alarm.alarm_type.clone()),
                        rule_id: alarm.rule_id.clone(),
                    });
                }
            }
            AiEvent::HeartbeatCompleted { workspace_id, result } => {
                match self.task_repo.insert_result(workspace_id, result).await {
                    Ok(_) => debug!(workspace_id, "Heartbeat result persisted"),
                    Err(e) => {
                        error!(workspace_id, error = %e, "Failed to persist heartbeat result");

                        // Retry with exponential backoff
                        self.retry_with_backoff(workspace_id, result).await;
                    }
                }
                // X6 心跳桥：落库结果如何不影响投递判定（O11 dedup 依据
                // agent_runs，不依赖本次心跳结果的持久化）。
                if let Some(bridge) = &self.heartbeat_bridge {
                    bridge.dispatch_proposals(workspace_id, result).await;
                }
            }
            AiEvent::WorkspaceCreated { workspace_id } => {
                self.heartbeat_runner.start(workspace_id).await;
                if let Some(manager) = &self.thing_agent_manager {
                    manager.start(workspace_id);
                }
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                self.heartbeat_runner.stop(workspace_id).await;
                if let Some(manager) = &self.thing_agent_manager {
                    manager.stop(workspace_id).await;
                }
            }
            // Self-referential events published by AiEventHandler itself —
            // these are intentionally no-ops to avoid processing loops.
            AiEvent::AlarmResolved { .. } => {}
            AiEvent::HeartbeatPersistFailed { .. } => {}
            AiEvent::ReflectionFailed { .. } => {}
            // ChatCompleted was previously handled here but reflection
            // now happens directly in chat/service.rs. Variant retained
            // for future EventBus-based reflection.
            AiEvent::ChatCompleted { .. } => {}
            AiEvent::ProposalCreated {
                workspace_id,
                proposal_id,
                tool_name,
            } => {
                info!(
                    workspace_id,
                    proposal_id, tool_name, "HITL proposal created — awaiting human approval"
                );
            }
            AiEvent::ProposalResolved {
                workspace_id,
                proposal_id,
                approved,
            } => {
                info!(workspace_id, proposal_id, approved, "HITL proposal resolved");
            }
        }
    }

    /// Retry heartbeat result persistence with exponential backoff.
    async fn retry_with_backoff(&self, workspace_id: &str, result: &crate::loop_::heartbeat::types::HeartbeatResult) {
        let result = result.clone();
        let ws_id = workspace_id.to_string();
        let task_repo = self.task_repo.clone();
        let event_publisher = self.event_publisher.clone();
        let dlq = self.dlq.clone();
        let shutting_down = self.shutting_down.clone();
        let shutdown_notify = self.shutdown_notify.clone();

        self.retry_in_flight.fetch_add(1, Ordering::SeqCst);
        let tracker = RetryTracker {
            count: self.retry_in_flight.clone(),
            idle: self.retry_idle.clone(),
        };

        tokio::spawn(async move {
            // Decrements the in-flight count on every exit path, waking drainers.
            let _tracker = tracker;
            let mut attempt: u32 = 0;
            let max_attempts: u32 = 5;
            let base_delay = Duration::from_secs(2);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(base_delay * 2u32.pow(attempt)) => {}
                    _ = shutdown_notify.notified() => {
                        debug!(ws_id, "Shutting down, aborting retry");
                        return;
                    }
                }
                if shutting_down.load(Ordering::SeqCst) {
                    debug!(ws_id, "Shutting down, aborting retry");
                    return;
                }
                match task_repo.insert_result(&ws_id, &result).await {
                    Ok(_) => {
                        debug!(ws_id, attempt, "Heartbeat result persisted on retry");
                        return;
                    }
                    Err(e) => {
                        attempt += 1;
                        if attempt >= max_attempts {
                            error!(
                                workspace_id = %ws_id,
                                attempts = attempt,
                                error = %e,
                                "Heartbeat persist exhausted retries, enqueuing to DLQ"
                            );
                            if let Err(dlq_err) = dead_letter_heartbeat_result(
                                dlq.as_ref(),
                                &event_publisher,
                                &ws_id,
                                &result,
                                &e.to_string(),
                                max_attempts,
                            )
                            .await
                            {
                                error!(
                                    workspace_id = %ws_id,
                                    error = %dlq_err,
                                    "DLQ enqueue failed — heartbeat result lost"
                                );
                            }
                            return;
                        }
                        warn!(ws_id, attempt, error = %e, "Heartbeat persist retry");
                    }
                }
            }
        });
    }

    /// Number of retry tasks currently alive (sleeping or persisting).
    pub fn in_flight_retries(&self) -> usize {
        self.retry_in_flight.load(Ordering::SeqCst)
    }

    /// Abort in-flight retry backoff sleeps and wait for the tasks to exit.
    pub async fn drain_retries(&self) {
        self.shutdown_notify.notify_waiters();
        loop {
            if self.retry_in_flight.load(Ordering::SeqCst) == 0 {
                return;
            }
            let notified = self.retry_idle.notified();
            tokio::pin!(notified);
            // Register the waiter before re-checking the count, otherwise a
            // task finishing between check and await would be missed.
            notified.as_mut().enable();
            if self.retry_in_flight.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }
}

/// Decrements the in-flight retry counter when the task exits, on any path,
/// and wakes drainers once the last task is gone.
struct RetryTracker {
    count: Arc<AtomicUsize>,
    idle: Arc<Notify>,
}

impl Drop for RetryTracker {
    fn drop(&mut self) {
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.idle.notify_waiters();
        }
    }
}

#[async_trait]
impl tinyiothub_core::event::EventHandler for AiEventHandler {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        self.handle_ai_event(event).await;
        Ok(())
    }

    fn name(&self) -> &str {
        "AiEventHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Ai(_))
    }
}

/// Dead-letter a heartbeat result after persist retries are exhausted.
///
/// Always publishes HeartbeatPersistFailed — even when the DLQ itself rejects
/// the entry, operators need the failure signal. Returns Err when the DLQ
/// enqueue failed so the caller can log it; swallowing it would lose the
/// result silently.
async fn dead_letter_heartbeat_result(
    dlq: Option<&Arc<dyn DeadLetterQueue>>,
    event_publisher: &AiEventPublisher,
    ws_id: &str,
    result: &crate::loop_::heartbeat::types::HeartbeatResult,
    last_error: &str,
    max_attempts: u32,
) -> Result<(), String> {
    let mut enqueue_result = Ok(());
    if let Some(dlq) = dlq {
        enqueue_result = dlq
            .enqueue(ws_id, "HeartbeatCompleted", &dlq_payload(result), last_error)
            .await;
    }
    event_publisher.publish(AiEvent::HeartbeatPersistFailed {
        workspace_id: ws_id.to_string(),
        reason: format!("Failed after {} attempts: {}", max_attempts, last_error),
    });
    enqueue_result
}

/// HeartbeatResult serialization cannot fail in practice, but an empty
/// payload would make the DLQ entry useless — fall back to a placeholder
/// that at least preserves the workspace and error context.
fn dlq_payload(result: &crate::loop_::heartbeat::types::HeartbeatResult) -> String {
    serde_json::to_string(result).unwrap_or_else(|e| {
        format!(
            r#"{{"unserializable":true,"workspace_id":"{}","serialize_error":"{}"}}"#,
            result.workspace_id, e
        )
    })
}

fn extract_payload(content: &RichContent) -> Option<String> {
    content.elements().iter().find_map(|el| match el {
        ContentElement::Text { content, .. } => Some(content.clone()),
        _ => None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent};
    use tinyiothub_runtime::EventBus;

    use tinyiothub_core::event::EventHandler;

    use crate::loop_::event::dlq::{DeadLetterEntry, DeadLetterQueue};
    use crate::loop_::heartbeat::repo::RepoError;
    use crate::loop_::heartbeat::types::{HeartbeatConfig, HeartbeatResult, HeartbeatStatus, HeartbeatTask};
    use tinyiothub_llm::provider::{LlmProvider, LlmResponse};
    use tinyiothub_memory::service::MemoryService;

    pub(crate) struct MockTaskRepo {
        pub(crate) fail_insert: bool,
        insert_result_calls: Arc<Mutex<Vec<(String, HeartbeatResult)>>>,
    }

    impl MockTaskRepo {
        pub(crate) fn new() -> Self {
            Self {
                fail_insert: false,
                insert_result_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Every insert_result fails — drives the retry/backoff path.
        pub(crate) fn failing() -> Self {
            Self {
                fail_insert: true,
                insert_result_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub(crate) fn insert_result_calls(&self) -> Arc<Mutex<Vec<(String, HeartbeatResult)>>> {
            Arc::clone(&self.insert_result_calls)
        }
    }

    #[async_trait::async_trait]
    impl crate::loop_::heartbeat::repo::HeartbeatTaskRepository for MockTaskRepo {
        async fn list_by_workspace(&self, _workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
            Ok(vec![])
        }

        async fn upsert(
            &self,
            _workspace_id: &str,
            _task: &HeartbeatTask,
            _expected_version: i64,
        ) -> Result<bool, RepoError> {
            Ok(true)
        }

        async fn insert(&self, _workspace_id: &str, _priority: &str, _text: &str) -> Result<HeartbeatTask, RepoError> {
            Err(RepoError::Database("mock".into()))
        }

        async fn set_paused(&self, _workspace_id: &str, _task_id: i64, _paused: bool) -> Result<(), RepoError> {
            Ok(())
        }

        async fn delete(&self, _workspace_id: &str, _task_id: i64) -> Result<(), RepoError> {
            Ok(())
        }

        async fn insert_result(&self, workspace_id: &str, result: &HeartbeatResult) -> Result<(), RepoError> {
            self.insert_result_calls
                .lock()
                .unwrap()
                .push((workspace_id.to_string(), result.clone()));
            if self.fail_insert {
                return Err(RepoError::Database("mock insert failure".into()));
            }
            Ok(())
        }
    }

    struct MockLlmProvider;

    #[async_trait::async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn chat(
            &self,
            _system: Option<&str>,
            _prompt: &str,
            _model: &str,
            _temperature: f32,
        ) -> anyhow::Result<LlmResponse> {
            anyhow::bail!("mock")
        }
    }

    struct MockMemoryStore;

    #[async_trait::async_trait]
    impl tinyiothub_core::memory::MemoryStore for MockMemoryStore {
        async fn put(
            &self,
            _input: tinyiothub_core::memory::MemoryInput,
        ) -> tinyiothub_core::error::Result<tinyiothub_core::memory::AgentMemory> {
            unimplemented!()
        }

        async fn get(&self, _id: &str) -> tinyiothub_core::error::Result<Option<tinyiothub_core::memory::AgentMemory>> {
            Ok(None)
        }

        async fn get_all(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }

        async fn list_active(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }

        async fn get_since(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
            _since: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }

        async fn set_pinned(&self, _id: &str, _pinned: bool) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }

        async fn record_load(&self, _id: &str) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }

        async fn record_reference(&self, _id: &str) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }

        async fn get_pending_queue(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::ReflectionQueueItem>> {
            Ok(vec![])
        }

        async fn resolve_queue_item(
            &self,
            _id: &str,
            _workspace_id: &str,
            _approved: bool,
            _reviewer_note: Option<&str>,
        ) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }

        async fn enqueue_candidate(
            &self,
            _item: tinyiothub_core::memory::QueueCandidateInput,
        ) -> tinyiothub_core::error::Result<String> {
            Ok("mock_id".into())
        }

        async fn count_by_source(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
            _source: tinyiothub_core::memory::MemorySource,
        ) -> tinyiothub_core::error::Result<u64> {
            Ok(0)
        }
    }

    pub(crate) fn make_memory_service() -> Arc<MemoryService> {
        Arc::new(MemoryService::new(Arc::new(MockLlmProvider), Arc::new(MockMemoryStore)))
    }

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(EventBus::new())))
    }

    fn make_heartbeat_runner(
        task_repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository>,
    ) -> Arc<HeartbeatRunner> {
        Arc::new(HeartbeatRunner::new(
            task_repo,
            make_publisher(),
            HeartbeatConfig::default(),
        ))
    }

    /// Wrap an AiEvent inside a tinyiothub_core Event for handler dispatch.
    fn wrap_ai_event(ai_event: &AiEvent) -> Event {
        let payload = serde_json::to_string(ai_event).unwrap();
        let ai_event_type: tinyiothub_core::models::event::AiEventType = ai_event.into();
        Event::new(
            EventType::Ai(ai_event_type),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("AiEvent".to_string(), payload),
        )
        .expect("Failed to create test event")
    }

    #[tokio::test]
    async fn test_handler_construction() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(handler.name(), "AiEventHandler");
    }

    #[tokio::test]
    async fn test_should_handle_filters_ai_events() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        let ai_event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_1".into(),
        });
        assert!(handler.should_handle(&ai_event));

        // System event should not be handled
        let system_event = Event::new(
            EventType::System(tinyiothub_core::models::event::SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "data".to_string()),
        )
        .expect("Failed to create system event");
        assert!(!handler.should_handle(&system_event));
    }

    #[tokio::test]
    async fn test_heartbeat_completed_inserts_result() {
        let repo = Arc::new(MockTaskRepo::new());
        let insert_calls = repo.insert_result_calls();
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = repo;
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            Arc::clone(&repo),
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        let result = HeartbeatResult {
            workspace_id: "ws_test".to_string(),
            status: HeartbeatStatus::Complete,
            summary: "All good".to_string(),
            task_count: 0,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        };

        let ai_event = AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result: result.clone(),
        };

        let event = wrap_ai_event(&ai_event);
        handler.handle_ai_event(&event).await;

        let calls = insert_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "ws_test");
        assert_eq!(calls[0].1.workspace_id, "ws_test");
        assert_eq!(calls[0].1.status, HeartbeatStatus::Complete);
    }

    #[tokio::test]
    async fn test_alarm_created_non_critical_no_signal() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        // Non-critical alarm should not trigger heartbeat signal
        let alarm = AiEvent::AlarmCreated(tinyiothub_event::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws_1".into(),
            device_id: "d1".into(),
            alarm_type: "high_temp".into(),
            severity: "warning".into(),
            message: "Temperature is high".into(),
            rule_id: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        });

        let event = wrap_ai_event(&alarm);
        handler.handle_ai_event(&event).await;
        // No assertion needed — just verifying no panic for non-critical alarms
    }

    #[tokio::test]
    async fn test_workspace_created_and_deleted_no_panic() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        // WorkspaceCreated (no tasks loaded → loop won't start, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;

        // WorkspaceDeleted (no loop running → no-op, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;
    }

    // T15: workspace lifecycle events start/stop the thing-agent loop
    // alongside the heartbeat runner.
    #[tokio::test]
    async fn test_workspace_lifecycle_drives_thing_agent_manager() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();
        let parts = crate::loop_::thing_agent::manager::tests::stub_manager();
        let manager = parts.manager.clone();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            Some(manager.clone()),
            None,
            Arc::new(AtomicBool::new(false)),
        );

        assert!(!manager.is_running("ws_life"));
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(manager.is_running("ws_life"), "WorkspaceCreated must start the loop");

        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_life".into(),
        });
        handler.handle_ai_event(&event).await;
        assert!(!manager.is_running("ws_life"), "WorkspaceDeleted must stop the loop");
    }

    #[tokio::test]
    async fn test_self_referential_events_are_noop() {
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            repo,
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        // Self-referential events should not panic
        for event_variant in [
            AiEvent::AlarmResolved {
                alarm_id: "a1".into(),
                device_id: "d1".into(),
                rule_id: None,
            },
            AiEvent::HeartbeatPersistFailed {
                workspace_id: "ws_1".into(),
                reason: "test".into(),
            },
            AiEvent::ReflectionFailed {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                reason: "test".into(),
            },
            AiEvent::ChatCompleted {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                model: "gpt-4".into(),
                messages: vec![],
            },
        ] {
            let event = wrap_ai_event(&event_variant);
            handler.handle_ai_event(&event).await;
        }
    }

    #[tokio::test]
    async fn test_shutting_down_skips_handling() {
        let repo = Arc::new(MockTaskRepo::new());
        let insert_calls = repo.insert_result_calls();
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = repo;
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            Arc::clone(&repo),
            memory,
            publisher,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(true)), // shutting_down = true
        );

        let result = HeartbeatResult {
            workspace_id: "ws_test".to_string(),
            status: HeartbeatStatus::Complete,
            summary: "All good".to_string(),
            task_count: 0,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        };

        let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result,
        });
        handler.handle_ai_event(&event).await;

        // insert_result should NOT have been called because shutting_down is true
        let calls = insert_calls.lock().unwrap();
        assert!(calls.is_empty());
    }

    #[tokio::test]
    async fn test_extract_payload_from_text() {
        let content = RichContent::new_text("Test event".to_string(), r#"{"key":"value"}"#.to_string());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(r#"{"key":"value"}"#.to_string()));
    }

    #[tokio::test]
    async fn test_extract_payload_empty_content() {
        let content = RichContent::new_text("Test".to_string(), String::new());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(String::new()));
    }

    #[derive(Default)]
    struct MockDlq {
        fail: bool,
        entries: Mutex<Vec<(String, String, String, String)>>,
    }

    #[async_trait::async_trait]
    impl DeadLetterQueue for MockDlq {
        async fn enqueue(
            &self,
            workspace_id: &str,
            event_type: &str,
            payload_json: &str,
            failure_reason: &str,
        ) -> Result<(), String> {
            if self.fail {
                return Err("dlq unavailable".into());
            }
            self.entries.lock().unwrap().push((
                workspace_id.to_string(),
                event_type.to_string(),
                payload_json.to_string(),
                failure_reason.to_string(),
            ));
            Ok(())
        }

        async fn list(&self, _workspace_id: &str) -> Result<Vec<DeadLetterEntry>, String> {
            Ok(vec![])
        }

        async fn discard(&self, _entry_id: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingHandler {
        seen: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl EventHandler for RecordingHandler {
        async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
            self.seen.lock().unwrap().push(event.content().to_plain_text());
            Ok(())
        }
        fn name(&self) -> &str {
            "recording"
        }
        fn should_handle(&self, _event: &Event) -> bool {
            true
        }
    }

    fn recording_publisher() -> (Arc<AiEventPublisher>, Arc<RecordingHandler>) {
        let bus = Arc::new(EventBus::new());
        let seen = Arc::new(RecordingHandler::default());
        bus.register_handler(seen.clone());
        (Arc::new(AiEventPublisher::new(bus)), seen)
    }

    fn sample_result() -> HeartbeatResult {
        HeartbeatResult {
            workspace_id: "ws_1".to_string(),
            status: HeartbeatStatus::Complete,
            summary: "done".to_string(),
            task_count: 1,
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        }
    }

    #[tokio::test]
    async fn dead_letter_enqueue_failure_is_returned_not_swallowed() {
        let (publisher, seen) = recording_publisher();
        let dlq: Arc<dyn DeadLetterQueue> = Arc::new(MockDlq {
            fail: true,
            entries: Mutex::new(vec![]),
        });

        let r = dead_letter_heartbeat_result(Some(&dlq), &publisher, "ws_1", &sample_result(), "db down", 5).await;
        publisher.shutdown().await;

        assert!(r.is_err(), "DLQ enqueue failure must surface so the caller can log it");
        let seen = seen.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "HeartbeatPersistFailed must still be published");
        assert!(seen[0].contains("ws_1"));
    }

    #[tokio::test]
    async fn dead_letter_records_entry_with_full_payload() {
        let (publisher, _seen) = recording_publisher();
        let mock = Arc::new(MockDlq::default());
        let dlq: Arc<dyn DeadLetterQueue> = mock.clone();

        let r = dead_letter_heartbeat_result(Some(&dlq), &publisher, "ws_1", &sample_result(), "db down", 5).await;
        publisher.shutdown().await;

        assert!(r.is_ok());
        let entries = mock.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "ws_1");
        assert_eq!(entries[0].1, "HeartbeatCompleted");
        assert!(
            entries[0].2.contains("ws_1"),
            "payload must carry the result JSON, not an empty string"
        );
        assert!(entries[0].2.contains("done"));
        assert_eq!(entries[0].3, "db down");
    }

    #[tokio::test]
    async fn dead_letter_without_dlq_still_publishes_failure_event() {
        let (publisher, seen) = recording_publisher();

        let r = dead_letter_heartbeat_result(None, &publisher, "ws_1", &sample_result(), "db down", 5).await;
        publisher.shutdown().await;

        assert!(r.is_ok());
        let seen = seen.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].contains("ws_1"));
    }

    // ── T18 X6 心跳桥 ──────────────────────────────────────────

    mod heartbeat_bridge {
        use super::*;
        use crate::loop_::thing_agent::report::AgentRunsRepository;
        use crate::loop_::thing_agent::scheduler::{EnqueueError, Scheduler};
        use crate::loop_::thing_agent::traits::DirectiveSink;
        use crate::loop_::thing_agent::types::{Outcome, Priority, RunReport, TriggerSource, WakeSignal};
        use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

        /// 内存 run 集：(outcome, verified, acked, age_hours)。窗口语义与
        /// Sqlite 实现一致（严格小于窗口不计入边界外）。
        struct MemRuns {
            runs: Vec<(Outcome, bool, bool, u32)>,
            fail: bool,
        }

        impl MemRuns {
            fn new(runs: Vec<(Outcome, bool, bool, u32)>) -> Self {
                Self { runs, fail: false }
            }

            fn failing() -> Self {
                Self {
                    runs: vec![],
                    fail: true,
                }
            }
        }

        #[async_trait::async_trait]
        impl AgentRunsRepository for MemRuns {
            async fn insert_run(
                &self,
                _report: &RunReport,
                _problem_key: Option<&str>,
                _dedup_key: Option<&str>,
            ) -> anyhow::Result<()> {
                Ok(())
            }

            async fn recent_summaries(&self, _workspace_id: &str, _limit: u32) -> anyhow::Result<Vec<String>> {
                Ok(vec![])
            }

            async fn history_by_dedup_key(
                &self,
                _workspace_id: &str,
                _key: &str,
                _limit: u32,
            ) -> anyhow::Result<Vec<String>> {
                Ok(vec![])
            }

            async fn recent_runs_by_dedup_key(
                &self,
                _workspace_id: &str,
                _key: &str,
                _limit: u32,
            ) -> anyhow::Result<Vec<RunReport>> {
                Ok(vec![])
            }

            async fn ack_run(&self, _run_id: &str, _actor: &str) -> anyhow::Result<bool> {
                Ok(false)
            }

            async fn last_problem_run(
                &self,
                _workspace_id: &str,
                _problem_key: &str,
                since_hours: u32,
            ) -> anyhow::Result<Option<(Outcome, bool, bool)>> {
                if self.fail {
                    anyhow::bail!("repo down");
                }
                Ok(self
                    .runs
                    .iter()
                    .filter(|(_, _, _, age)| *age < since_hours)
                    .min_by_key(|(_, _, _, age)| *age)
                    .map(|(o, v, a, _)| (*o, *v, *a)))
            }

            async fn count_problem_runs(
                &self,
                _workspace_id: &str,
                _problem_key: &str,
                since_hours: u32,
            ) -> anyhow::Result<u32> {
                if self.fail {
                    anyhow::bail!("repo down");
                }
                Ok(self.runs.iter().filter(|(_, _, _, age)| *age < since_hours).count() as u32)
            }
        }

        #[derive(Default)]
        struct RecordingSink {
            signals: Mutex<Vec<WakeSignal>>,
        }

        impl DirectiveSink for RecordingSink {
            fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
                self.signals.lock().unwrap().push(signal);
                Ok(())
            }
        }

        fn proposal(tool_name: &str, device_id: Option<&str>) -> Proposal {
            Proposal {
                id: "p1".into(),
                workspace_id: "ws_1".into(),
                agent_id: "hb".into(),
                tool_name: tool_name.into(),
                device_id: device_id.map(str::to_string),
                summary: "车间温度超过阈值 30°C".into(),
                reason: "连续 3 次采样超限".into(),
                risk: "medium".into(),
                parameters: None,
                created_at: "2026-08-03T00:00:00Z".into(),
                status: ProposalStatus::Pending,
            }
        }

        fn result_with(proposals: Vec<Proposal>) -> HeartbeatResult {
            HeartbeatResult {
                workspace_id: "ws_1".into(),
                status: HeartbeatStatus::Complete,
                summary: "tick done".into(),
                task_count: 1,
                executed_actions: vec![],
                proposals,
                error: None,
            }
        }

        fn bridge(runs: MemRuns) -> (HeartbeatBridge, Arc<RecordingSink>) {
            let sink = Arc::new(RecordingSink::default());
            (HeartbeatBridge::new(Arc::new(runs), sink.clone()), sink)
        }

        async fn dispatched(runs: Vec<(Outcome, bool, bool, u32)>) -> Arc<RecordingSink> {
            let (bridge, sink) = bridge(MemRuns::new(runs));
            bridge
                .dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            sink
        }

        #[test]
        fn problem_key_uses_structured_fields_not_free_text() {
            let p = proposal("set_hvac", Some("dev-1"));
            assert_eq!(HeartbeatBridge::problem_key_of(&p), "set_hvac:dev-1");
            // 无目标设备的提案用稳定占位符，不含摘要/原因等自由文本。
            let p = proposal("set_hvac", None);
            assert_eq!(HeartbeatBridge::problem_key_of(&p), "set_hvac:-");
        }

        #[tokio::test]
        async fn no_prior_run_dispatches_with_heartbeat_directive_shape() {
            let sink = dispatched(vec![]).await;
            let signals = sink.signals.lock().unwrap();
            assert_eq!(signals.len(), 1);
            let sig = &signals[0];
            assert_eq!(sig.workspace_id, "ws_1");
            // O5/O24：心跳来源 directive 降 Normal、不参与合并、标记来源。
            assert_eq!(sig.priority, Priority::Normal);
            assert_eq!(sig.dedup_key, None, "心跳 directive 不进合并窗口");
            match &sig.source {
                TriggerSource::UserDirective {
                    user_id,
                    text,
                    session_key,
                    source,
                    problem_key,
                } => {
                    assert_eq!(user_id, "heartbeat");
                    assert_eq!(source.as_deref(), Some("heartbeat"));
                    assert_eq!(problem_key.as_deref(), Some("set_hvac:dev-1"));
                    assert_eq!(*session_key, None);
                    assert!(text.contains("set_hvac:dev-1"), "指令文本携带 problem：{text}");
                    assert!(text.contains("车间温度超过阈值"), "指令文本携带摘要：{text}");
                    assert!(text.contains("请诊断并处置"), "指令文本可执行：{text}");
                }
                other => panic!("expected UserDirective, got {other:?}"),
            }
        }

        // O11 全 outcome 矩阵：窗口内最近一次 run 的 outcome 决定是否抑制。
        #[tokio::test]
        async fn outcome_matrix_suppresses_and_dispatches() {
            // 抑制：failed / rejected / budget_exceeded / no_action_needed /
            // acted+verified（各一例，年龄 1h 在 6h 窗口内）。
            for (outcome, verified) in [
                (Outcome::Failed, false),
                (Outcome::Rejected, false),
                (Outcome::BudgetExceeded, false),
                (Outcome::NoActionNeeded, false),
                (Outcome::Acted, true),
            ] {
                let sink = dispatched(vec![(outcome, verified, false, 1)]).await;
                assert!(
                    sink.signals.lock().unwrap().is_empty(),
                    "{outcome:?} (verified={verified}) must suppress"
                );
            }
        }

        #[tokio::test]
        async fn acted_unverified_allows_exactly_one_retry_in_window() {
            // 窗口内仅 1 次 acted+未 verified → 放行一次重试。
            let sink = dispatched(vec![(Outcome::Acted, false, false, 1)]).await;
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "first retry allowed");
            // 窗口内已有 2 次 → 第二次起跳过。
            let sink = dispatched(vec![
                (Outcome::Acted, false, false, 1),
                (Outcome::Acted, false, false, 2),
            ])
            .await;
            assert!(sink.signals.lock().unwrap().is_empty(), "second retry suppressed");
        }

        #[tokio::test]
        async fn recurrence_beyond_6h_window_dispatches_again() {
            // 超 6h 旧 Run 不抑制：7h 前 acted+verified → 放行。
            let sink = dispatched(vec![(Outcome::Acted, true, false, 7)]).await;
            assert_eq!(
                sink.signals.lock().unwrap().len(),
                1,
                "recurrence after 6h must dispatch"
            );
        }

        #[tokio::test]
        async fn ack_suppresses_for_7_days() {
            // ack 抑制：6h 窗口内 acked → 跳过。
            let sink = dispatched(vec![(Outcome::Acted, true, true, 1)]).await;
            assert!(sink.signals.lock().unwrap().is_empty(), "acked within 6h suppressed");
            // 6h 窗口外、7 天内 acked（72h）→ 仍跳过（复发在 ack 抑制期内）。
            let sink = dispatched(vec![(Outcome::Acted, true, true, 72)]).await;
            assert!(sink.signals.lock().unwrap().is_empty(), "acked within 7d suppressed");
            // ack 超 7 天（192h）→ 抑制过期，放行。
            let sink = dispatched(vec![(Outcome::Acted, true, true, 192)]).await;
            assert_eq!(
                sink.signals.lock().unwrap().len(),
                1,
                "ack older than 7d no longer suppresses"
            );
        }

        #[tokio::test]
        async fn repo_failure_skips_dispatch_fail_closed() {
            let (bridge, sink) = bridge(MemRuns::failing());
            bridge
                .dispatch_proposals("ws_1", &result_with(vec![proposal("set_hvac", Some("dev-1"))]))
                .await;
            assert!(
                sink.signals.lock().unwrap().is_empty(),
                "dedup query failure must fail-closed (skip, not spam)"
            );
        }

        #[tokio::test]
        async fn no_proposals_dispatches_nothing() {
            let sink = dispatched(vec![]).await; // sanity: one proposal dispatches
            assert_eq!(sink.signals.lock().unwrap().len(), 1);

            let (bridge, sink) = bridge(MemRuns::new(vec![]));
            bridge.dispatch_proposals("ws_1", &result_with(vec![])).await;
            assert!(
                sink.signals.lock().unwrap().is_empty(),
                "HeartbeatCompleted without proposals must not dispatch"
            );
        }

        // 已裁决（approved/rejected）的提案不二次投递——心跳结果里混入历史
        // 提案时不得重复打扰。
        #[tokio::test]
        async fn decided_proposals_are_not_dispatched() {
            for status in [ProposalStatus::Approved, ProposalStatus::Rejected] {
                let (bridge, sink) = bridge(MemRuns::new(vec![]));
                let mut decided = proposal("set_hvac", Some("dev-1"));
                decided.status = status.clone();
                bridge.dispatch_proposals("ws_1", &result_with(vec![decided])).await;
                assert!(
                    sink.signals.lock().unwrap().is_empty(),
                    "{status:?} proposal must not be re-dispatched"
                );
            }

            // 对照：同批 pending 提案照常投递。
            let sink = dispatched(vec![]).await;
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "pending sanity check");
        }

        // 心跳 directive 经真实调度器：不参与合并窗口（source=Some 绕过 30s
        // 窗口立即执行），且不触发 60s 同文去重（同文本连投两条都受理）。
        #[tokio::test]
        async fn heartbeat_directive_bypasses_merge_window_and_text_dedup() {
            tokio::time::pause();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let handle = Scheduler::spawn("ws_1".to_string(), move |sig| {
                let tx = tx.clone();
                Box::pin(async move {
                    let _ = tx.send(sig);
                })
            });

            let p = proposal("set_hvac", Some("dev-1"));
            let sig1 = heartbeat_directive("ws_1", "set_hvac:dev-1".into(), &p);
            let sig2 = heartbeat_directive("ws_1", "set_hvac:dev-1".into(), &p);
            handle.enqueue(sig1).expect("first heartbeat directive");
            // 同文本第二条：不是 60s Duplicate（那是 chat/API 用户指令的规则）。
            handle
                .enqueue(sig2)
                .expect("same-text heartbeat directive is not a Duplicate");

            // 暂停时钟下不 advance：若进了 30s 合并窗口则永远收不到。
            let first = rx.recv().await.expect("runs immediately, no merge window");
            let second = rx
                .recv()
                .await
                .expect("second identical directive also runs immediately");
            assert_eq!(first.priority, Priority::Normal);
            assert!(matches!(first.source, TriggerSource::UserDirective { .. }));
            assert!(
                !matches!(first.source, TriggerSource::Merged { .. }),
                "heartbeat directives must never be merged"
            );
            assert!(!matches!(second.source, TriggerSource::Merged { .. }));
        }

        // Orchestrator 接线：HeartbeatCompleted 落库后驱动心跳桥投递。
        #[tokio::test]
        async fn heartbeat_completed_drives_bridge_after_persist() {
            let repo = Arc::new(MockTaskRepo::new());
            let insert_calls = repo.insert_result_calls();
            let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = repo;
            let runner = make_heartbeat_runner(Arc::clone(&repo));
            let publisher = make_publisher();
            let memory = make_memory_service();

            let (bridge, sink) = bridge(MemRuns::new(vec![]));
            let handler = AiEventHandler::new(
                runner,
                Arc::clone(&repo),
                memory,
                publisher,
                None,
                None,
                Some(Arc::new(bridge)),
                Arc::new(AtomicBool::new(false)),
            );

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;

            assert_eq!(insert_calls.lock().unwrap().len(), 1, "result still persisted");
            assert_eq!(sink.signals.lock().unwrap().len(), 1, "bridge dispatched the proposal");
        }

        // 无桥（None）时 HeartbeatCompleted 仅落库，不 panic。
        #[tokio::test]
        async fn heartbeat_completed_without_bridge_only_persists() {
            let repo = Arc::new(MockTaskRepo::new());
            let insert_calls = repo.insert_result_calls();
            let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = repo;
            let runner = make_heartbeat_runner(Arc::clone(&repo));

            let handler = AiEventHandler::new(
                runner,
                Arc::clone(&repo),
                make_memory_service(),
                make_publisher(),
                None,
                None,
                None,
                Arc::new(AtomicBool::new(false)),
            );

            let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
                workspace_id: "ws_1".to_string(),
                result: result_with(vec![proposal("set_hvac", Some("dev-1"))]),
            });
            handler.handle_ai_event(&event).await;
            assert_eq!(insert_calls.lock().unwrap().len(), 1);
        }
    }
}
