use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tinyiothub_core::models::event::{
    DeviceEventType, EventId, EventLevel, EventSource, EventType,
};

use crate::modules::alarm::service::AlarmService;

// ── Core types ──────────────────────────────────────────────────

/// Result of routing a single thing event through validation,
/// throttling, and persistence.
#[derive(Debug, Clone)]
pub struct EventRouteResult {
    pub event_id: String,
    pub throttled: bool,
    pub unknown_event: bool,
    pub malformed: bool,
}

/// Inbound payload for a thing event before routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingEventInput {
    pub thing_id: String,
    pub workspace_id: String,
    pub event_name: String,
    pub level: EventLevel,
    pub data: serde_json::Value,
    pub ts: Option<String>,
}

// ── Throttle state ──────────────────────────────────────────────

/// In-memory per-thing sliding-window throttle.
///
/// * Error and Critical events are **always** admitted (exempt).
/// * Info and Warning events share a per-thing bucket of
///   `max_per_minute` entries within a rolling 60 s window.
pub struct ThrottleState {
    windows: Arc<DashMap<String, VecDeque<Instant>>>,
    max_per_minute: usize,
}

impl ThrottleState {
    pub fn new(max_per_minute: usize) -> Self {
        Self { windows: Arc::new(DashMap::new()), max_per_minute }
    }

    /// Check whether `thing_id` with `level` should be admitted.
    ///
    /// Returns `true` when the event passes the throttle check.
    /// On success the current timestamp is recorded for the thing.
    pub fn check_and_record(&self, thing_id: &str, level: &EventLevel) -> bool {
        match level {
            EventLevel::Error | EventLevel::Critical => true,
            _ => {
                let now = Instant::now();
                let cutoff = now - Duration::from_secs(60);
                let mut entry = self.windows.entry(thing_id.to_string()).or_default();
                // Purge timestamps older than the window.
                while entry.front().map_or(false, |t| *t < cutoff) {
                    entry.pop_front();
                }
                if entry.len() >= self.max_per_minute {
                    false
                } else {
                    entry.push_back(now);
                    true
                }
            }
        }
    }

    /// Number of things currently tracked (for diagnostics).
    pub fn active_window_count(&self) -> usize {
        self.windows.len()
    }
}

// ── Payload deserialization helper ──────────────────────────────

/// The JSON shape arriving on the MQTT topic.
#[derive(Debug, Deserialize)]
pub struct ThingEventPayload {
    pub level: String,
    pub data: serde_json::Value,
    #[serde(default)]
    pub ts: Option<String>,
}

// ── Routing ─────────────────────────────────────────────────────

/// Single write entry point for all thing events.
///
/// 1. Validates required fields.
/// 2. Maps the level string to `EventLevel`.
/// 3. Checks the per-thing throttle.
/// 4. Inserts a row into the `events` table.
pub async fn route_thing_event(
    pool: &sqlx::SqlitePool,
    throttle: &ThrottleState,
    alarm_service: Option<Arc<AlarmService>>,
    input: ThingEventInput,
) -> EventRouteResult {
    // ── 1. Validate payload ────────────────────────────────
    if input.thing_id.is_empty() || input.event_name.is_empty() {
        tracing::warn!(
            thing_id = %input.thing_id,
            event_name = %input.event_name,
            "Malformed thing event: empty required field"
        );
        return EventRouteResult {
            event_id: String::new(),
            throttled: false,
            unknown_event: false,
            malformed: true,
        };
    }

    // ── 2. Unknown event name check (future schema registry) ──
    // TODO: plug in event-name whitelist / schema registry.
    let unknown_event = false;

    // ── 3. Throttle check ───────────────────────────────────
    if !throttle.check_and_record(&input.thing_id, &input.level) {
        tracing::warn!(
            thing_id = %input.thing_id,
            level = %input.level,
            "Thing event throttled ({} per 60 s limit)",
            throttle.max_per_minute
        );
        return EventRouteResult {
            event_id: String::new(),
            throttled: true,
            unknown_event: false,
            malformed: false,
        };
    }

    // ── 4. Persist into events table ────────────────────────
    let event_id = EventId::new();
    let event_id_str = event_id.to_string();
    let timestamp = input.ts.unwrap_or_else(|| Utc::now().to_rfc3339());

    let event_type = EventType::Device(DeviceEventType::DeviceNormal);
    let event_subtype = serde_json::to_string(&event_type).unwrap_or_default();
    let level_num = input.level.to_numeric();

    let source = EventSource::new(
        "thing".to_string(),
        format!("thing/{}", input.thing_id),
        Some(input.thing_id.clone()),
        None::<String>,
    );

    let content = serde_json::json!({
        "title": input.event_name,
        "elements": [{
            "Text": {
                "content": serde_json::to_string(&input.data).unwrap_or_default(),
                "format": "Plain"
            }
        }],
        "metadata": {}
    });

    let created_at = Utc::now().to_rfc3339();

    let result = sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, source_id, device_id, user_id, title, content, created_at, workspace_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&event_id_str)
    .bind("device")
    .bind(&event_subtype)
    .bind(level_num)
    .bind(&timestamp)
    .bind(source.source_type())
    .bind(source.source_id())
    .bind(source.device_id())
    .bind(source.user_id())
    .bind(&input.event_name)
    .bind(serde_json::to_string(&content).unwrap_or_default())
    .bind(&created_at)
    .bind(&input.workspace_id)
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            tracing::info!(
                event_id = %event_id_str,
                thing_id = %input.thing_id,
                event_name = %input.event_name,
                level = %input.level,
                "Thing event routed and persisted"
            );

            // Trigger event-based alarm rules if an AlarmService is available
            if let Some(ref svc) = alarm_service {
                if let Err(e) = svc
                    .check_event_alarms(
                        &input.workspace_id,
                        &input.thing_id,
                        &input.event_name,
                        &input.level,
                        &input.data,
                    )
                    .await
                {
                    tracing::error!(
                        thing_id = %input.thing_id,
                        event_name = %input.event_name,
                        error = %e,
                        "Failed to check event alarms"
                    );
                }
            }

            EventRouteResult {
                event_id: event_id_str,
                throttled: false,
                unknown_event,
                malformed: false,
            }
        }
        Err(e) => {
            tracing::error!(
                thing_id = %input.thing_id,
                event_name = %input.event_name,
                error = %e,
                "Failed to persist thing event"
            );
            EventRouteResult {
                event_id: String::new(),
                throttled: false,
                unknown_event: false,
                malformed: true,
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Throttle unit tests ──

    #[test]
    fn test_throttle_60_info_rejects_61st() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            assert!(ts.check_and_record("thing-1", &EventLevel::Info));
        }
        assert!(!ts.check_and_record("thing-1", &EventLevel::Info)); // 61st rejected
    }

    #[test]
    fn test_throttle_spares_critical() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            ts.check_and_record("thing-1", &EventLevel::Info);
        }
        assert!(ts.check_and_record("thing-1", &EventLevel::Critical)); // exempt
    }

    #[test]
    fn test_throttle_spares_error() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            ts.check_and_record("thing-1", &EventLevel::Info);
        }
        assert!(ts.check_and_record("thing-1", &EventLevel::Error)); // exempt
    }

    #[test]
    fn test_throttle_separate_per_thing() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            ts.check_and_record("thing-1", &EventLevel::Info);
        }
        assert!(ts.check_and_record("thing-2", &EventLevel::Info)); // different thing
    }

    #[test]
    fn test_throttle_warning_is_throttled() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            ts.check_and_record("thing-3", &EventLevel::Warning);
        }
        assert!(!ts.check_and_record("thing-3", &EventLevel::Warning));
    }

    #[test]
    fn test_throttle_error_always_passes() {
        let ts = ThrottleState::new(60);
        for _ in 0..100 {
            assert!(ts.check_and_record("thing-4", &EventLevel::Error));
        }
    }

    #[test]
    fn test_throttle_active_window_count() {
        let ts = ThrottleState::new(60);
        ts.check_and_record("thing-a", &EventLevel::Info);
        ts.check_and_record("thing-b", &EventLevel::Warning);
        assert_eq!(ts.active_window_count(), 2);
    }

    #[test]
    fn test_throttle_debug_is_throttled() {
        let ts = ThrottleState::new(60);
        for _ in 0..60 {
            ts.check_and_record("thing-5", &EventLevel::Debug);
        }
        assert!(!ts.check_and_record("thing-5", &EventLevel::Debug));
    }
}
