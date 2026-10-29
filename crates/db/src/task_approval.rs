use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
use tinyiothub_core::task_approval::{
    ApprovalDecision, ApprovalState, DecisionOutcome, TaskApproval, TaskApprovalStoreError,
};

use crate::Db;
use crate::runtime_event::{RuntimeEvent, RuntimeEventInput, append_runtime_event_tx, task_event};

#[cfg(test)]
tokio::task_local! {
    static CANCEL_LOCK_TEST_PAUSE: (
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    );
}

#[cfg(test)]
async fn pause_cancel_for_lock_test() {
    if let Ok((paused, resume, attempted)) = CANCEL_LOCK_TEST_PAUSE.try_with(Clone::clone)
        && !attempted.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        paused.notify_one();
        resume.notified().await;
    }
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

fn storage(error: impl std::fmt::Display) -> TaskApprovalStoreError {
    TaskApprovalStoreError::Storage(error.to_string())
}

fn is_transient_lock(error: &TaskApprovalStoreError) -> bool {
    match error {
        TaskApprovalStoreError::Storage(message) => {
            message.contains("(code: 5)")
                || message.contains("(code: 6)")
                || message.contains("database is locked")
                || message.contains("database is deadlocked")
        }
        _ => false,
    }
}

pub(crate) const APPROVAL_COLUMNS: &str = "id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json,
        reason, state, decision, decider_id, requested_at, decided_at, expires_at, decision_event_id, updated_at";

async fn find_scoped(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    task_id: &str,
    approval_id: &str,
) -> Result<Option<TaskApproval>, TaskApprovalStoreError> {
    sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {APPROVAL_COLUMNS} FROM task_approvals WHERE workspace_id = ? AND task_id = ? AND id = ?"
    )))
    .bind(workspace_id)
    .bind(task_id)
    .bind(approval_id)
    .fetch_optional(connection)
    .await
    .map_err(storage)?
    .map(TaskApproval::try_from)
    .transpose()
}

async fn begin_immediate(pool: &SqlitePool) -> Result<SqliteConnection, TaskApprovalStoreError> {
    let mut connection = pool.acquire().await.map_err(storage)?.detach();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut connection)
        .await
        .map_err(storage)?;
    Ok(connection)
}

async fn finish_immediate<T>(
    mut connection: SqliteConnection,
    result: Result<T, TaskApprovalStoreError>,
) -> Result<T, TaskApprovalStoreError> {
    match result {
        Ok(value) => {
            sqlx::query("COMMIT").execute(&mut connection).await.map_err(storage)?;
            Ok(value)
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
            Err(error)
        }
    }
}

fn decision_wake(approval: &TaskApproval, now: DateTime<Utc>) -> TaskWake {
    TaskWake {
        task_id: approval.task_id.clone(),
        event_id: approval.decision_event_id.clone(),
        signal_json: json!({
            "kind":"approval_decided",
            "workspace_id":approval.workspace_id,
            "task_id":approval.task_id,
            "approval_id":approval.id
        })
        .to_string(),
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    }
}

async fn insert_decision_wake(
    connection: &mut SqliteConnection,
    wake: &TaskWake,
) -> Result<(), TaskApprovalStoreError> {
    sqlx::query(
        "INSERT INTO task_wake_outbox
            (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&wake.task_id)
    .bind(&wake.event_id)
    .bind(&wake.signal_json)
    .bind(wake.dispatch_state.as_str())
    .bind(wake.created_at)
    .bind(wake.dispatched_at)
    .bind(wake.resolved_at)
    .execute(connection)
    .await
    .map_err(storage)?;
    Ok(())
}

fn approval_event(approval: &TaskApproval, suffix: &str) -> RuntimeEventInput {
    RuntimeEventInput {
        event_id: format!("approval:{}:{suffix}", approval.id),
        event_type: format!("approval.{suffix}"),
        occurred_at: approval.updated_at,
        workspace_id: approval.workspace_id.clone(),
        task_id: Some(approval.task_id.clone()),
        run_id: Some(approval.run_id.clone()),
        tool_call_id: Some(approval.tool_call_id.clone()),
        job_id: None,
        approval_id: Some(approval.id.clone()),
        device_id: Some(approval.device_id.clone()),
        source_id: None,
        source_seq: None,
        evidence_id: None,
        data: json!({
            "state": approval.state.as_str(),
            "decisionEventId": approval.decision_event_id,
        }),
    }
}

impl Db {
    pub async fn create_task_approval_and_events(
        &self,
        approval: &TaskApproval,
        expected_version: i64,
    ) -> Result<(AgentTask, Vec<RuntimeEvent>), TaskApprovalStoreError> {
        if approval.state != ApprovalState::Pending {
            return Err(TaskApprovalStoreError::Storage("new approval is not pending".into()));
        }
        let mut connection = begin_immediate(self.pool()).await?;
        let result = async {
            let task = crate::runtime_task::find_task(&mut connection, &approval.workspace_id, &approval.task_id)
                .await
                .map_err(storage)?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            if task.state_version != expected_version {
                return Err(TaskApprovalStoreError::Storage("task changed concurrently".into()));
            }
            if !task.status.can_transition_to(TaskStatus::NeedsApproval) {
                return Err(TaskApprovalStoreError::Storage("task cannot wait for approval".into()));
            }
            sqlx::query(
                "INSERT INTO task_approvals
                    (id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json, reason,
                     state, decision, decider_id, requested_at, decided_at, expires_at, decision_event_id, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&approval.id)
            .bind(&approval.workspace_id)
            .bind(&approval.task_id)
            .bind(&approval.run_id)
            .bind(&approval.tool_call_id)
            .bind(&approval.device_id)
            .bind(&approval.tool)
            .bind(serde_json::to_string(&approval.args_json).map_err(storage)?)
            .bind(&approval.reason)
            .bind(approval.state.as_str())
            .bind(approval.decision.map(ApprovalDecision::as_str))
            .bind(&approval.decider_id)
            .bind(approval.requested_at)
            .bind(approval.decided_at)
            .bind(approval.expires_at)
            .bind(&approval.decision_event_id)
            .bind(approval.updated_at)
            .execute(&mut connection)
            .await
            .map_err(storage)?;
            let now = Utc::now();
            let changed = sqlx::query(
                "UPDATE agent_tasks SET status = 'needs_approval', state_version = state_version + 1, updated_at = ?
                 WHERE workspace_id = ? AND id = ? AND state_version = ?",
            )
            .bind(now)
            .bind(&approval.workspace_id)
            .bind(&approval.task_id)
            .bind(expected_version)
            .execute(&mut connection)
            .await
            .map_err(storage)?
            .rows_affected();
            if changed != 1 {
                return Err(TaskApprovalStoreError::Storage("task changed concurrently".into()));
            }
            let task = crate::runtime_task::find_task(&mut connection, &approval.workspace_id, &approval.task_id)
                .await
                .map_err(storage)?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            let task_event = append_runtime_event_tx(
                &mut connection,
                task_event(
                    &task,
                    "task.needs_approval",
                    format!("task:{}:v{}:needs_approval", task.id, task.state_version),
                ),
            )
            .await
            .map_err(storage)?;
            let approval_event = append_runtime_event_tx(&mut connection, approval_event(approval, "requested"))
                .await
                .map_err(storage)?;
            Ok((task, vec![task_event, approval_event]))
        }
        .await;
        let outcome = finish_immediate(connection, result).await;
        if outcome.is_ok() {
            tracing::info!(
                metric = "runtime_approval_requested",
                workspace_id = %approval.workspace_id,
                task_id = %approval.task_id,
                approval_id = %approval.id,
                device_id = %approval.device_id,
                tool = %approval.tool,
                "approval requested"
            );
        }
        outcome
    }

    pub async fn task_approval_scoped(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
    ) -> Result<Option<TaskApproval>, TaskApprovalStoreError> {
        let mut connection = self.pool().acquire().await.map_err(storage)?.detach();
        find_scoped(&mut connection, workspace_id, task_id, approval_id).await
    }

    pub async fn task_approval_by_decision_event(
        &self,
        event_id: &str,
    ) -> Result<Option<TaskApproval>, TaskApprovalStoreError> {
        sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPROVAL_COLUMNS} FROM task_approvals WHERE decision_event_id = ?"
        )))
        .bind(event_id)
        .fetch_optional(self.pool())
        .await
        .map_err(storage)?
        .map(TaskApproval::try_from)
        .transpose()
    }

    pub async fn decide_task_approval_and_event(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        decider_id: &str,
        now: DateTime<Utc>,
        wake: &TaskWake,
    ) -> Result<(DecisionOutcome, Option<RuntimeEvent>), TaskApprovalStoreError> {
        if wake.task_id != task_id {
            return Err(TaskApprovalStoreError::Storage("approval wake task differs".into()));
        }
        let signal: serde_json::Value = serde_json::from_str(&wake.signal_json).map_err(storage)?;
        if signal.get("workspace_id").and_then(serde_json::Value::as_str) != Some(workspace_id) {
            return Err(TaskApprovalStoreError::Storage(
                "approval wake workspace differs".into(),
            ));
        }
        let mut connection = begin_immediate(self.pool()).await?;
        let result = async {
            let current = find_scoped(&mut connection, workspace_id, task_id, approval_id)
                .await?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            if wake.event_id != current.decision_event_id {
                return Err(TaskApprovalStoreError::Storage(
                    "approval decision event differs".into(),
                ));
            }
            match current.state {
                ApprovalState::Approved | ApprovalState::Rejected => {
                    if current.decision != Some(decision) {
                        return Err(TaskApprovalStoreError::DecisionConflict);
                    }
                    return Ok((
                        DecisionOutcome {
                            approval: current,
                            deduplicated: true,
                        },
                        None,
                    ));
                }
                ApprovalState::Expired | ApprovalState::Unavailable => {
                    return Ok((
                        DecisionOutcome {
                            approval: current,
                            deduplicated: true,
                        },
                        None,
                    ));
                }
                ApprovalState::Pending => {}
            }
            let (next, recorded_decision) = if current.expires_at <= now {
                (ApprovalState::Expired, None)
            } else {
                let task = crate::runtime_task::find_task(&mut connection, workspace_id, task_id)
                    .await
                    .map_err(storage)?
                    .ok_or(TaskApprovalStoreError::NotFound)?;
                if !matches!(task.status, TaskStatus::NeedsApproval | TaskStatus::Paused) {
                    return Err(TaskApprovalStoreError::DecisionConflict);
                }
                (
                    match decision {
                        ApprovalDecision::Approve => ApprovalState::Approved,
                        ApprovalDecision::Reject => ApprovalState::Rejected,
                    },
                    Some(decision),
                )
            };
            let changed = sqlx::query(
                "UPDATE task_approvals
                 SET state = ?, decision = ?, decider_id = ?, decided_at = ?, updated_at = ?
                 WHERE id = ? AND state = 'pending'",
            )
            .bind(next.as_str())
            .bind(recorded_decision.map(ApprovalDecision::as_str))
            .bind(decider_id)
            .bind(now)
            .bind(now)
            .bind(approval_id)
            .execute(&mut connection)
            .await
            .map_err(storage)?
            .rows_affected();
            if changed != 1 {
                return Err(TaskApprovalStoreError::DecisionConflict);
            }
            insert_decision_wake(&mut connection, wake).await?;
            let approval = find_scoped(&mut connection, workspace_id, task_id, approval_id)
                .await?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            let event = append_runtime_event_tx(&mut connection, approval_event(&approval, next.as_str()))
                .await
                .map_err(storage)?;
            Ok((
                DecisionOutcome {
                    approval,
                    deduplicated: false,
                },
                Some(event),
            ))
        }
        .await;
        let outcome = finish_immediate(connection, result).await;
        match &outcome {
            Ok((decision, _)) => tracing::info!(
                metric = "runtime_approval_decided",
                workspace_id = %workspace_id,
                task_id = %task_id,
                approval_id = %approval_id,
                state = %decision.approval.state.as_str(),
                deduplicated = decision.deduplicated,
                "approval decision"
            ),
            Err(TaskApprovalStoreError::DecisionConflict) => tracing::warn!(
                metric = "runtime_approval_conflict",
                workspace_id = %workspace_id,
                task_id = %task_id,
                approval_id = %approval_id,
                "approval decision conflict"
            ),
            Err(_) => {}
        }
        outcome
    }

    pub async fn list_task_approvals(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<Vec<TaskApproval>, TaskApprovalStoreError> {
        sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPROVAL_COLUMNS} FROM task_approvals
             WHERE workspace_id = ? AND task_id = ? ORDER BY requested_at, id"
        )))
        .bind(workspace_id)
        .bind(task_id)
        .fetch_all(self.pool())
        .await
        .map_err(storage)?
        .into_iter()
        .map(TaskApproval::try_from)
        .collect()
    }

    pub async fn list_recoverable_task_approvals(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TaskApproval>, TaskApprovalStoreError> {
        sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPROVAL_COLUMNS} FROM task_approvals a
             WHERE a.state != 'pending' AND (? IS NULL OR a.workspace_id = ?)
               AND EXISTS (SELECT 1 FROM task_wake_outbox w
                   WHERE w.task_id = a.task_id AND w.event_id = a.decision_event_id
                     AND w.dispatch_state = 'pending' AND w.resolved_at IS NULL)
             ORDER BY a.updated_at, a.id"
        )))
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_all(self.pool())
        .await
        .map_err(storage)?
        .into_iter()
        .map(TaskApproval::try_from)
        .collect()
    }

    pub async fn expire_task_approvals_and_events(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(u64, Vec<RuntimeEvent>), TaskApprovalStoreError> {
        let mut connection = begin_immediate(self.pool()).await?;
        let result = async {
            let due = sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {APPROVAL_COLUMNS} FROM task_approvals WHERE state = 'pending' AND expires_at <= ? ORDER BY expires_at, id"
            )))
            .bind(now)
            .fetch_all(&mut connection)
            .await
            .map_err(storage)?
            .into_iter()
            .map(TaskApproval::try_from)
            .collect::<Result<Vec<_>, _>>()?;
            let mut events = Vec::with_capacity(due.len());
            for approval in due {
                let changed = sqlx::query(
                    "UPDATE task_approvals SET state = 'expired', decided_at = ?, updated_at = ? WHERE id = ? AND state = 'pending'",
                )
                .bind(now)
                .bind(now)
                .bind(&approval.id)
                .execute(&mut connection)
                .await
                .map_err(storage)?
                .rows_affected();
                if changed != 1 {
                    continue;
                }
                insert_decision_wake(&mut connection, &decision_wake(&approval, now)).await?;
                let expired = find_scoped(&mut connection, &approval.workspace_id, &approval.task_id, &approval.id)
                    .await?
                    .ok_or(TaskApprovalStoreError::NotFound)?;
                events.push(
                    append_runtime_event_tx(&mut connection, approval_event(&expired, "expired"))
                        .await
                        .map_err(storage)?,
                );
            }
            Ok((events.len() as u64, events))
        }
        .await;
        let outcome = finish_immediate(connection, result).await;
        if let Ok((expired, _)) = &outcome
            && *expired > 0
        {
            tracing::info!(metric = "runtime_approval_expired", count = *expired, "approval expiry");
        }
        outcome
    }

    pub async fn submit_approved_job_and_events(
        &self,
        job: &DeviceJob,
    ) -> Result<(Option<DeviceJob>, bool, Vec<RuntimeEvent>), DeviceJobError> {
        if job.state != DeviceJobState::Accepted {
            return Err(DeviceJobError::Validation("approved job must be accepted".into()));
        }
        let tool_call_id = job
            .tool_call_id
            .as_deref()
            .ok_or_else(|| DeviceJobError::Validation("approved job needs a tool call".into()))?;
        let task_id = job
            .task_id
            .as_deref()
            .ok_or_else(|| DeviceJobError::Validation("approved job needs a task".into()))?;
        let run_id = job
            .run_id
            .as_deref()
            .ok_or_else(|| DeviceJobError::Validation("approved job needs a run".into()))?;
        let mut connection = self
            .pool()
            .acquire()
            .await
            .map_err(crate::device_job::storage)?
            .detach();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut connection)
            .await
            .map_err(crate::device_job::storage)?;
        let result = async {
            let existing = sqlx::query_as::<_, crate::device_job::DeviceJobRow>(
                "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                        progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
                 FROM device_jobs WHERE workspace_id = ? AND tool_call_id = ?",
            )
            .bind(&job.workspace_id)
            .bind(tool_call_id)
            .fetch_optional(&mut connection)
            .await
            .map_err(crate::device_job::storage)?
            .map(DeviceJob::try_from)
            .transpose()?;
            if let Some(existing) = existing {
                let matches = existing.workspace_id == job.workspace_id
                    && existing.task_id.as_deref() == Some(task_id)
                    && existing.run_id.as_deref() == Some(run_id)
                    && existing.device_id == job.device_id
                    && existing.tool == job.tool
                    && existing.args_json == job.args_json;
                if !matches {
                    return Err(DeviceJobError::ExecutionConflict);
                }
                return Ok((Some(existing), false, Vec::new()));
            }
            let approval = sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {APPROVAL_COLUMNS} FROM task_approvals WHERE workspace_id = ? AND tool_call_id = ?"
            )))
            .bind(&job.workspace_id)
            .bind(tool_call_id)
            .fetch_optional(&mut connection)
            .await
            .map_err(crate::device_job::storage)?
            .map(TaskApproval::try_from)
            .transpose()
            .map_err(crate::device_job::storage)?;
            let Some(approval) = approval else {
                return Ok((None, false, Vec::new()));
            };
            if approval.state != ApprovalState::Approved {
                return Ok((None, false, Vec::new()));
            }
            let matches = approval.workspace_id == job.workspace_id
                && approval.task_id == task_id
                && approval.run_id == run_id
                && approval.tool_call_id == tool_call_id
                && approval.device_id == job.device_id
                && approval.tool == job.tool
                && approval.args_json == job.args_json;
            if !matches {
                return Err(DeviceJobError::ExecutionConflict);
            }
            let task = crate::runtime_task::find_task(&mut connection, &job.workspace_id, task_id)
                .await
                .map_err(crate::device_job::storage)?
                .ok_or(DeviceJobError::NotFound)?;
            if task.status != TaskStatus::NeedsApproval {
                return Ok((None, false, Vec::new()));
            }
            let now = Utc::now();
            sqlx::query(
                "INSERT INTO device_jobs
                    (id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                     progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
                 VALUES (?, ?, ?, ?, ?, 'accepted', NULL, NULL, NULL, ?, ?, ?, NULL, ?, ?, NULL)",
            )
            .bind(&job.id)
            .bind(&job.workspace_id)
            .bind(&job.device_id)
            .bind(&job.tool)
            .bind(serde_json::to_string(&job.args_json).map_err(crate::device_job::storage)?)
            .bind(task_id)
            .bind(run_id)
            .bind(tool_call_id)
            .bind(i64::try_from(job.timeout_ms).map_err(|_| DeviceJobError::Validation("timeout is too large".into()))?)
            .bind(job.started_at)
            .execute(&mut connection)
            .await
            .map_err(crate::device_job::storage)?;
            let changed = sqlx::query(
                "UPDATE agent_tasks SET status = 'waiting_job', state_version = state_version + 1, updated_at = ?
                 WHERE workspace_id = ? AND id = ? AND status = 'needs_approval' AND state_version = ?",
            )
            .bind(now)
            .bind(&job.workspace_id)
            .bind(task_id)
            .bind(task.state_version)
            .execute(&mut connection)
            .await
            .map_err(crate::device_job::storage)?
            .rows_affected();
            if changed != 1 {
                return Err(DeviceJobError::ExecutionConflict);
            }
            let task = crate::runtime_task::find_task(&mut connection, &job.workspace_id, task_id)
                .await
                .map_err(crate::device_job::storage)?
                .ok_or(DeviceJobError::NotFound)?;
            let job_event = append_runtime_event_tx(
                &mut connection,
                crate::runtime_event::job_event(job, "accepted", json!({"state":"accepted"})),
            )
            .await
            .map_err(crate::device_job::storage)?;
            let task_event = append_runtime_event_tx(
                &mut connection,
                task_event(
                    &task,
                    "task.waiting_job",
                    format!("task:{}:v{}:waiting_job", task.id, task.state_version),
                ),
            )
            .await
            .map_err(crate::device_job::storage)?;
            Ok((Some(job.clone()), true, vec![job_event, task_event]))
        }
        .await;
        match result {
            Ok(value) => {
                sqlx::query("COMMIT")
                    .execute(&mut connection)
                    .await
                    .map_err(crate::device_job::storage)?;
                Ok(value)
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
                Err(error)
            }
        }
    }

    pub async fn cancel_task_approvals_and_events(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(AgentTask, Vec<RuntimeEvent>), TaskApprovalStoreError> {
        const MAX_LOCK_RETRIES: u32 = 8;
        let mut retries = 0;
        loop {
            let connection = match begin_immediate(self.pool()).await {
                Ok(connection) => connection,
                Err(error) if retries < MAX_LOCK_RETRIES && is_transient_lock(&error) => {
                    retries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(5 * u64::from(retries))).await;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let result = self
                .cancel_task_approvals_and_events_once(connection, workspace_id, task_id, expected_version, now)
                .await;
            match result {
                Err(error) if retries < MAX_LOCK_RETRIES && is_transient_lock(&error) => {
                    retries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(5 * u64::from(retries))).await;
                }
                other => return other,
            }
        }
    }

    async fn cancel_task_approvals_and_events_once(
        &self,
        mut connection: SqliteConnection,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(AgentTask, Vec<RuntimeEvent>), TaskApprovalStoreError> {
        let result = async {
            let task = crate::runtime_task::find_task(&mut connection, workspace_id, task_id)
                .await
                .map_err(storage)?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            #[cfg(test)]
            pause_cancel_for_lock_test().await;
            if task.state_version != expected_version || !task.status.can_transition_to(TaskStatus::Cancelling) {
                return Err(TaskApprovalStoreError::Storage("task changed concurrently".into()));
            }
            let changed = sqlx::query(
                "UPDATE agent_tasks SET status = 'cancelling', state_version = state_version + 1, updated_at = ?
                 WHERE workspace_id = ? AND id = ? AND state_version = ?",
            )
            .bind(now)
            .bind(workspace_id)
            .bind(task_id)
            .bind(expected_version)
            .execute(&mut connection)
            .await
            .map_err(storage)?
            .rows_affected();
            if changed != 1 {
                return Err(TaskApprovalStoreError::Storage("task changed concurrently".into()));
            }
            let pending = sqlx::query_as::<_, TaskApprovalRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {APPROVAL_COLUMNS} FROM task_approvals WHERE workspace_id = ? AND task_id = ? AND state = 'pending' ORDER BY id"
            )))
            .bind(workspace_id)
            .bind(task_id)
            .fetch_all(&mut connection)
            .await
            .map_err(storage)?
            .into_iter()
            .map(TaskApproval::try_from)
            .collect::<Result<Vec<_>, _>>()?;
            let mut events = Vec::new();
            for approval in pending {
                sqlx::query(
                    "UPDATE task_approvals SET state = 'unavailable', decided_at = ?, updated_at = ? WHERE id = ? AND state = 'pending'",
                )
                .bind(now)
                .bind(now)
                .bind(&approval.id)
                .execute(&mut connection)
                .await
                .map_err(storage)?;
                insert_decision_wake(&mut connection, &decision_wake(&approval, now)).await?;
                let unavailable = find_scoped(&mut connection, workspace_id, task_id, &approval.id)
                    .await?
                    .ok_or(TaskApprovalStoreError::NotFound)?;
                events.push(
                    append_runtime_event_tx(&mut connection, approval_event(&unavailable, "unavailable"))
                        .await
                        .map_err(storage)?,
                );
            }
            sqlx::query(
                "UPDATE task_wake_outbox SET resolved_at = ? WHERE task_id = ? AND resolved_at IS NULL",
            )
            .bind(now)
            .bind(task_id)
            .execute(&mut connection)
            .await
            .map_err(storage)?;
            let task = crate::runtime_task::find_task(&mut connection, workspace_id, task_id)
                .await
                .map_err(storage)?
                .ok_or(TaskApprovalStoreError::NotFound)?;
            events.push(
                append_runtime_event_tx(
                    &mut connection,
                    task_event(&task, "task.cancelling", format!("task:{}:v{}:cancelling", task.id, task.state_version)),
                )
                .await
                .map_err(storage)?,
            );
            Ok((task, events))
        }
        .await;
        let outcome = finish_immediate(connection, result).await;
        if let Ok((_, events)) = &outcome {
            let unavailable = events
                .iter()
                .filter(|event| event.event_type == "approval.unavailable")
                .count();
            if unavailable > 0 {
                tracing::info!(
                    metric = "runtime_approval_unavailable",
                    workspace_id = %workspace_id,
                    task_id = %task_id,
                    count = unavailable,
                    "cancel marked approvals unavailable"
                );
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::{Value, json};
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::task_approval::{ApprovalDecision, ApprovalState, TaskApproval, TaskApprovalStoreError};

    use crate::Db;

    async fn seed_workspace(pool: &SqlitePool, workspace_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(pool)
        .await
        .expect("seed plan");
        sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(pool)
            .await
            .expect("seed tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 't1', 'now', 'now')",
        )
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    }

    struct Fixture {
        db: Db,
        next: usize,
        path: std::path::PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
            let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        }
    }

    impl Fixture {
        async fn new() -> Self {
            let path = std::env::temp_dir().join(format!("tinyiothub-approval-{}.db", uuid::Uuid::new_v4()));
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
            Self::with_pool(path, options).await
        }

        async fn new_in_memory() -> Self {
            let pool = crate::test_helpers::test_pool().await;
            seed_workspace(&pool, "ws_1").await;
            seed_workspace(&pool, "ws_other").await;
            Self {
                db: Db::new(pool),
                next: 0,
                path: std::path::PathBuf::new(),
            }
        }

        async fn with_pool(path: std::path::PathBuf, options: sqlx::sqlite::SqliteConnectOptions) -> Self {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(options)
                .await
                .expect("approval pool");
            crate::test_helpers::run_all_migrations(&pool)
                .await
                .expect("migrations");
            seed_workspace(&pool, "ws_1").await;
            seed_workspace(&pool, "ws_other").await;
            Self {
                db: Db::new(pool),
                next: 0,
                path,
            }
        }

        async fn pending_approval(&mut self) -> TaskApproval {
            self.next += 1;
            let id = format!("task_{}", self.next);
            let now = Utc::now();
            let task = AgentTask {
                id: id.clone(),
                workspace_id: "ws_1".into(),
                idempotency_key: format!("key_{id}"),
                objective: "exercise approvals".into(),
                session_key: None,
                created_by: "user_1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            };
            let create_wake = TaskWake {
                task_id: id.clone(),
                event_id: format!("create_{id}"),
                signal_json: json!({"workspace_id":"ws_1"}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            };
            self.db.create_task_with_wake(&task, &create_wake).await.expect("task");
            let approval = TaskApproval {
                id: format!("approval_{id}"),
                workspace_id: "ws_1".into(),
                task_id: id.clone(),
                run_id: format!("run_{id}"),
                tool_call_id: format!("call_{id}"),
                device_id: "device_1".into(),
                tool: "set_speed".into(),
                args_json: json!({"speed": 7, "nested": {"frozen": true}}),
                reason: "human confirmation required".into(),
                state: ApprovalState::Pending,
                decision: None,
                decider_id: None,
                requested_at: now,
                decided_at: None,
                expires_at: now + Duration::hours(1),
                decision_event_id: format!("decision_{id}"),
                updated_at: now,
            };
            let task = self
                .db
                .create_task_approval_and_events(&approval, 0)
                .await
                .expect("pending approval")
                .0;
            assert_eq!(task.status, TaskStatus::NeedsApproval);
            approval
        }

        fn wake(&self, approval: &TaskApproval) -> TaskWake {
            TaskWake {
                task_id: approval.task_id.clone(),
                event_id: approval.decision_event_id.clone(),
                signal_json: json!({
                    "kind":"approval_decided",
                    "workspace_id":approval.workspace_id,
                    "task_id":approval.task_id,
                    "approval_id":approval.id
                })
                .to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: Utc::now(),
                dispatched_at: None,
                resolved_at: None,
            }
        }

        async fn decide(
            &self,
            approval: &TaskApproval,
            decision: ApprovalDecision,
        ) -> Result<tinyiothub_core::task_approval::DecisionOutcome, TaskApprovalStoreError> {
            self.db
                .decide_task_approval_and_event(
                    &approval.workspace_id,
                    &approval.task_id,
                    &approval.id,
                    decision,
                    "admin_1",
                    Utc::now(),
                    &self.wake(approval),
                )
                .await
                .map(|(outcome, _)| outcome)
        }

        async fn get(&self, approval: &TaskApproval) -> TaskApproval {
            self.db
                .task_approval_scoped(&approval.workspace_id, &approval.task_id, &approval.id)
                .await
                .expect("get")
                .expect("approval")
        }

        async fn outbox_count(&self, approval: &TaskApproval) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ? AND event_id = ?")
                .bind(&approval.task_id)
                .bind(&approval.decision_event_id)
                .fetch_one(self.db.pool())
                .await
                .expect("outbox count")
        }

        fn job(&self, approval: &TaskApproval) -> tinyiothub_core::device_job::DeviceJob {
            tinyiothub_core::device_job::DeviceJob {
                id: format!("job_{}", approval.task_id),
                workspace_id: approval.workspace_id.clone(),
                device_id: approval.device_id.clone(),
                tool: approval.tool.clone(),
                args_json: approval.args_json.clone(),
                state: tinyiothub_core::device_job::DeviceJobState::Accepted,
                stop_requested_at: None,
                progress_json: None,
                result_json: None,
                task_id: Some(approval.task_id.clone()),
                run_id: Some(approval.run_id.clone()),
                tool_call_id: Some(approval.tool_call_id.clone()),
                driver_call_id: None,
                timeout_ms: 5_000,
                started_at: Utc::now(),
                ended_at: None,
            }
        }
    }

    #[tokio::test]
    async fn concurrent_same_decision_creates_one_outbox_and_preserves_arguments() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        let frozen = pending.args_json.clone();
        let db_a = fixture.db.clone();
        let db_b = fixture.db.clone();
        let wake_a = fixture.wake(&pending);
        let wake_b = fixture.wake(&pending);
        let pending_a = pending.clone();
        let pending_b = pending.clone();
        let (a, b) = tokio::join!(
            async move {
                db_a.decide_task_approval_and_event(
                    "ws_1",
                    &pending_a.task_id,
                    &pending_a.id,
                    ApprovalDecision::Approve,
                    "admin_1",
                    Utc::now(),
                    &wake_a,
                )
                .await
            },
            async move {
                db_b.decide_task_approval_and_event(
                    "ws_1",
                    &pending_b.task_id,
                    &pending_b.id,
                    ApprovalDecision::Approve,
                    "admin_1",
                    Utc::now(),
                    &wake_b,
                )
                .await
            },
        );
        let a = a.expect("first").0;
        let b = b.expect("replay").0;
        assert_eq!(a.approval.decision_event_id, b.approval.decision_event_id);
        assert_eq!(fixture.outbox_count(&pending).await, 1);
        assert_eq!(fixture.get(&pending).await.args_json, frozen);
        assert!(matches!(
            fixture.decide(&pending, ApprovalDecision::Reject).await,
            Err(TaskApprovalStoreError::DecisionConflict)
        ));
    }

    #[tokio::test]
    async fn reject_creates_decision_fact_and_transaction_failure_rolls_back() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        let rejected = fixture
            .decide(&pending, ApprovalDecision::Reject)
            .await
            .expect("reject");
        assert_eq!(rejected.approval.state, ApprovalState::Rejected);
        assert_eq!(fixture.outbox_count(&pending).await, 1);
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("jobs");
        assert_eq!(jobs, 0);

        let other = fixture.pending_approval().await;
        let mut invalid_wake = fixture.wake(&other);
        invalid_wake.event_id = "client_controlled_event".into();
        let result = fixture
            .db
            .decide_task_approval_and_event(
                "ws_1",
                &other.task_id,
                &other.id,
                ApprovalDecision::Approve,
                "admin_1",
                Utc::now(),
                &invalid_wake,
            )
            .await;
        assert!(result.is_err());
        assert_eq!(fixture.get(&other).await.state, ApprovalState::Pending);
        assert_eq!(fixture.outbox_count(&other).await, 0);
    }

    #[tokio::test]
    async fn due_pending_and_paused_decisions_expire_with_original_wake() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        let task = fixture
            .db
            .get_task("ws_1", &pending.task_id)
            .await
            .expect("task")
            .expect("present");
        fixture
            .db
            .transition_task("ws_1", &pending.task_id, task.state_version, TaskStatus::Paused, None)
            .await
            .expect("pause");
        let (expired, events) = fixture
            .db
            .expire_task_approvals_and_events(Utc::now() + Duration::hours(2))
            .await
            .expect("expiry");
        assert_eq!(expired, 1);
        assert_eq!(events.len(), 1);
        let current = fixture.get(&pending).await;
        assert_eq!(current.state, ApprovalState::Expired);
        assert_eq!(current.decision_event_id, pending.decision_event_id);
        assert_eq!(fixture.outbox_count(&pending).await, 1);
        let replay = fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("current expired");
        assert_eq!(replay.approval.state, ApprovalState::Expired);
        assert_eq!(fixture.outbox_count(&pending).await, 1);
        let (again, _) = fixture
            .db
            .expire_task_approvals_and_events(Utc::now() + Duration::hours(3))
            .await
            .expect("repeat expiry");
        assert_eq!(again, 0);
    }

    #[tokio::test]
    async fn approved_job_and_task_transition_are_atomic_and_idempotent() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("approve");
        let job = fixture.job(&pending);
        let (created, inserted, events) = fixture
            .db
            .submit_approved_job_and_events(&job)
            .await
            .expect("approved job");
        let created = created.expect("created job");
        assert!(inserted);
        assert_eq!(created.args_json, pending.args_json);
        assert_eq!(events.len(), 2);
        let task = fixture
            .db
            .get_task("ws_1", &pending.task_id)
            .await
            .expect("task")
            .expect("present");
        assert_eq!(task.status, TaskStatus::WaitingJob);

        let (existing, inserted, events) = fixture.db.submit_approved_job_and_events(&job).await.expect("replay");
        assert!(!inserted);
        assert!(events.is_empty());
        assert_eq!(existing.expect("existing").id, job.id);
        let mut conflict = job.clone();
        conflict.args_json = json!({"speed": 999});
        assert!(matches!(
            fixture.db.submit_approved_job_and_events(&conflict).await,
            Err(tinyiothub_core::device_job::DeviceJobError::ExecutionConflict)
        ));
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(jobs, 1);
    }

    #[tokio::test]
    async fn continuation_run_approval_creates_exactly_one_job() {
        let fixture = Fixture::new().await;
        let now = Utc::now();
        let task = AgentTask {
            id: "task_cont".into(),
            workspace_id: "ws_1".into(),
            idempotency_key: "key_cont".into(),
            objective: "continue after the first job".into(),
            session_key: None,
            created_by: "user_1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let create_wake = TaskWake {
            task_id: task.id.clone(),
            event_id: "create_task_cont".into(),
            signal_json: json!({"workspace_id": "ws_1"}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        fixture
            .db
            .create_task_with_wake(&task, &create_wake)
            .await
            .expect("task");

        // The Task is waiting on a previous Job when the continuation wake arrives.
        let waiting = fixture
            .db
            .transition_task("ws_1", &task.id, 0, TaskStatus::WaitingJob, None)
            .await
            .expect("waiting job");
        assert_eq!(waiting.state_version, 1);
        let continuation = TaskWake {
            task_id: task.id.clone(),
            event_id: "event_continue".into(),
            signal_json: json!({"workspace_id": "ws_1"}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: now,
            dispatched_at: None,
            resolved_at: None,
        };
        fixture.db.record_task_wake(&continuation).await.expect("record wake");
        assert!(
            fixture
                .db
                .begin_run(&waiting, &continuation.event_id, "run_cont")
                .await
                .expect("continuation run")
        );
        let running = fixture
            .db
            .get_task("ws_1", &task.id)
            .await
            .expect("task")
            .expect("present");
        assert_eq!(
            running.status,
            TaskStatus::Active,
            "a continuation Run must be observable as active"
        );
        assert_eq!(running.current_run_id.as_deref(), Some("run_cont"));
        let dispatched = fixture
            .db
            .wake_by_id(&task.id, &continuation.event_id)
            .await
            .expect("wake")
            .expect("wake present");
        assert_eq!(dispatched.dispatch_state, WakeDispatchState::Dispatched);
        assert!(
            !fixture
                .db
                .begin_run(&running, &continuation.event_id, "run_replay")
                .await
                .expect("replay")
        );

        let approval = TaskApproval {
            id: "approval_cont".into(),
            workspace_id: "ws_1".into(),
            task_id: task.id.clone(),
            run_id: "run_cont".into(),
            tool_call_id: "call_cont".into(),
            device_id: "device_1".into(),
            tool: "set_speed".into(),
            args_json: json!({"speed": 7}),
            reason: "confirm".into(),
            state: ApprovalState::Pending,
            decision: None,
            decider_id: None,
            requested_at: now,
            decided_at: None,
            expires_at: now + Duration::hours(1),
            decision_event_id: "decision_cont".into(),
            updated_at: now,
        };
        let (needs_approval, _) = fixture
            .db
            .create_task_approval_and_events(&approval, running.state_version)
            .await
            .expect("approval requested by continuation run");
        assert_eq!(needs_approval.status, TaskStatus::NeedsApproval);

        fixture
            .decide(&approval, ApprovalDecision::Approve)
            .await
            .expect("approve");
        let job = tinyiothub_core::device_job::DeviceJob {
            id: "job_cont".into(),
            workspace_id: "ws_1".into(),
            device_id: approval.device_id.clone(),
            tool: approval.tool.clone(),
            args_json: approval.args_json.clone(),
            state: tinyiothub_core::device_job::DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(task.id.clone()),
            run_id: Some("run_cont".into()),
            tool_call_id: Some(approval.tool_call_id.clone()),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: now,
            ended_at: None,
        };
        let (created, inserted, _) = fixture
            .db
            .submit_approved_job_and_events(&job)
            .await
            .expect("approved job");
        assert!(inserted);
        assert_eq!(created.expect("job").tool_call_id.as_deref(), Some("call_cont"));
        let (existing, inserted_again, _) = fixture
            .db
            .submit_approved_job_and_events(&job)
            .await
            .expect("idempotent replay");
        assert!(!inserted_again);
        assert!(existing.is_some());
        let jobs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_jobs WHERE workspace_id = 'ws_1' AND tool_call_id = 'call_cont'",
        )
        .fetch_one(fixture.db.pool())
        .await
        .expect("job count");
        assert_eq!(jobs, 1, "one approval decision must yield exactly one Job");
        let settled = fixture
            .db
            .get_task("ws_1", &task.id)
            .await
            .expect("task")
            .expect("present");
        assert_eq!(settled.status, TaskStatus::WaitingJob);
    }

    #[tokio::test]
    async fn approved_job_requires_current_approval_state_and_frozen_identity() {
        for state in [
            ApprovalState::Rejected,
            ApprovalState::Expired,
            ApprovalState::Unavailable,
        ] {
            let mut fixture = Fixture::new().await;
            let pending = fixture.pending_approval().await;
            match state {
                ApprovalState::Rejected => {
                    fixture
                        .decide(&pending, ApprovalDecision::Reject)
                        .await
                        .expect("reject");
                }
                ApprovalState::Expired => {
                    fixture
                        .db
                        .expire_task_approvals_and_events(Utc::now() + Duration::hours(2))
                        .await
                        .expect("expire");
                }
                ApprovalState::Unavailable => {
                    sqlx::query("UPDATE task_approvals SET state = 'unavailable' WHERE id = ?")
                        .bind(&pending.id)
                        .execute(fixture.db.pool())
                        .await
                        .expect("make unavailable");
                }
                ApprovalState::Pending | ApprovalState::Approved => unreachable!("test states are decided"),
            }
            let (job, inserted, events) = fixture
                .db
                .submit_approved_job_and_events(&fixture.job(&pending))
                .await
                .expect("non-approved submission");
            assert!(job.is_none(), "{state:?} inserted a Job");
            assert!(!inserted);
            assert!(events.is_empty());
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
                .bind(&pending.tool_call_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("job count");
            assert_eq!(count, 0, "{state:?} inserted a Job");
        }

        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("approve");
        let mut mismatched = fixture.job(&pending);
        mismatched.args_json = json!({"speed": 999});
        assert!(matches!(
            fixture.db.submit_approved_job_and_events(&mismatched).await,
            Err(tinyiothub_core::device_job::DeviceJobError::ExecutionConflict)
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn paused_or_cancelled_approval_does_not_insert_a_job() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        let task = fixture
            .db
            .get_task("ws_1", &pending.task_id)
            .await
            .expect("task")
            .expect("present");
        fixture
            .db
            .transition_task("ws_1", &pending.task_id, task.state_version, TaskStatus::Paused, None)
            .await
            .expect("pause");
        fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("approve while paused");
        let (job, _, _) = fixture
            .db
            .submit_approved_job_and_events(&fixture.job(&pending))
            .await
            .expect("paused submission");
        assert!(job.is_none());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(count, 0);

        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("approve before cancel");
        fixture
            .db
            .cancel_task_approvals_and_events("ws_1", &pending.task_id, 1, Utc::now())
            .await
            .expect("cancel decided approval");
        let (job, _, _) = fixture
            .db
            .submit_approved_job_and_events(&fixture.job(&pending))
            .await
            .expect("cancelled submission");
        assert!(job.is_none());
        let resolved: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM task_wake_outbox WHERE task_id = ? AND event_id = ?")
                .bind(&pending.task_id)
                .bind(&pending.decision_event_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("cancelled decision wake");
        assert!(resolved.is_some());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn claimed_approved_wake_cancel_race_stays_resolved_without_a_job() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("approve");
        let claimed = fixture.db.claim_pending_wakes(10).await.expect("claim decision");
        assert!(claimed.iter().any(|wake| wake.event_id == pending.decision_event_id));
        fixture
            .db
            .cancel_task_approvals_and_events("ws_1", &pending.task_id, 1, Utc::now())
            .await
            .expect("cancel claimed decision");
        let (job, _, _) = fixture
            .db
            .submit_approved_job_and_events(&fixture.job(&pending))
            .await
            .expect("cancelled submission");
        assert!(job.is_none());

        fixture
            .db
            .mark_wake(&pending.task_id, &pending.decision_event_id, WakeDispatchState::Pending)
            .await
            .expect("deferred dispatch restoration");
        let (resolved, dispatched): (Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
            "SELECT resolved_at, dispatched_at FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
        )
        .bind(&pending.task_id)
        .bind(&pending.decision_event_id)
        .fetch_one(fixture.db.pool())
        .await
        .expect("decision wake");
        assert!(
            resolved.is_some(),
            "deferred restoration resurrected the cancelled wake"
        );
        assert!(dispatched.is_some(), "the original claim should remain recorded");
        assert!(
            fixture
                .db
                .claim_pending_wakes(10)
                .await
                .expect("claim after cancellation")
                .iter()
                .all(|wake| wake.event_id != pending.decision_event_id)
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE tool_call_id = ?")
            .bind(&pending.tool_call_id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("job count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn cancel_retries_a_transient_task_update_deadlock_atomically() {
        let mut fixture = Fixture::new_in_memory().await;
        let pending = fixture.pending_approval().await;
        let mut writer = fixture.db.pool().begin().await.expect("writer transaction");
        sqlx::query("SELECT id FROM agent_tasks WHERE id = ?")
            .bind(&pending.task_id)
            .fetch_optional(&mut *writer)
            .await
            .expect("hold Task read lock");

        let paused = std::sync::Arc::new(tokio::sync::Notify::new());
        let resume = std::sync::Arc::new(tokio::sync::Notify::new());
        let attempted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel = super::CANCEL_LOCK_TEST_PAUSE.scope(
            (paused.clone(), resume.clone(), attempted),
            fixture
                .db
                .cancel_task_approvals_and_events("ws_1", &pending.task_id, 1, Utc::now()),
        );
        let deadlock = async {
            paused.notified().await;
            let mut upgrade = Box::pin(
                sqlx::query("UPDATE agent_tasks SET updated_at = updated_at WHERE id = ?")
                    .bind(&pending.task_id)
                    .execute(&mut *writer),
            );
            tokio::select! {
                result = &mut upgrade => panic!("reader upgrade should wait for cancellation: {result:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
            resume.notify_one();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(upgrade);
            writer.rollback().await.expect("release writer lock");
        };
        let (result, ()) = tokio::join!(cancel, deadlock);
        let (task, events) = result.expect("cancellation survives the transient lock deadlock");
        assert_eq!(task.status, TaskStatus::Cancelling);
        assert_eq!(events.len(), 2);
        assert_eq!(fixture.get(&pending).await.state, ApprovalState::Unavailable);
        let resolved: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM task_wake_outbox WHERE task_id = ? AND event_id = ?")
                .bind(&pending.task_id)
                .bind(&pending.decision_event_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("decision wake");
        assert!(resolved.is_some());
    }

    #[tokio::test]
    async fn cancel_marks_pending_unavailable_and_invalidates_decision_wake_atomically() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        let (task, events) = fixture
            .db
            .cancel_task_approvals_and_events("ws_1", &pending.task_id, 1, Utc::now())
            .await
            .expect("cancel");
        assert_eq!(task.status, TaskStatus::Cancelling);
        assert!(!events.is_empty());
        let current = fixture.get(&pending).await;
        assert_eq!(current.state, ApprovalState::Unavailable);
        let resolved: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM task_wake_outbox WHERE task_id = ? AND event_id = ?")
                .bind(&pending.task_id)
                .bind(&pending.decision_event_id)
                .fetch_one(fixture.db.pool())
                .await
                .expect("decision wake");
        assert!(resolved.is_some());
        let replay = fixture
            .decide(&pending, ApprovalDecision::Approve)
            .await
            .expect("current");
        assert_eq!(replay.approval.state, ApprovalState::Unavailable);
        assert_eq!(fixture.outbox_count(&pending).await, 1);
    }

    #[tokio::test]
    async fn scoped_lookup_and_migration_foreign_keys_are_enforced() {
        let mut fixture = Fixture::new().await;
        let pending = fixture.pending_approval().await;
        assert!(
            fixture
                .db
                .task_approval_scoped("ws_other", &pending.task_id, &pending.id)
                .await
                .expect("foreign scope")
                .is_none()
        );
        let violations: Vec<(String, i64, String, i64)> =
            sqlx::query_as("SELECT \"table\", rowid, \"parent\", fkid FROM pragma_foreign_key_check")
                .fetch_all(fixture.db.pool())
                .await
                .expect("foreign key check");
        assert!(violations.is_empty(), "foreign key violations: {violations:?}");
        let frozen: String = sqlx::query_scalar("SELECT args_json FROM task_approvals WHERE id = ?")
            .bind(&pending.id)
            .fetch_one(fixture.db.pool())
            .await
            .expect("stored args");
        assert_eq!(
            serde_json::from_str::<Value>(&frozen).expect("args JSON"),
            pending.args_json
        );
    }
}
