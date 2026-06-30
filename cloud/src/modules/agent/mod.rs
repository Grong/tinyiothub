// Agent module — capability-based architecture
// agent.rs:       AgentPool + zeroclaw Agent build + skills loading
// chat/:          Chat capability (stateless ChatService + ChatHandler)
// tools/:         Tool capability (ToolService + CanvasTool + catalog)
// config/:        Config capability (ConfigService + ConfigHandler)
// session.rs:     SessionKey unified parse + verify_workspace
// heartbeat.rs:   HeartbeatService (uses AgentPool directly)
// scaffold.rs:    Workspace scaffold + files CRUD

#[allow(clippy::module_inception)]
pub mod agent;
pub mod chat;
pub mod config;
pub mod memory;
pub mod reflect;
pub mod tools;

pub mod dlq_repo;
pub mod heartbeat;
pub mod heartbeat_repo;
pub mod scaffold;
pub mod session;

// Re-exports from old modules/agent/ — kept for compat
pub mod handler;
pub mod service;
pub mod skill;
pub mod types;

pub use service::SessionService;
pub use skill::{AgentSkill, SkillType};
pub use types::*;
