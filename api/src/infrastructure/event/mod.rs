// Infrastructure Layer - Event System
// This module contains infrastructure concerns and external integrations

pub mod channels;
pub mod event_bus;
pub mod handlers;
pub mod performance;
pub mod security;
pub mod sse_manager;

// Export core event infrastructure
pub use event_bus::{EventBus, EventHandler};
// Export SSE connection manager
pub use sse_manager::SseConnectionManager;
