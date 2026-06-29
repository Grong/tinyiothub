//! AI subsystem for TinyIoTHub — agents, patrol, alarms, memory, tools

pub mod agent;
pub mod alarm;
pub mod event;
pub mod memory;
pub mod orchestrator;
pub mod patrol;
pub mod session;
pub mod tool;

/// Shared types re-exported at crate root for cross-domain use.
pub mod types {
    pub use crate::event::types::AiEvent;
    pub use crate::patrol::types::{TrustConfig, TrustLevel, WakePriority, WakeSignal};
}

/// Build the full AI subsystem and return the orchestrator handle.
pub struct AiSystem {
    pub orchestrator: std::sync::Arc<orchestrator::Orchestrator>,
    pub agent_pool: std::sync::Arc<agent::pool::AgentPool>,
    pub patrol_manager: std::sync::Arc<patrol::manager::PatrolManager>,
}

impl AiSystem {
    pub async fn shutdown(&self) {
        self.orchestrator.shutdown().await;
    }
}
