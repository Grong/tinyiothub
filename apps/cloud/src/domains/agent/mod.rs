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

// AppState 削除（F3 relay 化）：handler 直取 AppState。

/// The composed agent router: host (agent/chat capability HTTP APIs) + chat
/// (session proxy) planes, generic over the composition state `S`.
pub fn router() -> axum::Router<crate::state::AppState> {
    host::router()
}
