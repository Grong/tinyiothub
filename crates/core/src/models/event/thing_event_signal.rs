//! ThingEventSignal — thing 事件持久化后的进程内广播信号（值类型）。
//!
//! 由 event 管道（apps/cloud `domains::event`）生产、thing-agent loop
//! （crates/agent）消费；作为共享契约住 core，使两边都只依赖值类型层。

use serde_json::Value;

/// Signal broadcast on the thing-event bus after a thing event is persisted.
///
/// `actor == "agent"` marks events produced by agent actions (invoke_action
/// dispatch / heartbeat autonomous actions) — consumers must not wake the
/// loop on those (resonance guard, O21).
#[derive(Debug, Clone)]
pub struct ThingEventSignal {
    pub workspace_id: String,
    pub thing_id: String,
    pub event_name: String,
    /// Monotonic cursor (events.rowid) — NOT the UUID `events.id`, which is
    /// not orderable.
    pub event_id: i64,
    pub level: i32,
    pub data: Value,
    pub is_unknown: bool,
    pub actor: String,
}
