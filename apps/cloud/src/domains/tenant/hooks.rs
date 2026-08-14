//! Tenant-owned hooks seam (G5b) — the traits through which the tenant
//! domain consumes agent-owned capabilities (the default heartbeat task
//! set) and publishes workspace lifecycle events onto the AI event plane.
//!
//! The tenant domain defines the traits; the agent domain implements
//! `AgentHooks` (`AgentHooksImpl`, agent-side host adapter) and the
//! composition layer implements `WorkspaceEventPublisher`
//! (`shared::ai_adapter::WorkspaceAiPublisherAdapter`), injecting both as
//! trait objects. Dependency direction: agent → tenant (never the reverse).

/// A heartbeat task definition crossing the tenant→agent boundary (value
/// type). Mirrors the agent-side task entry (priority/text/paused);
/// server-assigned fields (id, version, timestamps) never cross this seam.
#[derive(Debug, Clone)]
pub struct HeartbeatTaskDef {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Agent-owned capabilities consumed by the tenant domain's workspace
/// service.
pub trait AgentHooks: Send + Sync {
    /// The default heartbeat task set seeded into every new workspace.
    fn default_heartbeat_tasks(&self) -> Vec<HeartbeatTaskDef>;
}

/// Outbound port for workspace lifecycle events on the AI event plane.
/// Implemented by the composition layer over the agent-owned event bus.
pub trait WorkspaceEventPublisher: Send + Sync {
    /// A workspace was created (after the row is committed).
    fn publish_workspace_created(&self, workspace_id: String);

    /// A workspace was deleted (after the row is gone).
    fn publish_workspace_deleted(&self, workspace_id: String);
}
