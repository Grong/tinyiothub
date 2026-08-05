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
    publisher: Arc<tinyiothub_agent::loop_::event::bus::AiEventPublisher>,
}

impl AlarmAiPublisherAdapter {
    pub fn new(publisher: Arc<tinyiothub_agent::loop_::event::bus::AiEventPublisher>) -> Self {
        Self { publisher }
    }
}

impl tinyiothub_alarm::AlarmAiPublisher for AlarmAiPublisherAdapter {
    fn publish_alarm_created(&self, event: tinyiothub_alarm::AlarmEvent) {
        self.publisher.publish(tinyiothub_agent::loop_::event::types::AiEvent::AlarmCreated(event));
    }
}
