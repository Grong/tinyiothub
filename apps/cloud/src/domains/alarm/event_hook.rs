//! Adapter implementing the event crate's `EventAlarmHook` for `AlarmService`.
//!
//! Keeps the edge one-way (alarm → event): the event ingest pipeline calls
//! this hook after persisting a thing event; the event crate never names
//! alarm types. Moved from `cloud::modules::alarm::event_hook` in P4-Task19
//! (this file is the alarm side of the seam Task 18 cut).

use crate::domains::event::router::EventAlarmHook;
use async_trait::async_trait;
use tinyiothub_core::models::event::EventLevel;

use crate::domains::alarm::service::AlarmService;

#[async_trait]
impl EventAlarmHook for AlarmService {
    async fn check_event_alarms(
        &self,
        workspace_id: &str,
        thing_id: &str,
        event_name: &str,
        event_level: &EventLevel,
        event_data: &serde_json::Value,
    ) -> Result<(), String> {
        AlarmService::check_event_alarms(self, workspace_id, thing_id, event_name, event_level, event_data)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
