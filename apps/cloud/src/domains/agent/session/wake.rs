//! 自主唤醒：把一条到期的 `AssistantSchedule` 变成一个 scheduled Session 的 Turn。
//!
//! 触发与执行分离——本节只负责“接到 wake 后怎么跑”，何时唤醒由 `TriggerRouter` 决定。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::port::runtime::AgentLoopHandle;
use tinyiothub_core::harness::HarnessEventInput;
use tinyiothub_core::heartbeat::HeartbeatTask;
use tinyiothub_skills::LoadedSkill;
use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_storage::session::Session;

use super::trigger::EventWake;
use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::prompt::PromptMemorySource;
use crate::domains::agent::session::chat::service::ensure_agent;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::events::{SessionEventSink, session_event, session_started_payload};
use crate::domains::agent::session::runner::{SessionRunner, TurnCancellation, TurnOutcome};
use crate::domains::agent::session::suppression::SuppressionMode;
use crate::domains::agent::session::suppression::audit::{PlannedWake, WakeAttempt, attempt_event};
use crate::domains::agent::session::suppression::preflight::{PreflightDecision, SuppressionService};
use crate::domains::agent::session::suppression::registry::{
    self, SourceContext, SourceRegistry, WorkspaceSnapshotProvider,
};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::{PermissionMode, Todo, TodoStatus};

/// Assistant agent id 前缀。**不含冒号**：session key 的规范形状是
/// `agent:<ws>:<assistant>/<session>`，id 里多一个 `:` 会让 `SessionKey::parse`
/// 解析失败，scheduled 会话将无法被 chat 会话 API 打开/授权。
pub const ASSISTANT_AGENT_PREFIX: &str = "__assistant__";

/// 每个 workspace 一个常驻 Assistant 的 agent id。
pub fn assistant_agent_id(workspace_id: &str) -> String {
    format!("{ASSISTANT_AGENT_PREFIX}{workspace_id}")
}

/// 自定义 prompt > inspection 技能正文 > 未暂停的 legacy heartbeat tasks > 默认职责。
/// 只有未加载 inspection 技能时才会命中“任务全暂停 → `None`（本轮不唤醒）”分支；
/// 加载了技能则技能正文优先。
pub fn build_wake_message(
    schedule: &AssistantSchedule,
    inspection: Option<&LoadedSkill>,
    tasks: &[HeartbeatTask],
) -> Option<String> {
    let custom = schedule.prompt.trim();
    if !custom.is_empty() {
        return Some(custom.to_string());
    }
    if let Some(skill) = inspection {
        return Some(format!("按「{}」技能执行本轮检查。\n\n{}", skill.title, skill.body));
    }
    let active: Vec<&HeartbeatTask> = tasks.iter().filter(|task| !task.paused).collect();
    if !tasks.is_empty() && active.is_empty() {
        return None;
    }
    if active.is_empty() {
        return Some(
            "按照你的默认职责，检查当前工作区状态并处理需要关注的事项。若无事可做，简短说明即可。".to_string(),
        );
    }
    let mut message = String::from("本轮默认职责：\n");
    for task in active {
        message.push_str(&format!("- {}\n", task.text));
    }
    message.push_str("按顺序执行；用 todo_write 维护进度，完成后总结结果。");
    Some(message)
}

impl WakeRunner {
    async fn wake_prompt(
        &self,
        workspace_id: &str,
        assistant_id: &str,
        turn_input: String,
    ) -> anyhow::Result<crate::domains::agent::prompt::assistant::PromptPackage> {
        let session_key = format!("agent:{workspace_id}:{assistant_id}/main");
        let snapshot = match &self.snapshot_provider {
            Some(provider) => provider.workspace_snapshot(workspace_id).await,
            None => None,
        };
        // External semantic events are the same content the `session.events`
        // Source captures; harness lifecycle and cursor-owned user messages
        // are filtered there.
        let semantic_events = match registry::load_session_events(&self.db, workspace_id, &session_key).await {
            Ok(capture) => capture.events,
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    what = %error.what,
                    %workspace_id,
                    "failed to load semantic session events for full turn"
                );
                Vec::new()
            }
        };
        crate::domains::agent::prompt::assistant::build_assistant_prompt_with_sources(
            &self.system_prompts,
            workspace_id,
            assistant_id,
            turn_input,
            self.memory_source.as_ref(),
            snapshot.as_ref(),
            &semantic_events,
        )
        .await
    }

    /// Read-only suppression preflight observation.
    ///
    /// Exactly one registry and capture is built per wake. The returned
    /// registry owns the adapters' tombstone/payload caches, so the same
    /// captured snapshots serve the preflight decision and the eventual skip
    /// audit; rebuilding it between decision and commit is forbidden.
    async fn observe_suppression(
        &self,
        workspace_id: &str,
        assistant_id: &str,
    ) -> Option<(Arc<SourceRegistry>, PreflightDecision)> {
        let config = match self.db.get_suppression_config(workspace_id).await {
            Ok(Some(config)) => config,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression preflight: config read failed");
                return None;
            }
        };
        if config.mode == SuppressionMode::Off {
            return None;
        }
        let session_key = format!("agent:{workspace_id}:{assistant_id}/main");
        let state = match self.db.load_suppression_state(workspace_id, &session_key).await {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression preflight: state read failed");
                return None;
            }
        };
        // Same cursor predicate as Task 2/3: production
        // `message.created(role=user)` scoped to this session.
        let current_user_seq = match registry::load_session_events(&self.db, workspace_id, &session_key).await {
            Ok(capture) => capture.control_cursor,
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    %workspace_id,
                    "suppression preflight: user cursor read failed"
                );
                return None;
            }
        };
        let registry = match registry::default_sources(
            self.system_prompts.clone(),
            self.db.clone(),
            self.memory_source.clone(),
            self.snapshot_provider.clone(),
        ) {
            Ok(registry) => Arc::new(registry),
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    %workspace_id,
                    "suppression preflight: registry build failed"
                );
                return None;
            }
        };
        let context = SourceContext {
            workspace_id: workspace_id.to_string(),
            session_id: session_key,
            observed_at: chrono::Utc::now(),
            // The service applies the configured mode before capture.
            mode: config.mode.into(),
        };
        let service = SuppressionService::new(registry.clone());
        match service
            .evaluate(context, state.as_ref(), &config, current_user_seq)
            .await
        {
            Ok(decision) => {
                tracing::info!(
                    workspace_id = %workspace_id,
                    would_skip = decision.would_skip,
                    vetoes = ?decision.vetoes,
                    fence = %decision.fence,
                    baseline_state_id = decision.baseline_state_id.as_deref().unwrap_or(""),
                    config_version = decision.config_version,
                    mode = %config.mode.as_str(),
                    "suppression preflight observed"
                );
                Some((registry, decision))
            }
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    %workspace_id,
                    "suppression preflight failed; full turn unchanged"
                );
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOutcome {
    Completed,
    Partial,
    Failed {
        error: String,
    },
    Cancelled,
    Skipped,
    /// The skip audit failed or rolled back. TriggerRouter persists this as the
    /// Schedule outcome and it may not be overwritten by a later generic
    /// skipped/failed outcome.
    AuditFailed,
}

impl WakeOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::AuditFailed => "audit_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeReport {
    pub session_key: String,
    pub outcome: WakeOutcome,
}

#[async_trait::async_trait]
pub trait WakeExecutor: Send + Sync {
    async fn execute(&self, schedule: AssistantSchedule, cancel: CancellationToken) -> WakeReport;

    /// 事件 wake 入口。默认跳过——既有实现者无需改动即可继续编译。
    async fn execute_event(&self, _wake: EventWake, _cancel: CancellationToken) -> WakeReport {
        WakeReport {
            session_key: String::new(),
            outcome: WakeOutcome::Skipped,
        }
    }
}

pub struct WakeRunner {
    pub(crate) db: Arc<Db>,
    pool: Arc<AgentPool>,
    events: Arc<dyn SessionEventSink>,
    todos: Arc<TodoStore>,
    runner: Arc<SessionRunner>,
    system_prompts: Arc<tinyiothub_core::config::SystemPromptsConfig>,
    /// The same long-term memory selection the chat path injects.
    memory_source: Option<Arc<dyn PromptMemorySource>>,
    /// Existing live workspace snapshot; injected by the composition layer.
    snapshot_provider: Option<Arc<dyn WorkspaceSnapshotProvider>>,
}

/// Build the memory port over the platform database, matching the chat
/// composition (`MemoryStore::new(db.pool())`).
fn default_memory_source(db: &Arc<Db>) -> Arc<dyn PromptMemorySource> {
    Arc::new(
        crate::domains::agent::session::persist::memory::PromptMemoryStoreAdapter(Arc::new(
            tinyiothub_storage::memory::MemoryStore::new(db.pool().clone()),
        )),
    )
}

/// 一次 wake session 的来源（scheduled / event 共用同一套生命周期写入）。
///
/// `source_id` 为 `None` 时，`trigger.received.sourceId` 回落到本轮的
/// `wake.dispatched.wakeEventId`（scheduled 路径的相关性保持）。
struct SessionOrigin<'a> {
    workspace_id: &'a str,
    assistant_id: &'a str,
    kind: &'a str,
    trigger: &'a str,
    source_id: Option<String>,
    /// Stable planned wake time for scheduled dispatches. `None` keeps the
    /// previous per-invocation identity used by direct and event wakes.
    planned_wake_at: Option<DateTime<Utc>>,
}

impl WakeRunner {
    pub fn from_runtime(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        runtime: &crate::domains::agent::session::SessionRuntime,
    ) -> Self {
        let memory_source = Some(default_memory_source(&db));
        Self {
            db,
            pool,
            events: runtime.events.clone(),
            todos: runtime.todos.clone(),
            runner: runtime.runner.clone(),
            system_prompts: runtime.system_prompts.clone(),
            memory_source,
            snapshot_provider: None,
        }
    }

    pub fn from_parts(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        events: Arc<dyn SessionEventSink>,
        todos: Arc<TodoStore>,
        runner: Arc<SessionRunner>,
    ) -> Self {
        let memory_source = Some(default_memory_source(&db));
        Self {
            db,
            pool,
            events,
            todos,
            runner,
            system_prompts: Arc::new(tinyiothub_core::config::SystemPromptsConfig::default()),
            memory_source,
            snapshot_provider: None,
        }
    }

    /// Inject the existing live workspace snapshot port. Without it the
    /// `workspace.snapshot` Source is degraded (never silently empty).
    pub fn with_snapshot_provider(mut self, provider: Arc<dyn WorkspaceSnapshotProvider>) -> Self {
        self.snapshot_provider = Some(provider);
        self
    }

    /// Override the long-term memory port (tests/composition).
    pub fn with_memory_source(mut self, source: Arc<dyn PromptMemorySource>) -> Self {
        self.memory_source = Some(source);
        self
    }

    pub async fn run(&self, schedule: &AssistantSchedule, cancel: CancellationToken) -> WakeReport {
        if !schedule.enabled {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        }
        let tasks = self
            .db
            .list_heartbeat_tasks(&schedule.workspace_id)
            .await
            .unwrap_or_default();
        let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(&schedule.workspace_id);
        let skills = crate::domains::agent::pool::load_workspace_skills(&ws_dir);
        let inspection = skills.iter().find(|skill| skill.name == "inspection");
        let Some(message) = build_wake_message(schedule, inspection, &tasks) else {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        };
        // One shared registry/capture per wake survives until the Turn ends;
        // Task 6 consumes the same decision for the durable skip audit.
        let _suppression_observation = self
            .observe_suppression(&schedule.workspace_id, &schedule.assistant_id)
            .await;
        let prompt = match self
            .wake_prompt(&schedule.workspace_id, &schedule.assistant_id, message)
            .await
        {
            Ok(prompt) => prompt,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        let agent = match ensure_agent(
            &self.pool,
            &schedule.assistant_id,
            &schedule.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
            &prompt,
        )
        .await
        {
            Ok(agent) => agent,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        self.run_with_agent(schedule, agent, &prompt.turn_input, cancel).await
    }

    pub(crate) async fn run_with_agent(
        &self,
        schedule: &AssistantSchedule,
        agent: AgentLoopHandle,
        message: &str,
        cancel: CancellationToken,
    ) -> WakeReport {
        self.run_session_with_agent(
            SessionOrigin {
                workspace_id: &schedule.workspace_id,
                assistant_id: &schedule.assistant_id,
                kind: "scheduled",
                trigger: "schedule",
                source_id: None,
                planned_wake_at: schedule.next_wake_at,
            },
            agent,
            message,
            schedule.permission_mode,
            cancel,
        )
        .await
    }

    /// 告警等外部事件 wake：与定时 wake 共用生命周期事件，kind 为 `event`。
    ///
    /// 暂停态（`schedule.enabled == false`）的工作区不跑自主 Turn——告警仍
    /// 进入 Session Loop 的准入层，但在暂停态下被跳过。无 schedule 行时先
    /// ensure（继承 legacy autonomy mapping），确保 legacy `off` 工作区不会
    /// 在一次事件唤醒中恢复；ensure 失败按 fail-closed 跳过。
    pub async fn run_event(&self, wake: &EventWake, cancel: CancellationToken) -> WakeReport {
        let workspace_id = &wake.workspace_id;
        let schedule = match self
            .db
            .ensure_assistant_schedule(workspace_id, &assistant_agent_id(workspace_id), chrono::Utc::now())
            .await
        {
            Ok(schedule) => schedule,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "event wake: ensure schedule failed");
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Skipped,
                };
            }
        };
        if !schedule.enabled {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        }
        let permission_mode = schedule.permission_mode;
        let message = match wake.trigger.as_str() {
            "watch" => format!("收到设备值守命中：{}\n请评估影响；涉及写设备必须走审批。", wake.summary),
            "thing_event" => format!(
                "收到设备事件：{}\n请判断是否需要处理；涉及写设备必须走审批。",
                wake.summary
            ),
            _ => format!(
                "收到告警：{}\n请按「应急预案」技能（如未加载则按默认职责）评估影响、给出处置建议；涉及写设备必须走审批。",
                wake.summary
            ),
        };
        let prompt = match self
            .wake_prompt(&wake.workspace_id, &assistant_agent_id(&wake.workspace_id), message)
            .await
        {
            Ok(prompt) => prompt,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        let assistant_id = assistant_agent_id(&wake.workspace_id);
        let agent = match ensure_agent(
            &self.pool,
            &assistant_id,
            &wake.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
            &prompt,
        )
        .await
        {
            Ok(agent) => agent,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        self.run_event_with_agent(wake.clone(), agent, &prompt.turn_input, permission_mode, cancel)
            .await
    }

    pub(crate) async fn run_event_with_agent(
        &self,
        wake: EventWake,
        agent: AgentLoopHandle,
        message: &str,
        permission_mode: PermissionMode,
        cancel: CancellationToken,
    ) -> WakeReport {
        let assistant_id = assistant_agent_id(&wake.workspace_id);
        self.run_session_with_agent(
            SessionOrigin {
                workspace_id: &wake.workspace_id,
                assistant_id: &assistant_id,
                kind: "event",
                trigger: &wake.trigger,
                source_id: Some(wake.source_id.clone()),
                planned_wake_at: None,
            },
            agent,
            message,
            permission_mode,
            cancel,
        )
        .await
    }

    async fn reseed_session_history(
        &self,
        agent: &dyn tinyiothub_agent::port::runtime::AgentLoop,
        workspace_id: &str,
        session_key: &str,
    ) -> anyhow::Result<()> {
        let filter = tinyiothub_storage::harness_event::EventFilter {
            session_id: Some(session_key.to_string()),
            run_id: None,
            task_id: None,
            execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
        };
        let mut messages = Vec::new();
        let mut after = 0;
        loop {
            let page = self.db.session_event_page(workspace_id, &filter, after, 200).await?;
            let Some(last) = page.events.last() else {
                break;
            };
            after = last.seq;
            for event in page.events {
                if event.event_type != "message.created" {
                    continue;
                }
                let Some(role) = event.payload_json["role"].as_str() else {
                    continue;
                };
                if role != "user" && role != "assistant" {
                    continue;
                }
                let Some(text) = event.payload_json["text"].as_str() else {
                    continue;
                };
                messages.push(tinyiothub_agent::port::provider::ChatMessage {
                    role: role.to_string(),
                    content: text.to_string(),
                    attachments: vec![],
                });
            }
            if after >= page.high_watermark {
                break;
            }
        }
        agent.clear_history().await;
        agent.seed_history(&messages).await
    }

    async fn run_session_with_agent(
        &self,
        origin: SessionOrigin<'_>,
        agent: AgentLoopHandle,
        message: &str,
        permission_mode: PermissionMode,
        cancel: CancellationToken,
    ) -> WakeReport {
        let session_key = format!("agent:{}:{}/main", origin.workspace_id, origin.assistant_id);
        if let Err(error) = self
            .db
            .ensure_session(&session_key, origin.workspace_id, origin.assistant_id)
            .await
        {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed {
                    error: error.to_string(),
                },
            };
        }
        let mut session = match self.db.get_session(&session_key).await {
            Ok(Some(session)) => session,
            Ok(None) => Session::new(
                session_key.clone(),
                origin.workspace_id.to_string(),
                origin.assistant_id.to_string(),
            )
            .with_kind(origin.kind),
            Err(error) => {
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        session.kind = origin.kind.to_string();
        if session.label.is_none() {
            session.label = Some("大脑".to_string());
        }
        session.status = "active".to_string();
        session.touch();
        if let Err(error) = self.db.update_session(&session).await {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed {
                    error: error.to_string(),
                },
            };
        }
        // A planned scheduled wake owns a stable wakeId derived from
        // `(workspaceId, plannedWakeAt)` and an attempt-derived turnId. Direct
        // and event wakes keep their previous per-invocation identity.
        let planned_wake = origin
            .planned_wake_at
            .map(|planned_wake_at| PlannedWake::planned(origin.workspace_id, session_key.clone(), planned_wake_at));
        let attempt = match &planned_wake {
            Some(wake) => Some(wake.attempt(self.next_attempt_no(origin.workspace_id, &session_key).await)),
            None => None,
        };
        let context = wake_context(
            &origin,
            session_key.clone(),
            permission_mode,
            attempt.as_ref().map(|attempt| attempt.turn_id.clone()),
        );
        let wake_event_id = attempt
            .as_ref()
            .map(|attempt| attempt.wake_event_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        // scheduled 无显式 source：与本轮 wake.dispatched.wakeEventId 保持同一
        // 身份；event 使用告警 id。
        let trigger_source_id = origin.source_id.unwrap_or_else(|| wake_event_id.clone());
        self.emit_lifecycle(
            &context,
            attempt.as_ref(),
            "trigger.received",
            json!({ "trigger": origin.trigger, "sourceId": trigger_source_id }),
        )
        .await;
        self.emit_lifecycle(
            &context,
            attempt.as_ref(),
            "wake.dispatched",
            json!({ "wakeEventId": &wake_event_id }),
        )
        .await;
        self.emit_lifecycle(
            &context,
            attempt.as_ref(),
            "session.started",
            session_started_payload(&session_key, origin.kind),
        )
        .await;

        let turn = {
            let guard = agent.lock().await;
            // The shared loop must contain only this Assistant session's history.
            if let Err(error) = self
                .reseed_session_history(&*guard, origin.workspace_id, &session_key)
                .await
            {
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
            let turn = Some(
                self.runner
                    .run_turn(
                        &*guard,
                        &context,
                        message,
                        None,
                        Some(TurnCancellation::schedule(cancel.clone())),
                    )
                    .await,
            );
            drop(guard);
            turn
        };
        let outcome = match turn {
            Some(turn) => outcome_with_todos(turn, &self.todos.get(&session_key)),
            None => WakeOutcome::Cancelled,
        };

        self.emit_lifecycle(
            &context,
            attempt.as_ref(),
            "session.ended",
            json!({ "sessionId": &session_key, "outcome": outcome.as_str() }),
        )
        .await;

        let mut ended = session;
        ended.status = "ended".to_string();
        ended.touch();
        if let Err(error) = self.db.update_session(&ended).await {
            tracing::warn!(%error, %session_key, "failed to mark wake session ended");
        }
        WakeReport { session_key, outcome }
    }

    /// Next monotonic attempt number. Durable allocation happens in
    /// `begin_full_attempt`/`begin_skip_attempt`; reading the current row keeps
    /// the lifecycle identity aligned with the attempt those CAS operations
    /// will grant.
    async fn next_attempt_no(&self, workspace_id: &str, session_key: &str) -> i64 {
        match self.db.load_suppression_state(workspace_id, session_key).await {
            Ok(state) => state.map(|state| state.attempt_no + 1).unwrap_or(1),
            Err(error) => {
                tracing::warn!(
                    %error,
                    %workspace_id,
                    "failed to load suppression attempt number; using the first attempt"
                );
                1
            }
        }
    }

    async fn emit_lifecycle(
        &self,
        context: &TurnContext,
        attempt: Option<&WakeAttempt>,
        event_type: &str,
        payload: Value,
    ) {
        let input = match attempt {
            Some(attempt) => attempt_event(context, attempt, event_type, payload),
            None => session_event(context, event_type, payload),
        };
        self.emit_input(input).await;
    }

    async fn emit_input(&self, input: HarnessEventInput) {
        let event_type = input.event_type.clone();
        if let Err(error) = self.events.emit(input).await {
            tracing::warn!(%error, %event_type, "failed to persist wake session event");
        }
    }
}

#[async_trait::async_trait]
impl WakeExecutor for WakeRunner {
    async fn execute(&self, schedule: AssistantSchedule, cancel: CancellationToken) -> WakeReport {
        self.run(&schedule, cancel).await
    }

    async fn execute_event(&self, wake: EventWake, cancel: CancellationToken) -> WakeReport {
        self.run_event(&wake, cancel).await
    }
}

/// 构造 wake Turn 的上下文。scheduled / event 两个入口共用，确保 schedule
/// 的 `permission_mode` 一定进入 `TurnContext`（`PermissionGuardTool` 读它做
/// 工具层写拒绝）。
fn wake_context(
    origin: &SessionOrigin<'_>,
    session_key: String,
    permission_mode: PermissionMode,
    turn_id: Option<String>,
) -> TurnContext {
    let mut context = TurnContext::new(
        origin.workspace_id.to_string(),
        origin.assistant_id.to_string(),
        session_key,
        Uuid::new_v4().to_string(),
        0,
    )
    .with_permission_mode(permission_mode);
    if let Some(turn_id) = turn_id {
        context.turn_id = turn_id;
    }
    context
}

fn outcome_with_todos(outcome: TurnOutcome, todos: &[Todo]) -> WakeOutcome {
    match outcome {
        TurnOutcome::Completed { .. } if todos.iter().any(|todo| todo.status != TodoStatus::Completed) => {
            WakeOutcome::Partial
        }
        TurnOutcome::Completed { .. } => WakeOutcome::Completed,
        TurnOutcome::Cancelled { .. } => WakeOutcome::Cancelled,
        TurnOutcome::Failed { error } => WakeOutcome::Failed { error },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContextRegistry;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::runner::SessionRunner;
    use crate::domains::agent::session::todos::TodoStore;
    use crate::domains::agent::session::types::{Todo, TodoStatus};
    use std::sync::Arc;
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;
    use tinyiothub_agent::port::provider::{ChatMessage, ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_core::heartbeat::HeartbeatTask;
    use tinyiothub_storage::assistant_schedule::AssistantSchedule;
    use tokio::sync::mpsc;

    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("巡检完成".into()),
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

    #[derive(Default)]
    struct RecordingWakeState {
        seeds: Vec<Vec<ChatMessage>>,
        inputs: Vec<String>,
    }

    struct RecordingWakeLoop {
        state: Arc<tokio::sync::Mutex<RecordingWakeState>>,
    }

    #[async_trait::async_trait]
    impl tinyiothub_agent::port::runtime::AgentLoop for RecordingWakeLoop {
        async fn turn_streamed(
            &self,
            user_message: &str,
            _event_tx: mpsc::Sender<tinyiothub_agent::port::events::TurnEvent>,
            _cancel_token: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            self.state.lock().await.inputs.push(user_message.into());
            Ok("done".into())
        }

        async fn run_single(&self, message: &str) -> anyhow::Result<String> {
            self.state.lock().await.inputs.push(message.into());
            Ok("done".into())
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, messages: &[ChatMessage]) -> anyhow::Result<()> {
            self.state.lock().await.seeds.push(messages.to_vec());
            Ok(())
        }
    }

    fn task(text: &str, paused: bool) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws".into(),
            priority: "low".into(),
            text: text.into(),
            paused,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn test_prompt() -> crate::domains::agent::prompt::assistant::PromptPackage {
        crate::domains::agent::prompt::assistant::PromptPackage {
            system_prompt: "test-prompt".into(),
            turn_input: String::new(),
        }
    }

    fn schedule(prompt: &str) -> AssistantSchedule {
        AssistantSchedule {
            workspace_id: "ws".into(),
            assistant_id: assistant_agent_id("ws"),
            interval_seconds: 300,
            enabled: true,
            permission_mode: PermissionMode::Ask,
            prompt: prompt.into(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        }
    }

    async fn fixture() -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        fixture_with(Arc::new(|| Ok(Box::new(ScriptedProvider)))).await
    }

    async fn fixture_with(
        provider_factory: tinyiothub_agent::provider::ProviderFactory,
    ) -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        // Each test gets its own workspace-store root; the default root is
        // shared repo state, so parallel tests would race harness migrations.
        let db = Arc::new(tinyiothub_storage::Db::new(pool).with_workspace_store_root(
            std::env::temp_dir().join(format!("tinyiothub-wake-store-{}", uuid::Uuid::new_v4())),
        ));
        tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws', 'workspace', 'tenant-default-001', 'now', 'now')")
            .execute(db.pool())
            .await
            .unwrap();
        db.create_workspace_store("ws").await.unwrap();
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                provider_factory,
            )
            .unwrap(),
        );
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink.clone(), contexts));
        let todos = Arc::new(TodoStore::default());
        let wake = WakeRunner::from_parts(db.clone(), agent_pool.clone(), sink.clone(), todos, runner);
        (wake, agent_pool, sink)
    }

    #[test]
    fn wake_message_uses_custom_prompt_first() {
        let message = build_wake_message(&schedule("检查 3 号楼"), None, &[task("离线事件", false)]);
        assert_eq!(message.as_deref(), Some("检查 3 号楼"));
    }

    #[test]
    fn wake_message_lists_active_tasks_and_skips_all_paused() {
        let message =
            build_wake_message(&schedule(""), None, &[task("离线事件", false), task("设备异常", true)]).unwrap();
        assert!(message.contains("- 离线事件"));
        assert!(!message.contains("设备异常"));
        assert!(build_wake_message(&schedule(""), None, &[task("离线事件", true)]).is_none());
        assert!(
            build_wake_message(&schedule(""), None, &[])
                .unwrap()
                .contains("默认职责")
        );
    }

    #[test]
    fn wake_message_prefers_inspection_skill_over_tasks() {
        let skill = tinyiothub_skills::LoadedSkill {
            name: "inspection".into(),
            title: "巡检".into(),
            description: String::new(),
            version: "1".into(),
            inject: tinyiothub_skills::InjectMode::Trigger,
            trigger: Some("/inspection".into()),
            body: "# 巡检\n## 执行步骤\n1. 检查离线设备".into(),
        };
        let message = build_wake_message(&schedule(""), Some(&skill), &[task("旧任务", false)]).unwrap();
        assert!(message.contains("检查离线设备"));
        assert!(!message.contains("旧任务"));
        let custom = build_wake_message(&schedule("自定义"), Some(&skill), &[task("旧任务", false)]).unwrap();
        assert_eq!(custom, "自定义");
    }

    #[test]
    fn outcome_with_todos_marks_partial() {
        let incomplete = vec![Todo {
            content: "巡检".into(),
            status: TodoStatus::InProgress,
            active_form: "巡检中".into(),
        }];
        assert_eq!(
            outcome_with_todos(
                TurnOutcome::Completed {
                    text: String::new(),
                    tool_calls: 0
                },
                &incomplete
            ),
            WakeOutcome::Partial
        );
        assert_eq!(
            outcome_with_todos(
                TurnOutcome::Completed {
                    text: String::new(),
                    tool_calls: 0
                },
                &[]
            ),
            WakeOutcome::Completed
        );
    }

    #[tokio::test]
    async fn wake_reseeds_only_current_session_history() {
        let (wake, _agent_pool, _sink) = fixture().await;
        let assistant_id = assistant_agent_id("ws");
        let session_key = format!("agent:ws:{assistant_id}/main");
        for (session, text) in [
            (session_key.as_str(), "current-session-marker"),
            ("agent:ws:other/other", "other-session-marker"),
        ] {
            let context = TurnContext::new("ws".into(), assistant_id.clone(), session.into(), "old-run".into(), 0);
            wake.db
                .append_session_event(&session_event(
                    &context,
                    "message.created",
                    json!({ "messageId": uuid::Uuid::new_v4().to_string(), "role": "user", "text": text }),
                ))
                .await
                .unwrap();
        }
        let state = Arc::new(tokio::sync::Mutex::new(RecordingWakeState::default()));
        let agent = Arc::new(tokio::sync::Mutex::new(RecordingWakeLoop { state: state.clone() })) as AgentLoopHandle;

        let report = wake
            .run_with_agent(&schedule(""), agent, "next-turn", CancellationToken::new())
            .await;

        assert_eq!(report.outcome, WakeOutcome::Completed);
        let state = state.lock().await;
        let seeded = state.seeds.iter().flatten().collect::<Vec<_>>();
        assert!(
            seeded
                .iter()
                .any(|message| message.content.contains("current-session-marker"))
        );
        assert!(
            seeded
                .iter()
                .all(|message| !message.content.contains("other-session-marker"))
        );
        assert_eq!(state.inputs, vec!["next-turn"]);
    }

    #[tokio::test]
    async fn wake_prompt_uses_injected_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("AGENTS.md"), "configured-workspace-marker").unwrap();
        let (mut wake, _agent_pool, _sink) = fixture().await;
        wake.system_prompts = Arc::new(tinyiothub_core::config::SystemPromptsConfig {
            workspace_dir: root.path().display().to_string(),
            ..Default::default()
        });

        let package = wake.wake_prompt("ws", "assistant", "turn".into()).await.unwrap();
        assert!(package.system_prompt.contains("configured-workspace-marker"));
    }

    #[tokio::test]
    async fn wake_prompt_injects_short_term_snapshot_and_semantic_events_once() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("SHORT_TERM.md"), "short-term-wake-marker").unwrap();

        let (mut wake, _agent_pool, _sink) = fixture().await;
        wake.system_prompts = Arc::new(tinyiothub_core::config::SystemPromptsConfig {
            workspace_dir: root.path().display().to_string(),
            ..Default::default()
        });
        let snapshot = tinyiothub_runtime::watch::live_state::WorkspaceSnapshot {
            v: 1,
            devices: vec![tinyiothub_runtime::watch::live_state::DeviceSnapshot {
                thing_id: "device-1".into(),
                name: "wake-device-marker".into(),
                health: tinyiothub_runtime::watch::live_state::Health::Ok,
                online: true,
                zone: "zone".into(),
                values: vec![],
                properties: vec![],
            }],
            total: 1,
        };
        wake = wake.with_snapshot_provider(Arc::new(
            crate::domains::agent::session::suppression::registry::SnapshotProviderSlot::with(snapshot),
        ));

        let context = TurnContext::new(
            "ws".into(),
            assistant_agent_id("ws"),
            format!("agent:ws:{}/main", assistant_agent_id("ws")),
            "run".into(),
            0,
        );
        wake.db
            .append_session_event(&session_event(
                &context,
                "thing.event",
                json!({ "marker": "wake-event-marker" }),
            ))
            .await
            .unwrap();

        let package = wake
            .wake_prompt("ws", &assistant_agent_id("ws"), "turn".into())
            .await
            .unwrap();
        assert_eq!(package.system_prompt.matches("short-term-wake-marker").count(), 1);
        assert_eq!(package.system_prompt.matches("wake-device-marker").count(), 1);
        assert_eq!(package.system_prompt.matches("wake-event-marker").count(), 1);
        assert_eq!(package.turn_input, "turn");
    }

    #[tokio::test]
    async fn wake_run_writes_lifecycle_and_ends_session() {
        let (wake, agent_pool, sink) = fixture().await;
        let schedule = schedule("");
        let agent = agent_pool
            .create(
                &schedule.assistant_id,
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let report = wake
            .run_with_agent(&schedule, agent, "按默认职责巡检", CancellationToken::new())
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let parsed = tinyiothub_agent::session::SessionKey::parse(&report.session_key)
            .expect("wake session key must be a valid agent:<ws>:<assistant>/<session> key");
        assert_eq!(parsed.workspace_id, "ws");
        assert_eq!(parsed.agent_id, assistant_agent_id("ws"));
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "step.text",
                "message.created",
                "turn.completed",
                "session.ended",
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "scheduled");
        assert_eq!(
            events[0].payload_json["sourceId"], events[1].payload_json["wakeEventId"],
            "scheduled trigger.received.sourceId must correlate with wake.dispatched.wakeEventId"
        );
        assert_eq!(events.last().unwrap().payload_json["outcome"], "completed");
        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.kind, "scheduled");
        assert_eq!(session.status, "ended");
    }

    #[tokio::test]
    async fn subsequent_wakes_reuse_the_default_assistant_session() {
        let (wake, agent_pool, _sink) = fixture().await;
        let schedule = schedule("");
        let agent = agent_pool
            .create(
                &schedule.assistant_id,
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let first = wake
            .run_with_agent(&schedule, agent.clone(), "第一次巡检", CancellationToken::new())
            .await;
        let second = wake
            .run_with_agent(&schedule, agent, "第二次巡检", CancellationToken::new())
            .await;
        assert_eq!(first.outcome, WakeOutcome::Completed);
        assert_eq!(second.outcome, WakeOutcome::Completed);
        assert_eq!(first.session_key, second.session_key);
        assert!(first.session_key.ends_with("/main"));
        let sessions = wake.db.list_sessions(Some("ws"), None, 100, 0).await.unwrap();
        assert_eq!(sessions.len(), 1, "a workspace must not create one session per wake");
    }

    #[tokio::test]
    async fn disabled_schedule_skips_wake() {
        let (wake, agent_pool, sink) = fixture().await;
        let mut schedule = schedule("");
        schedule.enabled = false;
        let report = wake.run(&schedule, CancellationToken::new()).await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(sink.events.lock().await.is_empty());
        // schedule() uses assistant_agent_id("ws"); the wake agent must not have been created
        assert_eq!(agent_pool.pool_size(), 0);
    }

    #[tokio::test]
    async fn disabled_schedule_skips_event_wake() {
        let (wake, agent_pool, sink) = fixture().await;
        wake.db
            .ensure_assistant_schedule("ws", &assistant_agent_id("ws"), chrono::Utc::now())
            .await
            .unwrap();
        wake.db
            .update_assistant_schedule("ws", Some(false), None, None, None, None)
            .await
            .unwrap();
        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(report.session_key.is_empty());
        assert!(sink.events.lock().await.is_empty());
        assert_eq!(
            agent_pool.pool_size(),
            0,
            "no agent should be created for a skipped event wake"
        );
        let sessions = wake.db.list_sessions(Some("ws"), None, 10, 0).await.unwrap();
        assert!(sessions.is_empty(), "no session row should be written");
    }

    #[tokio::test]
    async fn event_wake_without_schedule_continues_enabled() {
        // No schedule row: event wakes default to enabled (Ask) rather than
        // silently failing.
        let (wake, _agent_pool, sink) = fixture().await;
        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        assert!(!sink.events.lock().await.is_empty());
        let row = wake
            .db
            .get_assistant_schedule("ws")
            .await
            .unwrap()
            .expect("schedule ensured");
        assert!(row.enabled);
        assert_eq!(row.permission_mode, PermissionMode::Ask);
    }

    #[tokio::test]
    async fn legacy_off_event_wake_is_skipped_and_persisted_as_paused() {
        // No schedule row yet, but the workspace still has the legacy `off`
        // policy: ensure must create a paused row and the event wake must skip.
        let (wake, agent_pool, sink) = fixture().await;
        sqlx::query("INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES ('ws', 'off')")
            .execute(wake.db.pool())
            .await
            .unwrap();

        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(report.session_key.is_empty());
        assert!(sink.events.lock().await.is_empty());
        assert_eq!(agent_pool.pool_size(), 0, "no agent for a paused event wake");

        let row = wake
            .db
            .get_assistant_schedule("ws")
            .await
            .unwrap()
            .expect("schedule ensured");
        assert!(!row.enabled, "legacy off must persist as paused");
    }

    #[test]
    fn wake_context_carries_schedule_permission_mode() {
        let origin = SessionOrigin {
            workspace_id: "ws",
            assistant_id: "__assistant__ws",
            kind: "scheduled",
            trigger: "schedule",
            source_id: None,
            planned_wake_at: None,
        };
        let context = wake_context(&origin, "agent:ws:a/s".into(), PermissionMode::ReadOnly, None);
        assert_eq!(context.workspace_id, "ws");
        assert_eq!(context.assistant_id, "__assistant__ws");
        assert_eq!(context.session_key, "agent:ws:a/s");
        assert_eq!(context.permission_mode, PermissionMode::ReadOnly);

        let ask = wake_context(&origin, "s".into(), PermissionMode::Ask, Some("turn-attempt-2".into()));
        assert_eq!(ask.permission_mode, PermissionMode::Ask);
        assert_eq!(ask.turn_id, "turn-attempt-2");
    }

    struct SleepingProvider;

    #[async_trait::async_trait]
    impl ModelProvider for SleepingProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Ok(ChatResponse {
                text: Some("too late".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for SleepingProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }

        fn alias(&self) -> &str {
            "SleepingProvider"
        }
    }

    #[tokio::test]
    async fn cancel_during_wake_ends_session_as_cancelled() {
        let (wake, _agent_pool, sink) = fixture_with(Arc::new(|| Ok(Box::new(SleepingProvider)))).await;
        let wake = Arc::new(wake);
        let cancel = CancellationToken::new();
        let handle = {
            let wake = wake.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { wake.run(&schedule(""), cancel).await })
        };

        // Wait until the turn has actually started before cancelling.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let started = sink
                .events
                .lock()
                .await
                .iter()
                .any(|event| event.event_type == "turn.started");
            if started {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "turn did not start before the deadline"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        cancel.cancel();

        let report = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("cancelled wake must finish promptly")
            .unwrap();
        assert_eq!(report.outcome, WakeOutcome::Cancelled);

        let events = sink.events.lock().await;
        assert_eq!(
            events.last().unwrap().event_type,
            "session.ended",
            "cancelled wake must still emit session.ended"
        );
        assert_eq!(events.last().unwrap().payload_json["outcome"], "cancelled");
        let terminal = events
            .iter()
            .find(|event| event.event_type == "turn.completed")
            .expect("cancelled wake must finish its turn");
        assert_eq!(terminal.payload_json["outcome"], "cancelled");
        assert_eq!(terminal.payload_json["source"], "schedule");
        assert_eq!(terminal.payload_json["error"], "调度器停止本轮执行");
        drop(events);

        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.status, "ended");
    }

    #[tokio::test]
    async fn event_wake_writes_event_session_lifecycle() {
        let (wake, agent_pool, sink) = fixture().await;
        let agent = agent_pool
            .create(
                &assistant_agent_id("ws"),
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let report = wake
            .run_event_with_agent(
                EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                agent,
                "收到告警：温度过高",
                PermissionMode::Ask,
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "step.text",
                "message.created",
                "turn.completed",
                "session.ended"
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "event");
        assert_eq!(events[0].payload_json["sourceId"], "alarm-1");
    }
}
