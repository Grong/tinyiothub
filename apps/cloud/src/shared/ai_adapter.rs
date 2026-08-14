//! Adapter: agent AiEventPublisher -> tinyiothub_alarm AlarmAiPublisher
//!
//! Composition-side implementation of the alarm crate's outbound port
//! (P4-Task19): `AlarmService` publishes significant alarms through
//! `AlarmAiPublisher` so the alarm crate never names agent types. The
//! `AlarmEvent` payload is shared via the event crate (P4-Task22), so no
//! field mapping is needed here.
//!
//! (The AgentPool -> AgentPoolLike half of the old ai_adapter moved into the
//! agent crate as `host::pool_adapter::HostAgentPoolAdapter` in P4-Task22.)

use std::sync::Arc;

pub struct AlarmAiPublisherAdapter {
    publisher: Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>,
}

impl AlarmAiPublisherAdapter {
    pub fn new(publisher: Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>) -> Self {
        Self { publisher }
    }
}

impl AlarmAiPublisherAdapter {
    pub fn publish_alarm_created(&self, event: crate::domains::alarm::AlarmEvent) {
        self.publisher
            .publish(crate::domains::agent::loop_::event::types::AiEvent::AlarmCreated(event));
    }
}

/// Composition-side implementation of the tenant domain's
/// [`WorkspaceEventPublisher`](crate::domains::tenant::hooks::WorkspaceEventPublisher)
/// port (G5b): workspace lifecycle events are forwarded onto the agent-owned
/// AI event plane, so the tenant domain never names agent types.
pub struct WorkspaceAiPublisherAdapter {
    publisher: Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>,
}

impl WorkspaceAiPublisherAdapter {
    pub fn new(publisher: Arc<crate::domains::agent::loop_::event::bus::AiEventPublisher>) -> Self {
        Self { publisher }
    }
}

impl crate::domains::tenant::hooks::WorkspaceEventPublisher for WorkspaceAiPublisherAdapter {
    fn publish_workspace_created(&self, workspace_id: String) {
        self.publisher
            .publish(crate::domains::agent::loop_::event::types::AiEvent::WorkspaceCreated {
                workspace_id,
            });
    }

    fn publish_workspace_deleted(&self, workspace_id: String) {
        self.publisher
            .publish(crate::domains::agent::loop_::event::types::AiEvent::WorkspaceDeleted {
                workspace_id,
            });
    }
}
