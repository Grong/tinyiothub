// Application Layer
// This module contains application services and orchestration logic

pub mod agent;
pub mod cron;
pub mod data_context;
pub mod data_server;
pub mod message_server;
pub mod service_manager;

pub use data_context::DataContext;

use std::sync::Arc;

/// 应用上下文（所有插件共享）
pub struct AppContext {
    pub data_context: Arc<DataContext>,
}
pub use data_server::DataServer;
pub use service_manager::ServiceManager;

// Re-export agent application services
pub use agent::{ChatService, ChatRequest, ChatEvent, ChatError, ChatStream};
pub use agent::{SessionService, SessionRepository, Session, ChatMessage, CompactedSession};
pub use agent::{AgentMemoryService, MemoryContext, DeviceSnapshot, AgentMemoryItem};
