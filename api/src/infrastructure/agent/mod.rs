// Agent Runtime Module
//
// This module provides the AgentRuntime trait that consolidates agent functionality
// into a single interface for AppState.

use std::pin::Pin;
use std::sync::Arc;

use crate::infrastructure::zeroclaw_agent::{AgentClient, AgentConfig, AgentError, AgentInfo};

/// Trait that consolidates all agent runtime functionality
///
/// This trait is implemented by TinyIoTHubAgentClient and provides:
/// - All AgentClient operations (chat, history, config, tools)
/// - Tool refresh capability (refresh_tools)
pub trait AgentRuntime: AgentClient + Send + Sync {
    /// Refresh the agent's tool registry
    fn refresh_tools(&self) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>>;
}

// Re-export the concrete implementation
pub use crate::infrastructure::zeroclaw_runtime::TinyIoTHubAgentClient;
