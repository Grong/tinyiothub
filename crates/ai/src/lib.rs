//! AI subsystem for TinyIoTHub — agents, heartbeat, alarms, memory, tools

pub mod agent;
pub mod alarm;
pub mod event;
pub mod heartbeat;
pub mod knowledge;
pub mod memory;
pub mod orchestrator;
pub mod policy;
pub mod proposal;
pub use tinyiothub_llm::{prompt, session};
pub mod skills;
pub mod thing_agent;
pub mod tool;

/// Shared types re-exported at crate root for cross-domain use.
pub mod types {
    pub use crate::event::bus::{DropNotifier, LoggingDropNotifier};
    pub use crate::event::dlq::{DeadLetterEntry, DeadLetterQueue};
    pub use crate::event::types::AiEvent;
    pub use crate::heartbeat::metrics::{Metrics, MetricsSnapshot};
    pub use crate::heartbeat::types::{HeartbeatSignal, SignalPriority};
    pub use crate::knowledge::{KnowledgeEntity, KnowledgeGraph, KnowledgeRelation, NoopKnowledgeGraph};
    pub use crate::memory::provider::{LlmCallMetadata, LlmProvider, LlmResponse};
    pub use crate::memory::reflect::{
        build_reflection_input, build_reflection_prompt, contains_injection, parse_facts, sanitize_input,
    };
    pub use crate::memory::types::MemoryFact;
    pub use crate::policy::adapters::{ChatConfirmAdapter, ChatConfirmVerdict, HeartbeatTrustAdapter};
    pub use crate::policy::{
        NoopPolicyEngine, PolicyAction, PolicyCategory, PolicyDecision, PolicyEngine, PolicyRule, evaluate_rules,
        sanitize_llm_input, target_matches, validate_llm_output,
    };
    pub use tinyiothub_llm::prompt::PromptRegistry;
    pub use tinyiothub_llm::prompt::types::PromptTemplate;
    pub use crate::proposal::{Proposal, ProposalStatus};
    pub use crate::tool::registry::{OutputSchema, ToolDescriptor, ToolParameter, ToolRegistry};
    pub use crate::tool::trust::{
        ToolSafety, TrustConfig, TrustDecision, TrustLevel, classify_tool_safety, evaluate_tool_trust,
        evaluate_tool_trust_with_safety, risk_for_tool,
    };
}
