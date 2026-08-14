//! Agent-side implementation of the tenant domain's `AgentHooks` seam (G5b).
//!
//! Adapts the agent domain's heartbeat-task knowledge (the default task set)
//! to the tenant-owned trait so the tenant domain carries no dependency edge
//! on the agent domain. Dependency direction: agent → tenant.

use crate::domains::agent::host::heartbeat;

/// [`AgentHooks`](crate::domains::tenant::hooks::AgentHooks) backed by the
/// agent domain's real implementations.
#[derive(Default)]
pub struct AgentHooksImpl;

impl AgentHooksImpl {
    pub fn new() -> Self {
        Self
    }
}

impl crate::domains::tenant::hooks::AgentHooks for AgentHooksImpl {
    fn default_heartbeat_tasks(&self) -> Vec<crate::domains::tenant::hooks::HeartbeatTaskDef> {
        heartbeat::get_default_tasks()
            .into_iter()
            .map(|t| crate::domains::tenant::hooks::HeartbeatTaskDef {
                priority: t.priority,
                text: t.text,
                paused: t.paused,
            })
            .collect()
    }
}
