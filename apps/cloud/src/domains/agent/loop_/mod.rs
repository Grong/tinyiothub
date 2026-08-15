//! AI subsystem for TinyIoTHub — agents, heartbeat, alarms, orchestration

pub mod agent;
pub mod event;
pub mod events;
pub mod heartbeat;
pub mod orchestrator;
pub mod thing_agent;
pub use tinyiothub_llm::{prompt, session};

/// Shared types re-exported at crate root for cross-domain use.
pub mod types {
    pub use crate::domains::agent::loop_::event::bus::{DropNotifier, LoggingDropNotifier};
    pub use crate::domains::agent::loop_::event::dlq::{DeadLetterEntry, DeadLetterQueue};
    pub use crate::domains::agent::loop_::event::types::AiEvent;
    pub use crate::domains::agent::loop_::heartbeat::metrics::{Metrics, MetricsSnapshot};
    pub use crate::domains::agent::loop_::heartbeat::types::{HeartbeatSignal, SignalPriority};
    pub use tinyiothub_core::heartbeat::{TrustConfig, TrustLevel};
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
}
