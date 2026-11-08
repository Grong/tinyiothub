use std::time::Duration;

pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const SESSION_TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub const SUBAGENT_TURN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub mod agent_host;
pub mod approvals;
pub mod chat;
pub mod chat_types;
pub mod config;
pub mod context;
pub mod events;
pub mod factory;
pub mod handler;
pub mod handlers;
pub mod hud;
pub mod index;
pub mod judge;
pub mod notify;
pub mod persist;
pub mod pool_adapter;
pub mod ports;
pub mod questions;
pub mod runner;
pub mod skills_seed;
pub mod subagent;
pub mod todos;
pub mod tool_guard;
pub mod tools;
pub mod trigger;
pub mod triggers;
pub mod types;
pub mod wake;
pub mod workspace_api;

#[cfg(test)]
pub(crate) mod test_utils;

pub use approvals::{ApprovalBroker, ApprovalDecision, ApprovalError, ApprovalSummary};
pub use context::*;
pub use events::*;
pub use questions::QuestionBroker;
pub use runner::{SessionRunner, TurnObserver, TurnOutcome};
pub use skills_seed::ensure_inspection_skill;
pub use subagent::{SubagentReport, SubagentRequest, SubagentRunner, SubagentSlot, SubagentStatus};
pub use todos::TodoStore;
pub use tool_guard::{APPROVAL_TIMEOUT, PermissionGuardTool};
pub use trigger::{TRIGGER_TICK_INTERVAL, TriggerRouter};
pub use types::*;
pub use wake::{WakeExecutor, WakeOutcome, WakeReport, WakeRunner, assistant_agent_id, build_wake_message};

use std::sync::Arc;

use crate::domains::agent::runtime::session_stream::SessionStreamHub;

#[derive(Clone)]
pub struct SessionRuntime {
    pub events: Arc<dyn SessionEventSink>,
    pub contexts: Arc<TurnContextRegistry>,
    pub todos: Arc<TodoStore>,
    pub questions: Arc<QuestionBroker>,
    pub approvals: Arc<ApprovalBroker>,
    pub subagents: Arc<SubagentSlot>,
    pub runner: Arc<SessionRunner>,
}

impl SessionRuntime {
    pub fn new(db: Arc<tinyiothub_storage::Db>, hub: Arc<SessionStreamHub>) -> Self {
        let events: Arc<dyn SessionEventSink> = Arc::new(DbSessionEventSink::new(db, hub));
        let contexts = Arc::new(TurnContextRegistry::default());
        let todos = Arc::new(TodoStore::default());
        let questions = Arc::new(QuestionBroker::default());
        let approvals = Arc::new(ApprovalBroker::default());
        let subagents = Arc::new(SubagentSlot::default());
        let runner = Arc::new(SessionRunner::new(events.clone(), contexts.clone()));
        Self {
            events,
            contexts,
            todos,
            questions,
            approvals,
            subagents,
            runner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_wires_shared_components() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool));
        let hub = Arc::new(crate::domains::agent::runtime::session_stream::SessionStreamHub::default());
        let runtime = SessionRuntime::new(db, hub);

        let context = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let guard = runtime.contexts.set(context);
        assert!(runtime.contexts.get("agent_main").is_some());
        drop(guard);
        assert!(runtime.contexts.get("agent_main").is_none());
        assert!(runtime.todos.get("agent:ws:agent_main/s1").is_empty());
    }
}
