// Agent module — 3-layer architecture
// types:  domain types and DTOs
// repo:   SessionRepository trait
// service: SessionService + compact logic
// chat_service: ChatService orchestration
// memory_service: AgentMemoryService
// handler: HTTP routes

pub mod types;
pub mod service;
pub mod chat_service;
pub mod memory_service;
pub mod device_memory;
pub mod skill;
pub mod handler;

pub use types::*;
pub use service::SessionService;
pub use chat_service::ChatService;
pub use memory_service::AgentMemoryService;
pub use device_memory::DeviceMemory;
pub use skill::{AgentSkill, SkillType};
