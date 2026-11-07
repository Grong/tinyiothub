//! 自主唤醒：把一条到期的 `AssistantSchedule` 变成一个 scheduled Session 的 Turn。
//!
//! 触发与执行分离——本节只负责“接到 wake 后怎么跑”，何时唤醒由 `TriggerRouter` 决定。

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::port::runtime::AgentLoopHandle;
use tinyiothub_core::heartbeat::HeartbeatTask;
use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_storage::session::Session;

use crate::domains::agent::host::chat::service::ensure_agent;
use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::session::SESSION_TURN_TIMEOUT;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::events::{SessionEventSink, session_event, session_started_payload};
use crate::domains::agent::session::runner::{SessionRunner, TurnOutcome};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::{Todo, TodoStatus};

/// Assistant agent id 前缀。**不含冒号**：session key 的规范形状是
/// `agent:<ws>:<assistant>/<session>`，id 里多一个 `:` 会让 `SessionKey::parse`
/// 解析失败，scheduled 会话将无法被 chat 会话 API 打开/授权。
pub const ASSISTANT_AGENT_PREFIX: &str = "__assistant__";

/// 每个 workspace 一个常驻 Assistant 的 agent id。
pub fn assistant_agent_id(workspace_id: &str) -> String {
    format!("{ASSISTANT_AGENT_PREFIX}{workspace_id}")
}

/// 自定义 prompt > 未暂停的 legacy heartbeat tasks > 默认职责。
/// 存在任务但全部暂停时返回 `None`（本轮不唤醒）。
pub fn build_wake_message(schedule: &AssistantSchedule, tasks: &[HeartbeatTask]) -> Option<String> {
    let custom = schedule.prompt.trim();
    if !custom.is_empty() {
        return Some(custom.to_string());
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOutcome {
    Completed,
    Partial,
    Failed { error: String },
    Cancelled,
    Skipped,
}

impl WakeOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
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
    async fn execute(&self, schedule: AssistantSchedule) -> WakeReport;
}

pub struct WakeRunner {
    pub(crate) db: Arc<Db>,
    pool: Arc<AgentPool>,
    events: Arc<dyn SessionEventSink>,
    todos: Arc<TodoStore>,
    runner: Arc<SessionRunner>,
}

impl WakeRunner {
    pub fn from_runtime(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        runtime: &crate::domains::agent::session::SessionRuntime,
    ) -> Self {
        Self {
            db,
            pool,
            events: runtime.events.clone(),
            todos: runtime.todos.clone(),
            runner: runtime.runner.clone(),
        }
    }

    pub fn from_parts(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        events: Arc<dyn SessionEventSink>,
        todos: Arc<TodoStore>,
        runner: Arc<SessionRunner>,
    ) -> Self {
        Self {
            db,
            pool,
            events,
            todos,
            runner,
        }
    }

    pub async fn run(&self, schedule: &AssistantSchedule) -> WakeReport {
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
        let Some(message) = build_wake_message(schedule, &tasks) else {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        };
        let agent = match ensure_agent(
            &self.pool,
            &schedule.assistant_id,
            &schedule.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
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
        self.run_with_agent(schedule, agent, &message).await
    }

    pub(crate) async fn run_with_agent(
        &self,
        schedule: &AssistantSchedule,
        agent: AgentLoopHandle,
        message: &str,
    ) -> WakeReport {
        let session_key = format!(
            "agent:{}:{}/{}",
            schedule.workspace_id,
            schedule.assistant_id,
            Uuid::new_v4()
        );
        let session = Session::new(
            session_key.clone(),
            schedule.workspace_id.clone(),
            schedule.assistant_id.clone(),
        )
        .with_kind("scheduled");
        if let Err(error) = self.db.create_session(&session).await {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed {
                    error: error.to_string(),
                },
            };
        }
        let context = TurnContext::new(
            schedule.workspace_id.clone(),
            schedule.assistant_id.clone(),
            session_key.clone(),
            Uuid::new_v4().to_string(),
            0,
        );
        let wake_event_id = Uuid::new_v4().to_string();
        self.emit(
            &context,
            "trigger.received",
            json!({ "trigger": "schedule", "sourceId": &wake_event_id }),
        )
        .await;
        self.emit(&context, "wake.dispatched", json!({ "wakeEventId": &wake_event_id }))
            .await;
        self.emit(
            &context,
            "session.started",
            session_started_payload(&session_key, "scheduled"),
        )
        .await;

        let outcome = {
            let guard = agent.lock().await;
            let _ = guard.clear_history().await;
            let turn = self
                .runner
                .run_turn_bounded(&*guard, &context, message, None, SESSION_TURN_TIMEOUT)
                .await;
            drop(guard);
            outcome_with_todos(turn, &self.todos.get(&session_key))
        };

        self.emit(
            &context,
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

    async fn emit(&self, context: &TurnContext, event_type: &str, payload: serde_json::Value) {
        if let Err(error) = self.events.emit(session_event(context, event_type, payload)).await {
            tracing::warn!(%error, %event_type, "failed to persist wake session event");
        }
    }
}

#[async_trait::async_trait]
impl WakeExecutor for WakeRunner {
    async fn execute(&self, schedule: AssistantSchedule) -> WakeReport {
        self.run(&schedule).await
    }
}

fn outcome_with_todos(outcome: TurnOutcome, todos: &[Todo]) -> WakeOutcome {
    match outcome {
        TurnOutcome::Completed { .. } if todos.iter().any(|todo| todo.status != TodoStatus::Completed) => {
            WakeOutcome::Partial
        }
        TurnOutcome::Completed { .. } => WakeOutcome::Completed,
        TurnOutcome::Cancelled => WakeOutcome::Cancelled,
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
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_core::heartbeat::HeartbeatTask;
    use tinyiothub_storage::assistant_schedule::AssistantSchedule;

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

    fn schedule(prompt: &str) -> AssistantSchedule {
        AssistantSchedule {
            workspace_id: "ws".into(),
            assistant_id: assistant_agent_id("ws"),
            interval_seconds: 300,
            enabled: true,
            prompt: prompt.into(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        }
    }

    async fn fixture() -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
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
        let message = build_wake_message(&schedule("检查 3 号楼"), &[task("离线事件", false)]);
        assert_eq!(message.as_deref(), Some("检查 3 号楼"));
    }

    #[test]
    fn wake_message_lists_active_tasks_and_skips_all_paused() {
        let message = build_wake_message(&schedule(""), &[task("离线事件", false), task("设备异常", true)]).unwrap();
        assert!(message.contains("- 离线事件"));
        assert!(!message.contains("设备异常"));
        assert!(build_wake_message(&schedule(""), &[task("离线事件", true)]).is_none());
        assert!(build_wake_message(&schedule(""), &[]).unwrap().contains("默认职责"));
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
            )
            .unwrap();
        let report = wake.run_with_agent(&schedule, agent, "按默认职责巡检").await;
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
                "message.created",
                "turn.completed",
                "session.ended",
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "scheduled");
        assert_eq!(events.last().unwrap().payload_json["outcome"], "completed");
        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.kind, "scheduled");
        assert_eq!(session.status, "ended");
    }
}
