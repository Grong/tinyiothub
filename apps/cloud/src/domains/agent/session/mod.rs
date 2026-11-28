use std::time::Duration;

pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Raw `turn.state` envelopes (state ref, payload refs, work keys, revisions)
/// are backend metadata. Every public Session Events surface filters them on
/// the server; the frontend is never the enforcement point.
pub fn is_public_session_event(event_type: &str) -> bool {
    event_type != "turn.state"
}

pub mod agenda;
pub mod agent_host;
pub mod approval_labels;
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

pub mod ports;
pub mod projection;
pub mod questions;
pub mod runner;
pub mod skill_sequencer;
pub mod skills_seed;
pub mod subagent;
pub mod suppression;
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

pub use approval_labels::{ApprovalLabel, ThingApprovalLabels};
pub use approvals::{ApprovalBroker, ApprovalDecision, ApprovalError, ApprovalSummary};
pub use context::*;
pub use events::*;
pub use questions::QuestionBroker;
pub use runner::{SessionRunner, TurnObserver, TurnOutcome};
pub use skill_sequencer::{SkillEntry, SkillOutcome, SkillSequencer};
pub use skills_seed::{
    ensure_builtin_skills, ensure_global_skills, ensure_inspection_skill, ensure_video_inspection_skill,
};
pub use subagent::{SubagentReport, SubagentRequest, SubagentRunner, SubagentSlot, SubagentStatus};
pub use todos::TodoStore;
pub use tool_guard::{APPROVAL_TIMEOUT, PermissionGuardTool};
pub use trigger::{TRIGGER_TICK_INTERVAL, TriggerRouter};
pub use types::*;
pub use wake::{WakeExecutor, WakeOutcome, WakeReport, WakeRunner, assistant_agent_id, build_wake_message};

use std::sync::Arc;

use crate::domains::agent::runtime::session_live::SessionLiveHub;
use crate::domains::agent::runtime::session_stream::SessionStreamHub;

#[derive(Clone)]
pub struct SessionRuntime {
    pub events: Arc<dyn SessionEventSink>,
    pub contexts: Arc<TurnContextRegistry>,
    pub todos: Arc<TodoStore>,
    pub questions: Arc<QuestionBroker>,
    pub approvals: Arc<ApprovalBroker>,
    pub subagents: Arc<SubagentSlot>,
    pub skills: Arc<SkillSequencer>,
    pub runner: Arc<SessionRunner>,
    pub live: Arc<SessionLiveHub>,
    pub system_prompts: Arc<tinyiothub_core::config::SystemPromptsConfig>,
}

impl SessionRuntime {
    pub fn new(db: Arc<tinyiothub_storage::Db>, hub: Arc<SessionStreamHub>) -> Self {
        let live = Arc::new(SessionLiveHub::default());
        let events: Arc<dyn SessionEventSink> = Arc::new(DbSessionEventSink::new(db, hub, live.clone()));
        let contexts = Arc::new(TurnContextRegistry::default());
        let todos = Arc::new(TodoStore::default());
        let questions = Arc::new(QuestionBroker::default());
        let approvals = Arc::new(ApprovalBroker::default());
        let subagents = Arc::new(SubagentSlot::default());
        let skills = Arc::new(SkillSequencer::new());
        let runner = Arc::new(
            SessionRunner::new(events.clone(), contexts.clone())
                .with_todos(todos.clone())
                .with_skills(skills.clone()),
        );
        Self {
            events,
            contexts,
            todos,
            questions,
            approvals,
            subagents,
            skills,
            runner,
            live,
            system_prompts: Arc::new(tinyiothub_core::config::SystemPromptsConfig::default()),
        }
    }

    pub fn with_system_prompts(mut self, system_prompts: tinyiothub_core::config::SystemPromptsConfig) -> Self {
        self.system_prompts = Arc::new(system_prompts);
        self
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
