// Agent module — 3-layer architecture
// types:  domain types and DTOs
// repo:   SessionRepository trait
// service: SessionService + compact logic
// chat_service: ChatService orchestration
// memory_service: AgentMemoryService
// handler: HTTP routes

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
