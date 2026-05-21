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
pub mod reflection;
pub mod tools;

pub mod heartbeat;
pub mod scaffold;
pub mod session;

// Re-exports from old modules/agent/ — kept for compat
pub mod device_memory;
pub mod handler;
pub mod memory_service;
pub mod service;
pub mod skill;
pub mod types;

pub use device_memory::DeviceMemory;
pub use memory_service::AgentMemoryService;
pub use service::SessionService;
pub use skill::{AgentSkill, SkillType};
pub use types::*;
