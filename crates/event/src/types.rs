// Event module types
// Consolidated from domain/event/repositories/*.rs and domain/event/services/event_service.rs

use crate::value_objects::EventSource;

// Repository-side types (criteria/statistics/real-time status) live in the db
// crate (E3 集中化); re-exported for compatibility.
pub use tinyiothub_storage::event::*;

// ──────────────────────────────────────────────
// Event Pattern (from event_service.rs)
// ──────────────────────────────────────────────

/// Event pattern detection result
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub pattern_type: String,
    pub description: String,
    pub severity: String,
    pub event_count: usize,
    pub sources: Vec<EventSource>,
}

// ──────────────────────────────────────────────
// Tests (from event_repository.rs)
// ──────────────────────────────────────────────
