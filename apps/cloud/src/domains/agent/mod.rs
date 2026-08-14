//! Agent domain for TinyIoTHub — loop + host + chat unified (P4-Task22).
//!
//! Three internal layers, one crate:
//!
//! - [`loop_`] — the agent runtime: thing-agent loop, orchestrator, heartbeat
//!   runner, AI event bus, agent pool contract. Pure domain/runtime code; it
//!   MUST NOT depend on web/axum (host → loop is one-way).
//! - [`host`] — the HTTP/service host: AgentPool, session/chat services,
//!   tools, handlers, repos, prompt scaffolding. Consumes `loop_`.
//! - [`chat`] — the chat session proxy plane (OpenClaw-facing handlers).
//!
//! ## 设计不变量
//! - 三层隔离：loop_（纯运行时，不依赖 web/axum）← host（HTTP/工具）+ chat
//! - 跨领域调用只许 agent→{event,thing,policy,memory,skills,llm,auth}

pub mod chat;
pub mod host;
pub mod loop_;

use std::sync::Arc;

use tinyiothub_storage::Database;

// ---------------------------------------------------------------------------
// Compat re-export surface (formerly `tinyiothub_ai::types` + llm re-exports).
// External consumers (mcp, tenant, cloud composition) import shared contract
// types through here so the internal loop_/host split stays an implementation
// detail.
// ---------------------------------------------------------------------------

pub use tinyiothub_llm::{prompt, session};

/// Shared types re-exported at crate root for cross-domain use.
pub mod types {
    pub use crate::domains::agent::loop_::event::bus::{DropNotifier, LoggingDropNotifier};
    pub use crate::domains::agent::loop_::event::dlq::{DeadLetterEntry, DeadLetterQueue};
    pub use crate::domains::agent::loop_::event::types::AiEvent;
    pub use crate::domains::agent::loop_::heartbeat::metrics::{Metrics, MetricsSnapshot};
    pub use crate::domains::agent::loop_::heartbeat::types::{HeartbeatSignal, SignalPriority};
    pub use tinyiothub_llm::prompt::PromptRegistry;
    pub use tinyiothub_llm::prompt::types::PromptTemplate;
    pub use tinyiothub_llm::provider::{LlmCallMetadata, LlmProvider, LlmResponse};
    pub use tinyiothub_memory::knowledge::{KnowledgeEntity, KnowledgeGraph, KnowledgeRelation, NoopKnowledgeGraph};
    pub use tinyiothub_memory::reflect::{
        build_reflection_input, build_reflection_prompt, contains_injection, parse_facts, sanitize_input,
    };
    pub use tinyiothub_memory::types::MemoryFact;
    pub use tinyiothub_policy::adapters::{ChatConfirmAdapter, ChatConfirmVerdict, HeartbeatTrustAdapter};
    pub use tinyiothub_policy::proposal::{Proposal, ProposalStatus};
    pub use tinyiothub_policy::{
        NoopPolicyEngine, PolicyAction, PolicyCategory, PolicyDecision, PolicyEngine, PolicyRule, evaluate_rules,
        sanitize_llm_input, target_matches, validate_llm_output,
    };
    pub use tinyiothub_skills::registry::{OutputSchema, ToolDescriptor, ToolParameter, ToolRegistry};
    pub use tinyiothub_skills::trust::{
        ToolSafety, TrustDecision, classify_tool_safety, evaluate_tool_trust, evaluate_tool_trust_with_safety,
        risk_for_tool,
    };
    pub use tinyiothub_storage::heartbeat::{TrustConfig, TrustLevel};
}

// AppState 削除（G7 FromRef 切片）：handler 萃取 `State<AgentState>`。

/// Agent domain state slice (G7) — the fields of cloud's `AppState` the
/// agent host/chat handlers actually consume. The composition layer (cloud)
/// derives it via `FromRef<AppState>`; this crate never names `AppState`.
#[derive(Clone)]
pub struct AgentState {
    /// 数据库连接池 - agent runs/policy/heartbeat SQL 查询
    pub database: Arc<Database>,
    /// 工作空间服务 - agent_tasks 的 verify_workspace_access! 租户校验
    pub workspace_service: Arc<crate::domains::tenant::WorkspaceService>,
    /// 工作空间访问校验（tenant seam）- files/heartbeat handlers
    pub workspace_access: Arc<crate::state::TenantWorkspaceAccess>,
    /// 用户指令投递入口 - agent_tasks 指令端点（None 时 503）
    pub directive_sink: Option<Arc<dyn loop_::thing_agent::DirectiveSink>>,
    /// 心跳运行器 - workspace heartbeat 配置/任务/信任 API
    pub heartbeat_runner: Option<Arc<loop_::heartbeat::runner::HeartbeatRunner>>,
    /// AI subsystem orchestrator - memory profile compile/weekly digest
    pub orchestrator: Option<Arc<loop_::orchestrator::Orchestrator>>,
    /// Agent 记忆存储 - memory handlers + chat prompt 构造
    pub memory_store: Arc<tinyiothub_storage::memory::MemoryStore>,
    /// Agent Pool - chat proxy 的会话/配置/工具 API
    pub agent_pool: Arc<host::agent::AgentPool>,
    /// 会话服务 - chat sessions 列表/标签/删除
    pub session_service: Arc<host::SessionService>,
    /// System prompts 配置 - chat proxy 构造 full prompt
    pub system_prompts: Arc<crate::shared::config::SystemPromptsConfig>,
}

impl AgentState {
    /// 获取数据库连接池
    pub fn db_pool(&self) -> sqlx::SqlitePool {
        self.database.pool().clone()
    }
}

/// The composed agent router: host (agent/chat capability HTTP APIs) + chat
/// (session proxy) planes, generic over the composition state `S`.
pub fn router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
    Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    host::router()
}
