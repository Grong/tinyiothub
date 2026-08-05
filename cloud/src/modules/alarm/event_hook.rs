//! Adapter implementing the event crate's `EventAlarmHook` for `AlarmService`.
//!
//! Keeps the edge one-way (alarm → event): the event ingest pipeline calls
//! this hook after persisting a thing event; the event crate never names
//! alarm types.
//!
//! Reclaim task: Task 19 (alarm crate extraction) moves this impl into the
//! alarm crate.

use async_trait::async_trait;
use tinyiothub_core::models::event::EventLevel;
use tinyiothub_event::router::EventAlarmHook;

use super::service::AlarmService;

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
        AlarmService::check_event_alarms(
            self,
            workspace_id,
            thing_id,
            event_name,
            event_level,
            event_data,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}
