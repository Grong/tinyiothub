// Application Layer - Agent Subdomain
// This module provides application services for AI Agent chat functionality

pub mod chat_service;
pub mod session_service;
pub mod memory_service;

pub use chat_service::{ChatService, ChatRequest, ChatEvent, ChatError, ChatStream};
pub use session_service::{SessionService, SessionRepository, Session, ChatMessage, CompactedSession};
pub use memory_service::{AgentMemoryService, MemoryContext, DeviceSnapshot, AgentMemoryItem};
