//! Legacy platform `task_approvals` projection.
//!
//! Task 19C2 removed every writer: the workspace `decisions` table is the only
//! approval source. This module keeps the row type for schema/tooling parity
//! only. It has **no production caller or reader** and is expected to be
//! deleted together with the platform table (L4, unwired surface).

use chrono::{DateTime, Utc};
use tinyiothub_core::task_approval::{ApprovalDecision, ApprovalState, TaskApproval, TaskApprovalStoreError};

fn storage(message: impl std::fmt::Display) -> TaskApprovalStoreError {
    TaskApprovalStoreError::Storage(message.to_string())
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TaskApprovalRow {
    id: String,
    workspace_id: String,
    task_id: String,
    run_id: String,
    tool_call_id: String,
    device_id: String,
    tool: String,
    args_json: String,
    reason: String,
    state: String,
    decision: Option<String>,
    decider_id: Option<String>,
    requested_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    decision_event_id: String,
    updated_at: DateTime<Utc>,
}

impl TryFrom<TaskApprovalRow> for TaskApproval {
    type Error = TaskApprovalStoreError;

    fn try_from(row: TaskApprovalRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            task_id: row.task_id,
            run_id: row.run_id,
            tool_call_id: row.tool_call_id,
            device_id: row.device_id,
            tool: row.tool,
            args_json: serde_json::from_str(&row.args_json).map_err(storage)?,
            reason: row.reason,
            state: ApprovalState::from_db(&row.state)
                .ok_or_else(|| TaskApprovalStoreError::Storage(format!("unknown approval state: {}", row.state)))?,
            decision: match row.decision.as_deref() {
                Some(value) => {
                    Some(ApprovalDecision::from_db(value).ok_or_else(|| {
                        TaskApprovalStoreError::Storage(format!("unknown approval decision: {value}"))
                    })?)
                }
                None => None,
            },
            decider_id: row.decider_id,
            requested_at: row.requested_at,
            decided_at: row.decided_at,
            expires_at: row.expires_at,
            decision_event_id: row.decision_event_id,
            updated_at: row.updated_at,
        })
    }
}
