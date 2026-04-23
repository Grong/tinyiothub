// Infrastructure Layer - Event System
// This module contains infrastructure concerns and external integrations

pub mod channels;
pub mod handlers;
pub mod performance;
pub mod security;
pub mod sse_manager;

// Re-export core event infrastructure from tinyiothub-engine
pub use tinyiothub_engine::event_bus::{EventBus, EventHandler};
// Export SSE connection manager
pub use sse_manager::SseConnectionManager;
