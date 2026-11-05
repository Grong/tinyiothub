//! Canonical projection state and hash contract (Spec T18).
//!
//! Any component that writes `projection_snapshots.state_json` — a future
//! production projection writer or a rebuild command — MUST build the state
//! with [`canonical_state`] and verify it with [`canonical_hash`]. The
//! comparison inspector uses exactly the same functions, so a writer cannot
//! drift silently while keeping the same `projection_version`.
//!
//! Shape (backward-compatible with existing test snapshots):
//!
//! ```json
//! {
//!   "projectionName": "supervisor",
//!   "subjectId": "supervisor:<workspace_id>",
//!   "lastSeq": 42,
//!   "events": [
//!     {
//!       "seq": 41,
//!       "eventId": "event-1",
//!       "type": "step.text",
//!       "occurredAt": "2026-11-03T00:00:00Z",
//!       "causeId": null,
//!       "payloadDigest": "sha256:…",
//!       "payload": { "text": "public event payload" }
//!     }
//!   ]
//! }
//! ```
//!
//! Derived fields (for example a snapshot `updated_at`) are deliberately not
//! part of the canonical document. Changing the shape requires a new
//! `projection_version`; otherwise an existing snapshot would compare as a
//! `hash_mismatch` instead of being silently accepted.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::HarnessEvent;

/// Canonical document for one fixed boundary. Event order is the persisted
/// `seq` order supplied by the caller.
pub fn canonical_state(projection_name: &str, subject_id: &str, last_seq: i64, events: &[HarnessEvent]) -> Value {
    let events: Vec<Value> = events.iter().map(canonical_event).collect();
    json!({
        "projectionName": projection_name,
        "subjectId": subject_id,
        "lastSeq": last_seq,
        "events": events,
    })
}

fn canonical_event(event: &HarnessEvent) -> Value {
    json!({
        "seq": event.seq,
        "eventId": event.event_id,
        "type": event.event_type,
        "occurredAt": event.occurred_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        "causeId": event.cause_id,
        "payloadDigest": event.payload_digest,
        "payload": event.payload_json,
    })
}

/// Deterministic JSON serialization: object keys are sorted recursively.
pub fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort_unstable();
            let mut sorted = Map::with_capacity(keys.len());
            for key in keys {
                if let Some(value) = object.get(key) {
                    sorted.insert(key.clone(), canonical_value(value));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_value).collect()),
        _ => value.clone(),
    }
}

/// SHA-256 over the canonical JSON bytes. `serde_json::Value` serialization
/// cannot fail, so hashing never silently falls back to empty bytes.
pub fn canonical_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(&canonical_value(value)).expect("serde_json::Value serialization cannot fail");
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{canonical_hash, canonical_value};

    #[test]
    fn canonical_hash_ignores_object_key_order_but_not_values() {
        let left = json!({"b": [{"d": 1, "c": 2}], "a": true});
        let right = json!({"a": true, "b": [{"c": 2, "d": 1}]});
        assert_eq!(canonical_hash(&left), canonical_hash(&right));
        assert_ne!(canonical_hash(&left), canonical_hash(&json!({"a": true})));
    }

    #[test]
    fn canonical_value_sorts_nested_keys_and_keeps_array_order() {
        let value = canonical_value(&json!({"z": 1, "a": {"d": 2, "c": 3}, "items": [3, 1]}));
        assert_eq!(
            serde_json::to_string(&value).expect("value"),
            r#"{"a":{"c":3,"d":2},"items":[3,1],"z":1}"#
        );
    }
}
