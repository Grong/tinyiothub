// Agent host module — HTTP/services plane (P4-Task22, was cloud::modules::agent)
// agent.rs:       AgentPool + zeroclaw Agent build + skills loading
// chat/:          Chat capability (stateless ChatService + ChatHandler)
// tools/:         Tool capability (ToolService + CanvasTool + catalog)
// config/:        Config capability (ConfigService + ConfigHandler)
// session.rs:     SessionKey unified parse + verify_workspace
// heartbeat.rs:   HeartbeatService (uses AgentPool directly)
// scaffold.rs:    Workspace scaffold + files CRUD
// shared/:        Agent config types + system prompt building + fs paths
// state.rs:       AppState composition slice
// ports.rs:       Composition seams (external tools / workspace access / config)

#[allow(clippy::module_inception)]
pub mod agent;
pub mod autonomous_factory;
pub mod chat;
pub mod config;
pub mod memory;
pub mod ports;
pub mod reflect;
pub mod shared;
pub mod tools;

pub mod agent_hooks;
// Test stub, also used by cloud's integration tests (agent_tasks_api_tests)
// — compiled unconditionally so downstream test harnesses can consume it.
pub mod directive_sink;
pub mod dlq_repo;
pub mod heartbeat;
pub mod policy_engine;
pub mod pool_adapter;
pub mod scaffold;
pub mod session;
pub mod thing_action_hooks;
pub mod thing_agent_host;

#[cfg(test)]
pub(crate) mod test_utils;

// Re-exports from old modules/agent/ — kept for compat
pub mod handler;
pub mod service;
pub mod skill;
pub mod types;

pub use service::SessionService;
pub use skill::{AgentSkill, SkillType};
pub use types::*;
