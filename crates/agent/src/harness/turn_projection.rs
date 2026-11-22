//! Replayable turn projection.
//!
//! The runner emits session events; this fold reconstructs the stable turn
//! phase and agenda from those events. It performs no I/O and can be used by a
//! snapshot cache or a replay test.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::turn_state::{AgendaItem, AgendaStatus, TurnPhase};

/// One projected todo item. The status vocabulary matches the model-facing
/// todo tool (`pending` / `in_progress` / `completed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedTodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectedTodo {
    pub content: String,
    pub status: ProjectedTodoStatus,
}

/// Current projection of one turn, advanced by session events in seq order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnProjection {
    pub turn_id: Option<String>,
    pub phase: TurnPhase,
    pub agenda: Vec<AgendaItem>,
    pub todos: Vec<ProjectedTodo>,
    pub last_seq: i64,
}

impl Default for TurnProjection {
    fn default() -> Self {
        Self {
            turn_id: None,
            phase: TurnPhase::Planning,
            agenda: Vec::new(),
            todos: Vec::new(),
            last_seq: 0,
        }
    }
}

impl TurnProjection {
    /// Apply one event. Events outside the turn projection are ignored.
    pub fn apply(&mut self, seq: i64, event_type: &str, payload: &Value) -> bool {
        if seq <= self.last_seq {
            return false;
        }
        let changed = match event_type {
            "turn/start" => {
                self.turn_id = payload.get("turnId").and_then(Value::as_str).map(str::to_string);
                self.phase = TurnPhase::Planning;
                self.todos.clear();
                true
            }
            "agenda/set" | "agenda/update" => self.apply_agenda(payload),
            "todo/write" => self.apply_todos(payload),
            "step/start" => {
                self.phase = TurnPhase::Executing;
                true
            }
            "turn/incomplete" => {
                self.phase = TurnPhase::Verifying;
                true
            }
            "turn/end" => {
                self.phase = match payload.get("outcome").and_then(Value::as_str) {
                    Some("completed") => TurnPhase::Completed,
                    Some("partial") => TurnPhase::Partial,
                    Some("cancelled") => TurnPhase::Cancelled,
                    _ => TurnPhase::Failed,
                };
                true
            }
            _ => false,
        };
        if changed {
            self.last_seq = seq;
        }
        changed
    }

    fn apply_todos(&mut self, payload: &Value) -> bool {
        let Some(items) = payload.get("todos").and_then(Value::as_array) else {
            return false;
        };
        self.todos = items
            .iter()
            .filter_map(|item| {
                let record = item.as_object()?;
                let content = record.get("content")?.as_str()?.to_string();
                let status = match record.get("status")?.as_str()? {
                    "in_progress" => ProjectedTodoStatus::InProgress,
                    "completed" => ProjectedTodoStatus::Completed,
                    _ => ProjectedTodoStatus::Pending,
                };
                Some(ProjectedTodo { content, status })
            })
            .collect();
        self.phase = TurnPhase::Executing;
        true
    }

    fn apply_agenda(&mut self, payload: &Value) -> bool {
        let Some(items) = payload.get("items").and_then(Value::as_array) else {
            return false;
        };
        let parsed = items
            .iter()
            .filter_map(|item| {
                let record = item.as_object()?;
                let key = record.get("key")?.as_str()?.to_string();
                let label = record.get("label")?.as_str()?.to_string();
                let status = match record.get("status")?.as_str()? {
                    "in_progress" => AgendaStatus::InProgress,
                    "completed" => AgendaStatus::Completed,
                    "skipped" => AgendaStatus::Skipped,
                    "blocked" => AgendaStatus::Blocked,
                    _ => AgendaStatus::Pending,
                };
                Some(AgendaItem { key, label, status })
            })
            .collect::<Vec<_>>();
        if parsed.is_empty() {
            return false;
        }
        self.agenda = parsed;
        if self.agenda.iter().any(|item| item.status == AgendaStatus::InProgress) {
            self.phase = TurnPhase::Executing;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn folds_agenda_updates_and_terminal_status() {
        let mut projection = TurnProjection::default();
        projection.apply(1, "turn/start", &json!({"turnId": "t1"}));
        projection.apply(
            2,
            "agenda/set",
            &json!({"items": [
                {"key": "inspection", "label": "巡检", "status": "pending"},
                {"key": "energy", "label": "节能", "status": "pending"}
            ]}),
        );
        projection.apply(
            3,
            "agenda/update",
            &json!({"items": [
                {"key": "inspection", "label": "巡检", "status": "completed"},
                {"key": "energy", "label": "节能", "status": "in_progress"}
            ]}),
        );
        projection.apply(
            4,
            "todo/write",
            &json!({"todos": [
                {"content": "扫描告警", "status": "completed"},
                {"content": "生成摘要", "status": "in_progress"}
            ]}),
        );
        projection.apply(5, "turn/end", &json!({"outcome": "partial"}));

        assert_eq!(projection.turn_id.as_deref(), Some("t1"));
        assert_eq!(projection.phase, TurnPhase::Partial);
        assert_eq!(projection.agenda[0].status, AgendaStatus::Completed);
        assert_eq!(projection.agenda[1].status, AgendaStatus::InProgress);
        assert_eq!(projection.todos[0].content, "扫描告警");
        assert_eq!(projection.todos[1].status, ProjectedTodoStatus::InProgress);
        assert_eq!(projection.last_seq, 5);
    }

    #[test]
    fn ignores_out_of_order_events() {
        let mut projection = TurnProjection::default();
        projection.apply(2, "turn/start", &json!({"turnId": "t1"}));
        assert!(!projection.apply(1, "turn/end", &json!({"outcome": "completed"})));
        assert_eq!(projection.phase, TurnPhase::Planning);
    }
}
