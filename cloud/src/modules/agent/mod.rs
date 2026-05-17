// Agent module — capability-based architecture
// agent.rs:       Agent struct + AgentPool + zeroclaw Agent build
// chat/:          Chat capability (ChatService stateless + ChatHandler)
// tools/:         Tool capability (ToolService + CanvasTool + catalog)
// config/:        Config capability (ConfigService + ConfigHandler)
// session.rs:     SessionKey unified parse + verify_workspace
// skills.rs:      SkillsCache with TTL + async/sync loading
// memory.rs:      MemoryService (device snapshots)
// heartbeat.rs:   HeartbeatService (moved from shared/agent/)
// scaffold.rs:    Workspace scaffold + files CRUD

pub mod agent;
pub mod chat;
pub mod config;
pub mod tools;

pub mod heartbeat;
pub mod memory;
pub mod scaffold;
pub mod session;
pub mod skills;

// Re-exports from old modules/agent/ — kept until T7 migration
pub mod chat_service;
pub mod device_memory;
pub mod handler;
pub mod memory_service;
pub mod service;
pub mod skill;
pub mod types;

pub use chat_service::ChatService;
pub use device_memory::DeviceMemory;
pub use memory_service::AgentMemoryService;
pub use service::SessionService;
pub use skill::{AgentSkill, SkillType};
pub use types::*;
