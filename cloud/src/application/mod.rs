// Application Layer
// This module contains application services and orchestration logic

pub mod agent;
pub mod cron_scheduler;
pub mod service_manager;

use std::sync::Arc;

use tinyiothub_storage::cache::DeviceCache;

/// Application context shared by plugins.
pub struct AppContext {
    pub device_cache: Arc<DeviceCache>,
}

pub use service_manager::ServiceManager;

// Re-export agent application services
pub use agent::{ChatService, ChatRequest, ChatEvent, ChatError, ChatStream};
pub use agent::{SessionService, SessionRepository, Session, ChatMessage, CompactedSession};
pub use agent::{AgentMemoryService, MemoryContext, DeviceSnapshot, AgentMemoryItem};
