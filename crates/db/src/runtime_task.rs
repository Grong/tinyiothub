use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::Digest;
use sqlx::{Sqlite, SqliteConnection, Transaction};
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake,
};
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

use crate::Db;
use crate::error::{DbError, Result as DbResult};
use crate::runtime_event::{RuntimeEventInput, append_runtime_event_tx, task_event};

#[derive(Debug, sqlx::FromRow)]
struct FinalizeIntentRow {
    task_id: String,
    workspace_id: String,
    wake_event_id: String,
    run_id: String,
    report_json: String,
    report_persisted: i64,
    report_emitted: i64,
    undelivered_json: String,
}

impl FinalizeIntentRow {
    fn into_intent(self) -> Result<TaskFinalizeIntent, TaskStoreError> {
        Ok(TaskFinalizeIntent {
            task_id: self.task_id,
            workspace_id: self.workspace_id,
            wake_event_id: self.wake_event_id,
            run_id: self.run_id,
            report: serde_json::from_str(&self.report_json).map_err(unavailable)?,
            report_persisted: self.report_persisted != 0,
            report_emitted: self.report_emitted != 0,
            undelivered: serde_json::from_str(&self.undelivered_json).map_err(unavailable)?,
        })
    }
}

/// Read-only projection used by the Harness overview and Subagent page.
#[derive(Debug, Clone)]
pub struct HarnessTaskRow {
    pub id: String,
    pub run_id: Option<String>,
    pub state: String,
    pub objective: Option<String>,
    /// Latest `task.outcome` payload, if any; parsed by the HTTP projection.
    pub outcome_json: Option<String>,
    pub updated_at: Option<String>,
    pub parent_task_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct HarnessTaskDbRow {
    id: String,
    run_id: Option<String>,
    state: String,
    objective: Option<String>,
    outcome_json: Option<String>,
    updated_at: Option<String>,
    parent_task_id: Option<String>,
}

impl From<HarnessTaskDbRow> for HarnessTaskRow {
    fn from(row: HarnessTaskDbRow) -> Self {
        Self {
            id: row.id,
            run_id: row.run_id,
            state: row.state,
            objective: row.objective,
            outcome_json: row.outcome_json,
            updated_at: row.updated_at,
            parent_task_id: row.parent_task_id,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AgentTaskRow {
    id: String,
    workspace_id: String,
    idempotency_key: String,
    objective: String,
    session_key: Option<String>,
    created_by: String,
    status: String,
    current_run_id: Option<String>,
    state_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl TryFrom<AgentTaskRow> for AgentTask {
    type Error = TaskStoreError;

    fn try_from(row: AgentTaskRow) -> Result<Self, Self::Error> {
        let status = tinyiothub_core::agent_task::TaskStatus::from_db(&row.status)
            .ok_or_else(|| TaskStoreError::Unavailable(format!("unknown task status: {}", row.status)))?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            idempotency_key: row.idempotency_key,
            objective: row.objective,
            session_key: row.session_key,
            created_by: row.created_by,
            status,
            current_run_id: row.current_run_id,
            state_version: row.state_version,
            created_at: row.created_at,
            updated_at: row.updated_at,
            ended_at: row.ended_at,
        })
    }
}

fn unavailable(error: impl std::fmt::Display) -> TaskStoreError {
    TaskStoreError::Unavailable(error.to_string())
}

async fn upsert_finalize_intent_tx(
    tx: &mut Transaction<'_, Sqlite>,
    intent: &TaskFinalizeIntent,
    now: DateTime<Utc>,
) -> Result<(), TaskStoreError> {
    let report_json = serde_json::to_string(&intent.report).map_err(unavailable)?;
    let undelivered_json = serde_json::to_string(&intent.undelivered).map_err(unavailable)?;
    sqlx::query(
        "INSERT INTO task_finalize_intents
             (task_id, workspace_id, wake_event_id, run_id, report_json,
              report_persisted, report_emitted, undelivered_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(task_id, wake_event_id) DO UPDATE SET
             workspace_id = excluded.workspace_id,
             run_id = excluded.run_id,
             report_json = excluded.report_json,
             report_persisted = excluded.report_persisted,
             report_emitted = excluded.report_emitted,
             undelivered_json = excluded.undelivered_json,
             updated_at = excluded.updated_at",
    )
    .bind(&intent.task_id)
    .bind(&intent.workspace_id)
    .bind(&intent.wake_event_id)
    .bind(&intent.run_id)
    .bind(report_json)
    .bind(i64::from(intent.report_persisted))
    .bind(i64::from(intent.report_emitted))
    .bind(undelivered_json)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(unavailable)?;
    Ok(())
}

/// Statuses that may legally transition to `next`. The first write of a
/// transition transaction carries this list so SQLite acquires the write lock
/// before any read (avoiding a deferred SHARED -> RESERVED upgrade failure).
fn status_predecessors(next: TaskStatus) -> Vec<&'static str> {
    [
        TaskStatus::Active,
        TaskStatus::WaitingJob,
        TaskStatus::WaitingWatch,
        TaskStatus::NeedsApproval,
        TaskStatus::Paused,
        TaskStatus::Cancelling,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ]
    .into_iter()
    .filter(|status| status.can_transition_to(next))
    .map(TaskStatus::as_str)
    .collect()
}

pub(crate) async fn find_task(
    executor: &mut sqlx::SqliteConnection,
    workspace_id: &str,
    task_id: &str,
) -> Result<Option<AgentTask>, TaskStoreError> {
    sqlx::query_as::<_, AgentTaskRow>(
        "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                status, current_run_id, state_version, created_at, updated_at, ended_at
         FROM agent_tasks WHERE workspace_id = ? AND id = ?",
    )
    .bind(workspace_id)
    .bind(task_id)
    .fetch_optional(executor)
    .await
    .map_err(unavailable)?
    .map(AgentTask::try_from)
    .transpose()
}

async fn insert_wake(tx: &mut Transaction<'_, Sqlite>, wake: &TaskWake) -> Result<(), TaskStoreError> {
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
    .execute(&mut **tx)
    .await
    .map_err(unavailable)?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct TaskWakeRow {
    task_id: String,
    event_id: String,
    signal_json: String,
    dispatch_state: String,
    created_at: DateTime<Utc>,
    dispatched_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
}

impl TryFrom<TaskWakeRow> for TaskWake {
    type Error = TaskStoreError;

    fn try_from(row: TaskWakeRow) -> Result<Self, Self::Error> {
        let dispatch_state =
            tinyiothub_core::agent_task::WakeDispatchState::from_db(&row.dispatch_state).ok_or_else(|| {
                TaskStoreError::Unavailable(format!("unknown wake dispatch state: {}", row.dispatch_state))
            })?;
        Ok(Self {
            task_id: row.task_id,
            event_id: row.event_id,
            signal_json: row.signal_json,
            dispatch_state,
            created_at: row.created_at,
            dispatched_at: row.dispatched_at,
            resolved_at: row.resolved_at,
        })
    }
}

impl Db {
    pub async fn create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError> {
        self.create_task_with_wake_and_event(task, wake)
            .await
            .map(|(outcome, _)| outcome)
    }

    pub async fn create_task_with_wake_and_event(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<(CreateTaskOutcome, crate::runtime_event::RuntimeEvent), TaskStoreError> {
        let signal: serde_json::Value = serde_json::from_str(&wake.signal_json)
            .map_err(|error| TaskStoreError::Validation(format!("invalid wake signal: {error}")))?;
        let signal_workspace = signal
            .get("workspace_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| TaskStoreError::Validation("wake signal has no workspace_id".to_string()))?;
        if task.workspace_id != signal_workspace {
            return Err(TaskStoreError::Validation("task and wake workspace differ".to_string()));
        }

        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let inserted = sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, session_key, created_by,
                  status, current_run_id, state_version, created_at, updated_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(workspace_id, idempotency_key) DO NOTHING",
        )
        .bind(&task.id)
        .bind(&task.workspace_id)
        .bind(&task.idempotency_key)
        .bind(&task.objective)
        .bind(&task.session_key)
        .bind(&task.created_by)
        .bind(task.status.as_str())
        .bind(&task.current_run_id)
        .bind(task.state_version)
        .bind(task.created_at)
        .bind(task.updated_at)
        .bind(task.ended_at)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();

        if inserted == 0 {
            let original = find_task_by_key(&mut tx, &task.workspace_id, &task.idempotency_key)
                .await?
                .ok_or_else(|| TaskStoreError::Unavailable("idempotency conflict without original task".into()))?;
            if original.objective != task.objective {
                return Err(TaskStoreError::ObjectiveConflict);
            }
            let event = append_runtime_event_tx(
                &mut tx,
                task_event(&original, "task.created", format!("task:{}:created", original.id)),
            )
            .await
            .map_err(unavailable)?;
            tx.commit().await.map_err(unavailable)?;
            return Ok((
                CreateTaskOutcome {
                    task: original,
                    deduplicated: true,
                },
                event,
            ));
        }

        if wake.task_id != task.id {
            return Err(TaskStoreError::Validation("task and wake task id differ".to_string()));
        }
        insert_wake(&mut tx, wake).await?;
        let event = append_runtime_event_tx(
            &mut tx,
            task_event(task, "task.created", format!("task:{}:created", task.id)),
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok((
            CreateTaskOutcome {
                task: task.clone(),
                deduplicated: false,
            },
            event,
        ))
    }

    pub async fn get_task(&self, workspace_id: &str, task_id: &str) -> Result<Option<AgentTask>, TaskStoreError> {
        let mut connection = self.pool().acquire().await.map_err(unavailable)?;
        find_task(&mut connection, workspace_id, task_id).await
    }

    pub async fn list_tasks(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentTask>, TaskStoreError> {
        sqlx::query_as::<_, AgentTaskRow>(
            "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                    status, current_run_id, state_version, created_at, updated_at, ended_at
             FROM agent_tasks WHERE workspace_id = ?
             ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?",
        )
        .bind(workspace_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(AgentTask::try_from)
        .collect()
    }

    pub async fn wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError> {
        sqlx::query_as::<_, TaskWakeRow>(
            "SELECT task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at
             FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .map(TaskWake::try_from)
        .transpose()
    }

    pub async fn record_task_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError> {
        let signal: serde_json::Value = serde_json::from_str(&wake.signal_json)
            .map_err(|error| TaskStoreError::Validation(format!("invalid wake signal: {error}")))?;
        let workspace_id = signal
            .get("workspace_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| TaskStoreError::Validation("wake signal has no workspace_id".to_string()))?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             SELECT ?, ?, ?, ?, ?, ?, ?
             WHERE EXISTS (
                 SELECT 1 FROM agent_tasks WHERE id = ? AND workspace_id = ?
             )",
        )
        .bind(&wake.task_id)
        .bind(&wake.event_id)
        .bind(&wake.signal_json)
        .bind(wake.dispatch_state.as_str())
        .bind(wake.created_at)
        .bind(wake.dispatched_at)
        .bind(wake.resolved_at)
        .bind(&wake.task_id)
        .bind(workspace_id)
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        if inserted == 1 {
            return Ok(true);
        }
        let exists =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ? AND event_id = ?")
                .bind(&wake.task_id)
                .bind(&wake.event_id)
                .fetch_one(self.pool())
                .await
                .map_err(unavailable)?;
        if exists > 0 {
            Ok(false)
        } else {
            Err(TaskStoreError::Validation(
                "wake task or workspace does not match".to_string(),
            ))
        }
    }

    pub async fn resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError> {
        let affected =
            sqlx::query("UPDATE task_wake_outbox SET resolved_at = ? WHERE task_id = ? AND resolved_at IS NULL")
                .bind(Utc::now())
                .bind(task_id)
                .execute(self.pool())
                .await
                .map_err(unavailable)?
                .rows_affected();
        Ok(affected as usize)
    }

    pub async fn transition_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: tinyiothub_core::agent_task::TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<AgentTask, TaskStoreError> {
        self.transition_task_and_event(workspace_id, task_id, expected_version, next, wake)
            .await
            .map(|(task, _)| task)
    }

    pub async fn transition_task_and_event(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: tinyiothub_core::agent_task::TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<(AgentTask, crate::runtime_event::RuntimeEvent), TaskStoreError> {
        if let Some(wake) = wake {
            validate_wake(workspace_id, task_id, wake)?;
        }
        let now = Utc::now();
        let ended_at = next.is_terminal().then_some(now);
        let predecessors = status_predecessors(next);
        if predecessors.is_empty() {
            return Err(TaskStoreError::Validation(format!(
                "task state {} has no legal predecessor",
                next.as_str()
            )));
        }
        let placeholders = vec!["?"; predecessors.len()].join(", ");
        let statement = format!(
            "UPDATE agent_tasks
             SET status = ?, state_version = state_version + 1, updated_at = ?, ended_at = ?
             WHERE workspace_id = ? AND id = ? AND state_version = ?
               AND status IN ({placeholders})
             RETURNING id, workspace_id, idempotency_key, objective, session_key, created_by,
                       status, current_run_id, state_version, created_at, updated_at, ended_at"
        );
        let mut query = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(statement))
            .bind(next.as_str())
            .bind(now)
            .bind(ended_at)
            .bind(workspace_id)
            .bind(task_id)
            .bind(expected_version);
        for predecessor in &predecessors {
            query = query.bind(*predecessor);
        }
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let updated = query.fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let Some(updated) = updated else {
            let current = find_task(&mut tx, workspace_id, task_id).await?;
            return match current {
                None => Err(TaskStoreError::ConcurrentUpdate),
                Some(current) if current.state_version != expected_version => Err(TaskStoreError::ConcurrentUpdate),
                Some(current) => Err(TaskStoreError::Validation(format!(
                    "task state transition {} -> {} is forbidden",
                    current.status.as_str(),
                    next.as_str()
                ))),
            };
        };
        if let Some(wake) = wake {
            insert_wake(&mut tx, wake).await?;
        }
        let task = AgentTask::try_from(updated)?;
        if task.status.is_terminal() {
            // Device-state projections are scoped to a live Task. Remove them
            // in the same transaction so a terminal Task leaves no orphans.
            sqlx::query("DELETE FROM runtime_device_state WHERE workspace_id = ? AND task_id = ?")
                .bind(&task.workspace_id)
                .bind(&task.id)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
        }
        let event = append_runtime_event_tx(
            &mut tx,
            task_event(
                &task,
                &format!("task.{}", task.status.as_str()),
                format!("task:{}:v{}:{}", task.id, task.state_version, task.status.as_str()),
            ),
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok((task, event))
    }

    pub async fn bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError> {
        self.bind_current_run_and_event(task_id, expected_version, run_id)
            .await
            .map(|(task, _)| task)
    }

    pub async fn bind_current_run_and_event(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<(AgentTask, crate::runtime_event::RuntimeEvent), TaskStoreError> {
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let affected = sqlx::query(
            "UPDATE agent_tasks
             SET current_run_id = ?, state_version = state_version + 1, updated_at = ?
             WHERE id = ? AND state_version = ?",
        )
        .bind(run_id)
        .bind(Utc::now())
        .bind(task_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 0 {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        let task = find_task_by_id(&mut tx, task_id)
            .await?
            .ok_or_else(|| TaskStoreError::Unavailable("updated task disappeared".to_string()))?;
        let event = append_runtime_event_tx(
            &mut tx,
            task_event(
                &task,
                "task.run_bound",
                format!("task:{}:v{}:run_bound", task.id, task.state_version),
            ),
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok((task, event))
    }

    pub async fn begin_run(&self, task: &AgentTask, wake_event_id: &str, run_id: &str) -> Result<bool, TaskStoreError> {
        self.begin_run_and_event(task, wake_event_id, run_id)
            .await
            .map(|(started, _)| started)
    }

    pub async fn begin_run_and_event(
        &self,
        task: &AgentTask,
        wake_event_id: &str,
        run_id: &str,
    ) -> Result<(bool, Option<crate::runtime_event::RuntimeEvent>), TaskStoreError> {
        // Paused is resumable but never by starting a Run from an old wake;
        // Cancelling and terminal states must not begin work either.
        if !task.status.can_transition_to(TaskStatus::Active)
            || matches!(task.status, TaskStatus::Paused | TaskStatus::Cancelling)
        {
            return Err(TaskStoreError::Validation(format!(
                "task state transition {} -> active is forbidden",
                task.status.as_str()
            )));
        }
        let predecessors = status_predecessors(TaskStatus::Active);
        let placeholders = vec!["?"; predecessors.len()].join(", ");
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        // The run row insert is deliberately first: it acquires the write lock
        // before any read, and the unique (task, wake) index is the duplicate
        // guard. Task status + current run + wake dispatch then move in the
        // same transaction, so a continuation Run is observable as active.
        let inserted = sqlx::query(
            "INSERT INTO agent_runs
                 (id, workspace_id, trigger_type, trigger_context, outcome, summary, report,
                  status, task_id, wake_event_id)
             VALUES (?, ?, 'task', ?, 'failed', '', '{}', 'running', ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(run_id)
        .bind(&task.workspace_id)
        .bind(wake_event_id)
        .bind(&task.id)
        .bind(wake_event_id)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if inserted == 0 {
            let duplicate: Option<(String,)> =
                sqlx::query_as("SELECT id FROM agent_runs WHERE task_id = ? AND wake_event_id = ?")
                    .bind(&task.id)
                    .bind(wake_event_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(unavailable)?;
            return if duplicate.is_some() {
                tx.commit().await.map_err(unavailable)?;
                Ok((false, None))
            } else {
                Err(TaskStoreError::Unavailable(
                    "agent run conflict was not a task/wake replay".to_string(),
                ))
            };
        }
        let now = Utc::now();
        let statement = format!(
            "UPDATE agent_tasks
             SET status = 'active', current_run_id = ?, state_version = state_version + 1, updated_at = ?
             WHERE workspace_id = ? AND id = ? AND state_version = ? AND status IN ({placeholders})
             RETURNING id, workspace_id, idempotency_key, objective, session_key, created_by,
                       status, current_run_id, state_version, created_at, updated_at, ended_at"
        );
        let mut query = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(statement))
            .bind(run_id)
            .bind(now)
            .bind(&task.workspace_id)
            .bind(&task.id)
            .bind(task.state_version);
        for predecessor in &predecessors {
            query = query.bind(*predecessor);
        }
        let updated = query.fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let Some(updated) = updated else {
            return Err(TaskStoreError::ConcurrentUpdate);
        };
        sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'dispatched', dispatched_at = COALESCE(dispatched_at, ?)
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL",
        )
        .bind(now)
        .bind(&task.id)
        .bind(wake_event_id)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
        let updated_task = AgentTask::try_from(updated)?;
        let event = append_runtime_event_tx(
            &mut tx,
            RuntimeEventInput {
                event_id: format!("run:{run_id}:started"),
                event_type: "run.started".into(),
                occurred_at: now,
                workspace_id: updated_task.workspace_id.clone(),
                task_id: Some(updated_task.id.clone()),
                run_id: Some(run_id.to_string()),
                tool_call_id: None,
                job_id: None,
                approval_id: None,
                device_id: None,
                source_id: None,
                source_seq: None,
                evidence_id: None,
                data: serde_json::json!({"taskStatus": updated_task.status.as_str()}),
            },
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok((true, Some(event)))
    }

    pub async fn finish_run(&self, report: &tinyiothub_core::agent_runs::RunReport) -> Result<(), TaskStoreError> {
        self.finish_run_and_event(report).await.map(|_| ())
    }

    pub async fn finish_run_and_event(
        &self,
        report: &tinyiothub_core::agent_runs::RunReport,
    ) -> Result<crate::runtime_event::RuntimeEvent, TaskStoreError> {
        let mut report_json = serde_json::to_value(report).map_err(unavailable)?;
        report_json["action_count"] = serde_json::json!(report.actions.len());
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let affected = sqlx::query(
            "UPDATE agent_runs
             SET outcome = ?, summary = ?, report = ?, verified = ?, tool_calls = ?,
                 tokens = ?, duration_ms = ?, status = 'completed'
             WHERE id = ? AND workspace_id = ? AND status = 'running'",
        )
        .bind(report.outcome.as_str())
        .bind(&report.summary)
        .bind(report_json.to_string())
        .bind(report.verified)
        .bind(i64::from(report.tool_calls))
        .bind(report.tokens as i64)
        .bind(report.duration_ms as i64)
        .bind(&report.run_id)
        .bind(&report.workspace_id)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 0 {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        let event = append_runtime_event_tx(
            &mut tx,
            RuntimeEventInput {
                event_id: format!("run:{}:finished", report.run_id),
                event_type: "run.finished".into(),
                occurred_at: Utc::now(),
                workspace_id: report.workspace_id.clone(),
                task_id: report.task_id.clone(),
                run_id: Some(report.run_id.clone()),
                tool_call_id: None,
                job_id: None,
                approval_id: None,
                device_id: report.thing_id.clone(),
                source_id: None,
                source_seq: None,
                evidence_id: None,
                data: serde_json::json!({"outcome": report.outcome.as_str()}),
            },
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(event)
    }

    pub async fn latest_task_run(
        &self,
        task_id: &str,
    ) -> Result<Option<tinyiothub_core::agent_runs::RunReport>, TaskStoreError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT report FROM agent_runs
             WHERE task_id = ? AND status = 'completed' AND report IS NOT NULL
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(|(json,)| serde_json::from_str(&json).map_err(unavailable))
            .transpose()
    }

    pub async fn claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError> {
        let now = Utc::now();
        let started = std::time::Instant::now();
        let claimed = sqlx::query_as::<_, TaskWakeRow>(
            "UPDATE task_wake_outbox
             SET dispatched_at = ?
             WHERE (task_id, event_id) IN (
                 SELECT o.task_id, o.event_id FROM task_wake_outbox o
                 JOIN agent_tasks t ON t.id = o.task_id
                 WHERE o.dispatch_state = 'pending' AND o.dispatched_at IS NULL AND o.resolved_at IS NULL
                   AND t.status != 'paused'
                 ORDER BY o.created_at, o.task_id, o.event_id LIMIT ?
             )
             RETURNING task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(TaskWake::try_from)
        .collect::<Result<Vec<_>, _>>()?;
        if !claimed.is_empty() {
            let oldest_lag_ms = claimed
                .iter()
                .map(|wake| (now - wake.created_at).num_milliseconds().max(0) as u64)
                .max()
                .unwrap_or(0);
            tracing::info!(
                metric = "runtime_outbox_claimed",
                count = claimed.len(),
                oldest_lag_ms,
                claim_ms = started.elapsed().as_millis() as u64,
                "wake outbox claimed"
            );
        }
        Ok(claimed)
    }

    pub async fn recover_claimed_wakes(&self) -> Result<usize, TaskStoreError> {
        let affected = sqlx::query(
            "UPDATE task_wake_outbox SET dispatched_at = NULL
             WHERE dispatch_state = 'pending' AND dispatched_at IS NOT NULL",
        )
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        Ok(affected as usize)
    }

    pub async fn mark_wake(
        &self,
        task_id: &str,
        event_id: &str,
        state: tinyiothub_core::agent_task::WakeDispatchState,
    ) -> Result<(), TaskStoreError> {
        let state = state.as_str();
        let now = Utc::now();
        let affected = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = ?,
                 dispatched_at = CASE ? WHEN 'dispatched' THEN ? WHEN 'pending' THEN NULL ELSE dispatched_at END,
                 resolved_at = CASE ? WHEN 'dead' THEN ? WHEN 'pending' THEN NULL ELSE resolved_at END
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL
               AND (? != 'dispatched' OR dispatched_at IS NOT NULL)",
        )
        .bind(state)
        .bind(state)
        .bind(now)
        .bind(state)
        .bind(now)
        .bind(task_id)
        .bind(event_id)
        .bind(state)
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 1 {
            tracing::info!(
                metric = "runtime_outbox_marked",
                %task_id,
                %event_id,
                state,
                "wake outbox marked"
            );
        }
        Ok(())
    }

    /// Mark a wake dead together with the bounded failure reason. Kept
    /// separate from [`Self::mark_wake`] so every non-dead transition keeps
    /// its existing signature and guards.
    pub async fn mark_wake_dead(&self, task_id: &str, event_id: &str, reason: &str) -> Result<(), TaskStoreError> {
        let now = Utc::now();
        let affected = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'dead', resolved_at = ?, dead_reason = ?
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(task_id)
        .bind(event_id)
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 1 {
            tracing::info!(
                metric = "runtime_outbox_marked",
                %task_id,
                %event_id,
                state = "dead",
                reason,
                "wake outbox marked"
            );
        }
        Ok(())
    }

    /// Stage the finalize intent and requeue its wake in one transaction. The
    /// wake update runs first so SQLite takes the write lock before reading,
    /// and clearing `dispatched_at` means the original dispatch's later
    /// `Dispatched` acknowledgement is rejected by `mark_wake`'s guard.
    pub async fn requeue_with_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        let now = Utc::now();
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let requeued = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'pending', dispatched_at = NULL
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL",
        )
        .bind(&intent.task_id)
        .bind(&intent.wake_event_id)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if requeued != 1 {
            // A resolved wake belongs to a cancelled/terminal Task; do not
            // resurrect it or leave an unreachable intent behind.
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        upsert_finalize_intent_tx(&mut tx, intent, now).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    /// Persist a finalize intent without changing the wake state. This backs
    /// the in-memory staging retry after a transient atomic-stage failure.
    pub async fn put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        upsert_finalize_intent_tx(&mut tx, intent, Utc::now()).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    pub async fn finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError> {
        sqlx::query_as::<_, FinalizeIntentRow>(
            "SELECT i.task_id, i.workspace_id, i.wake_event_id, i.run_id, i.report_json,
                    i.report_persisted, i.report_emitted, i.undelivered_json
             FROM task_finalize_intents i WHERE i.task_id = ? AND i.wake_event_id = ?",
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .map(FinalizeIntentRow::into_intent)
        .transpose()
    }

    /// Intents restart recovery must honor: their wake is still live and the
    /// Task still exists and is nonterminal. Ordered by creation time. This is a
    /// startup read of retained teardown failures, not a user-facing list, so it
    /// deliberately has no pagination cap.
    pub async fn pending_finalize_intents(&self) -> Result<Vec<TaskFinalizeIntent>, TaskStoreError> {
        sqlx::query_as::<_, FinalizeIntentRow>(
            "SELECT i.task_id, i.workspace_id, i.wake_event_id, i.run_id, i.report_json,
                    i.report_persisted, i.report_emitted, i.undelivered_json
             FROM task_finalize_intents i
             JOIN task_wake_outbox w
               ON w.task_id = i.task_id AND w.event_id = i.wake_event_id
             JOIN agent_tasks t ON t.id = i.task_id
             WHERE w.resolved_at IS NULL
               AND t.status NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY i.created_at ASC, i.task_id ASC, i.wake_event_id ASC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(FinalizeIntentRow::into_intent)
        .collect()
    }

    pub async fn complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError> {
        sqlx::query("DELETE FROM task_finalize_intents WHERE task_id = ? AND wake_event_id = ?")
            .bind(task_id)
            .bind(event_id)
            .execute(self.pool())
            .await
            .map_err(unavailable)?;
        Ok(())
    }

    /// The completed Run report for `run_id`. `finish_run` returning
    /// `ConcurrentUpdate` is ambiguous: the prior write may have committed and
    /// lost only its acknowledgement. This read resolves that ambiguity.
    pub async fn completed_run_report(
        &self,
        run_id: &str,
    ) -> Result<Option<tinyiothub_core::agent_runs::RunReport>, TaskStoreError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT report FROM agent_runs
             WHERE id = ? AND status = 'completed' AND report IS NOT NULL",
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(|(json,)| serde_json::from_str(&json).map_err(unavailable))
            .transpose()
    }

    pub async fn count_finalize_intents(&self) -> Result<i64, TaskStoreError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM task_finalize_intents")
            .fetch_one(self.pool())
            .await
            .map_err(unavailable)
    }

    /// Bounded retention sweep for finalize intents. A row is removable only
    /// when it is older than the cutoff **and** its wake is no longer live
    /// (resolved/dead/missing) **and** its Task is terminal or missing. A
    /// dead-wake intent on a live Task is retained: `retry_dead_wake` can
    /// resurrect the wake, and for `report_persisted = false` the intent is the
    /// only copy of the model output.
    pub async fn prune_finalize_intents(&self, older_than: DateTime<Utc>) -> Result<usize, TaskStoreError> {
        let affected = sqlx::query(
            "DELETE FROM task_finalize_intents
             WHERE updated_at < ?
               AND NOT EXISTS (
                   SELECT 1 FROM task_wake_outbox w
                   WHERE w.task_id = task_finalize_intents.task_id
                     AND w.event_id = task_finalize_intents.wake_event_id
                     AND w.resolved_at IS NULL
               )
               AND NOT EXISTS (
                   SELECT 1 FROM agent_tasks t
                   WHERE t.id = task_finalize_intents.task_id
                     AND t.status NOT IN ('completed', 'failed', 'cancelled')
               )",
        )
        .bind(older_than)
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        Ok(affected as usize)
    }

    pub async fn retry_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, TaskStoreError> {
        let affected = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'pending', dispatched_at = NULL, resolved_at = NULL
             WHERE event_id = ? AND dispatch_state = 'dead'
               AND task_id IN (SELECT id FROM agent_tasks WHERE workspace_id = ?)",
        )
        .bind(event_id)
        .bind(workspace_id)
        .execute(self.pool())
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected > 0 {
            tracing::info!(
                metric = "runtime_outbox_retried",
                %workspace_id,
                %event_id,
                "dead wake retried"
            );
        }
        Ok(affected > 0)
    }

    pub async fn list_nonterminal_tasks(&self, workspace_id: Option<&str>) -> Result<Vec<AgentTask>, TaskStoreError> {
        let rows = if let Some(workspace_id) = workspace_id {
            sqlx::query_as::<_, AgentTaskRow>(
                "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                        status, current_run_id, state_version, created_at, updated_at, ended_at
                 FROM agent_tasks
                 WHERE workspace_id = ? AND status NOT IN ('completed', 'failed', 'cancelled')
                 ORDER BY updated_at, id",
            )
            .bind(workspace_id)
            .fetch_all(self.pool())
            .await
        } else {
            sqlx::query_as::<_, AgentTaskRow>(
                "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                        status, current_run_id, state_version, created_at, updated_at, ended_at
                 FROM agent_tasks
                 WHERE status NOT IN ('completed', 'failed', 'cancelled')
                 ORDER BY updated_at, id",
            )
            .fetch_all(self.pool())
            .await
        }
        .map_err(unavailable)?;
        rows.into_iter().map(AgentTask::try_from).collect()
    }
}

fn validate_wake(workspace_id: &str, task_id: &str, wake: &TaskWake) -> Result<(), TaskStoreError> {
    let signal: serde_json::Value = serde_json::from_str(&wake.signal_json)
        .map_err(|error| TaskStoreError::Validation(format!("invalid wake signal: {error}")))?;
    let signal_workspace = signal
        .get("workspace_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| TaskStoreError::Validation("wake signal has no workspace_id".to_string()))?;
    if workspace_id != signal_workspace || wake.task_id != task_id {
        return Err(TaskStoreError::Validation(
            "task and wake workspace or task id differ".to_string(),
        ));
    }
    Ok(())
}

async fn find_task_by_id(
    executor: &mut sqlx::SqliteConnection,
    task_id: &str,
) -> Result<Option<AgentTask>, TaskStoreError> {
    sqlx::query_as::<_, AgentTaskRow>(
        "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                status, current_run_id, state_version, created_at, updated_at, ended_at
         FROM agent_tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_optional(executor)
    .await
    .map_err(unavailable)?
    .map(AgentTask::try_from)
    .transpose()
}

async fn find_task_by_key(
    executor: &mut sqlx::SqliteConnection,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<AgentTask>, TaskStoreError> {
    sqlx::query_as::<_, AgentTaskRow>(
        "SELECT id, workspace_id, idempotency_key, objective, session_key, created_by,
                status, current_run_id, state_version, created_at, updated_at, ended_at
         FROM agent_tasks WHERE workspace_id = ? AND idempotency_key = ?",
    )
    .bind(workspace_id)
    .bind(idempotency_key)
    .fetch_optional(executor)
    .await
    .map_err(unavailable)?
    .map(AgentTask::try_from)
    .transpose()
}

fn harness_task_event(
    workspace_id: &str,
    task_id: &str,
    run_id: Option<&str>,
    event_id: String,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    payload_json: Value,
) -> DbResult<HarnessEventInput> {
    let payload = serde_json::to_string(&payload_json)?;
    Ok(HarnessEventInput {
        event_id,
        workspace_id: workspace_id.to_string(),
        session_id: run_id
            .map(|run_id| format!("task:{task_id}/run:{run_id}"))
            .unwrap_or_else(|| format!("task:{task_id}")),
        execution_mode: ExecutionMode::Production,
        run_id: run_id.map(str::to_string),
        task_id: Some(task_id.to_string()),
        event_type: event_type.to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", sha2::Sha256::digest(payload.as_bytes())),
        payload_json,
    })
}

pub(crate) async fn drain_cancelling_tasks(
    connection: &mut SqliteConnection,
    workspace_id: &str,
    now: DateTime<Utc>,
) -> DbResult<()> {
    let tasks = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
        "SELECT id, parent_task_id, run_id, state_version FROM agent_tasks
         WHERE state = 'cancelling' AND execution_mode = 'production'
         ORDER BY CASE WHEN parent_task_id IS NULL THEN 1 ELSE 0 END, id",
    )
    .fetch_all(&mut *connection)
    .await?;
    for (task_id, _, run_id, expected_version) in tasks {
        let blocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM agent_tasks child
                 WHERE child.parent_task_id = ?
                   AND child.state NOT IN ('completed', 'failed', 'cancelled')
             ) OR EXISTS(
                 SELECT 1 FROM effect_intents effect
                 JOIN agent_tasks owner ON owner.id = effect.task_id
                 WHERE (owner.id = ? OR owner.parent_task_id = ?)
                   AND effect.state NOT IN
                       ('completed', 'failed', 'not_applied', 'cancelled', 'cancelled_unknown')
             )",
        )
        .bind(&task_id)
        .bind(&task_id)
        .bind(&task_id)
        .fetch_one(&mut *connection)
        .await?;
        if blocked {
            continue;
        }
        let cancel_unknown = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM effect_intents effect
                 JOIN agent_tasks owner ON owner.id = effect.task_id
                 WHERE (owner.id = ? OR owner.parent_task_id = ?)
                   AND effect.state = 'cancelled_unknown'
             )",
        )
        .bind(&task_id)
        .bind(&task_id)
        .fetch_one(&mut *connection)
        .await?;
        let next = if cancel_unknown { "failed" } else { "cancelled" };
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = 'cancelling' AND state_version = ?",
        )
        .bind(next)
        .bind(&task_id)
        .bind(expected_version)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if updated != 1 {
            continue;
        }
        sqlx::query("UPDATE agent_runs SET state = ? WHERE task_id = ?")
            .bind(next)
            .bind(&task_id)
            .execute(&mut *connection)
            .await?;
        let reason = if cancel_unknown { "cancel_unknown" } else { "cancelled" };
        let event = harness_task_event(
            workspace_id,
            &task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:{next}", expected_version + 1),
            &format!("task.{next}"),
            now,
            json!({"taskId":task_id,"state":next,"reason":reason}),
        )?;
        crate::harness_effect::insert_event(connection, &event).await?;
    }
    Ok(())
}

impl Db {
    pub async fn pause_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT state, run_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, version) = current;
        if state == "paused" {
            return Ok(());
        }
        if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
            return Err(DbError::Conflict {
                message: format!("task {task_id} cannot be paused from {state}"),
            });
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = 'paused', state_version = state_version + 1
             WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .bind(&state)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("task {task_id} changed while pausing"),
            });
        }
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:paused", version + 1),
            "task.paused",
            Utc::now(),
            json!({"taskId":task_id,"state":"paused","previousState":state}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn resume_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<String> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT state, run_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, version) = current;
        if state != "paused" {
            return Ok(state);
        }
        let previous = sqlx::query_scalar::<_, String>(
            "SELECT json_extract(payload_json, '$.previousState') FROM session_events
             WHERE task_id = ? AND type IN ('task.paused', 'task.pause_waiting')
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::Internal(format!("paused task {task_id} has no waiting relation")))?;
        if !matches!(
            previous.as_str(),
            "pending" | "running" | "waiting_job" | "waiting_watch" | "needs_approval"
        ) {
            return Err(DbError::Internal(format!(
                "paused task {task_id} has invalid previous state {previous}"
            )));
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = 'paused' AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(&previous)
        .bind(task_id)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("task {task_id} changed while resuming"),
            });
        }
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            format!("task:{task_id}:v{}:resumed", version + 1),
            "task.resumed",
            Utc::now(),
            json!({"taskId":task_id,"state":previous}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(previous)
    }

    pub async fn cancel_workflow_task(&self, workspace_id: &str, task_id: &str) -> DbResult<Vec<String>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let root_state = sqlx::query_scalar::<_, String>(
            "SELECT state FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        if matches!(root_state.as_str(), "completed" | "failed" | "cancelled") {
            return Ok(Vec::new());
        }
        let mut task_ids = vec![task_id.to_string()];
        task_ids.extend(
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM agent_tasks WHERE parent_task_id = ? AND execution_mode = 'production' ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&mut *tx)
            .await?,
        );
        let now = Utc::now();
        for id in &task_ids {
            let current = sqlx::query_as::<_, (String, Option<String>, i64)>(
                "SELECT state, run_id, state_version FROM agent_tasks
                 WHERE id = ? AND execution_mode = 'production'",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let (state, run_id, version) = current;
            if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
                continue;
            }
            let updated = sqlx::query(
                "UPDATE agent_tasks SET state = 'cancelling', state_version = state_version + 1
                 WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
            )
            .bind(id)
            .bind(&state)
            .bind(version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(DbError::CasConflict {
                    message: format!("task {id} changed while cancelling"),
                });
            }
            sqlx::query(
                "UPDATE task_wake_outbox
                 SET dispatch_state = 'dead', resolved_at = ?, failure_reason = 'cancelled'
                 WHERE task_id = ? AND dispatch_state = 'pending'",
            )
            .bind(now.to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
            let event = harness_task_event(
                workspace_id,
                id,
                run_id.as_deref(),
                format!("task:{id}:v{}:cancelling", version + 1),
                "task.cancelling",
                now,
                json!({"taskId":id,"state":"cancelling","reason":"cancel_requested"}),
            )?;
            crate::harness_effect::insert_event(&mut tx, &event).await?;
        }
        drain_cancelling_tasks(&mut tx, workspace_id, now).await?;
        tx.commit().await?;
        Ok(task_ids)
    }

    /// Non-terminal direct Subagents of a Task. Subagents never nest, so one
    /// level is the whole tree; this is the cancel complement used when the
    /// live coordinator, not the workspace cascade, transitions the root.
    pub async fn harness_active_child_task_ids(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
    ) -> DbResult<Vec<String>> {
        let pool = self.workspace_pool(workspace_id).await?;
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT id FROM agent_tasks
             WHERE parent_task_id = ? AND execution_mode = 'production'
               AND state NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY id",
        )
        .bind(parent_task_id)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn drain_workflow_cancellation(&self, workspace_id: &str) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        drain_cancelling_tasks(&mut tx, workspace_id, Utc::now()).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Nonterminal root Tasks for the Harness overview. Subagents are loaded
    /// separately so a Main with many children stays bounded.
    pub async fn harness_main_tasks(&self, workspace_id: &str) -> DbResult<Vec<HarnessTaskRow>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, HarnessTaskDbRow>(
            "SELECT t.id, t.run_id, t.state,
                    json_extract(r.definition_json, '$.objective') AS objective,
                    (SELECT e.payload_json FROM session_events e
                      WHERE e.task_id = t.id AND e.type = 'task.outcome'
                      ORDER BY e.seq DESC LIMIT 1) AS outcome_json,
                    (SELECT MAX(e2.occurred_at) FROM session_events e2
                      WHERE e2.task_id = t.id) AS updated_at,
                    t.parent_task_id
             FROM agent_tasks t
             LEFT JOIN workflow_revisions r
               ON r.workflow_id = t.workflow_id AND r.version = t.workflow_version
             WHERE t.parent_task_id IS NULL
               AND t.execution_mode = 'production'
               AND t.state NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY t.id",
        )
        .fetch_all(&pool)
        .await?;
        Ok(rows.into_iter().map(HarnessTaskRow::from).collect())
    }

    /// One bounded page of children for a Main Task. A missing Main returns
    /// NotFound; tasks from other parents are never included.
    pub async fn harness_subagents(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
        limit: i64,
        offset: i64,
    ) -> DbResult<(Vec<HarnessTaskRow>, i64)> {
        let pool = self.workspace_pool(workspace_id).await?;
        let exists = sqlx::query_scalar::<_, String>(
            "SELECT id FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_optional(&pool)
        .await?;
        if exists.is_none() {
            return Err(DbError::NotFound {
                id: parent_task_id.to_string(),
            });
        }
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_tasks WHERE parent_task_id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_one(&pool)
        .await?;
        let rows = sqlx::query_as::<_, HarnessTaskDbRow>(
            "SELECT t.id, t.run_id, t.state,
                    json_extract(r.definition_json, '$.objective') AS objective,
                    (SELECT e.payload_json FROM session_events e
                      WHERE e.task_id = t.id AND e.type = 'task.outcome'
                      ORDER BY e.seq DESC LIMIT 1) AS outcome_json,
                    (SELECT MAX(e2.occurred_at) FROM session_events e2
                      WHERE e2.task_id = t.id) AS updated_at,
                    t.parent_task_id
             FROM agent_tasks t
             LEFT JOIN workflow_revisions r
               ON r.workflow_id = t.workflow_id AND r.version = t.workflow_version
             WHERE t.parent_task_id = ?
               AND t.execution_mode = 'production'
             ORDER BY t.id LIMIT ? OFFSET ?",
        )
        .bind(parent_task_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await?;
        Ok((rows.into_iter().map(HarnessTaskRow::from).collect(), total))
    }

    pub async fn create_subagent_task(
        &self,
        workspace_id: &str,
        parent_task_id: &str,
        task_id: &str,
        run_id: &str,
        now: DateTime<Utc>,
    ) -> DbResult<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let parent = sqlx::query_as::<_, (Option<String>, Option<String>, Option<i64>, Option<String>, String)>(
            "SELECT parent_task_id, workflow_id, workflow_version, run_id, state
             FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(parent_task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: parent_task_id.to_string(),
        })?;
        let (grandparent, workflow_id, workflow_version, parent_run_id, parent_state) = parent;
        if grandparent.is_some() {
            return Err(DbError::Validation {
                message: "v1 subagents cannot create child tasks".to_string(),
            });
        }
        if matches!(
            parent_state.as_str(),
            "paused" | "cancelling" | "completed" | "failed" | "cancelled"
        ) {
            return Err(DbError::Conflict {
                message: format!("parent task {parent_task_id} cannot dispatch a child from {parent_state}"),
            });
        }
        let workflow_id = workflow_id.ok_or_else(|| DbError::Validation {
            message: "parent task has no pinned workflow".to_string(),
        })?;
        let workflow_version = workflow_version.ok_or_else(|| DbError::Validation {
            message: "parent task has no pinned workflow version".to_string(),
        })?;
        sqlx::query(
            "INSERT INTO agent_tasks
                (id, run_id, state, state_version, workflow_id, workflow_version, parent_task_id)
             VALUES (?, ?, 'pending', 1, ?, ?, ?)",
        )
        .bind(task_id)
        .bind(run_id)
        .bind(&workflow_id)
        .bind(workflow_version)
        .bind(parent_task_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO agent_runs (id, task_id, state, parent_run_id) VALUES (?, ?, 'pending', ?)")
            .bind(run_id)
            .bind(task_id)
            .bind(parent_run_id)
            .execute(&mut *tx)
            .await?;
        let signal = json!({
            "workspace_id":workspace_id,
            "task_id":task_id,
            "run_id":run_id,
            "workflow_id":workflow_id,
            "workflow_version":workflow_version,
            "parent_task_id":parent_task_id,
        });
        sqlx::query(
            "INSERT INTO task_wake_outbox
                (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, 'pending', ?, NULL, NULL)",
        )
        .bind(task_id)
        .bind(format!("task:{task_id}:created"))
        .bind(serde_json::to_string(&signal)?)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        let event = harness_task_event(
            workspace_id,
            task_id,
            Some(run_id),
            format!("task:{task_id}:created"),
            "task.created",
            now,
            json!({"taskId":task_id,"parentTaskId":parent_task_id,"workflowVersion":workflow_version}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_subagent_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        outcome_status: &str,
        outcome: Value,
        now: DateTime<Utc>,
    ) -> DbResult<bool> {
        if !matches!(outcome_status, "completed" | "failed") {
            return Err(DbError::Validation {
                message: format!("invalid subagent outcome status {outcome_status}"),
            });
        }
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
            "SELECT state, run_id, parent_task_id, state_version FROM agent_tasks
             WHERE id = ? AND execution_mode = 'production'",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound {
            id: task_id.to_string(),
        })?;
        let (state, run_id, parent_task_id, version) = current;
        if matches!(state.as_str(), "cancelling" | "completed" | "failed" | "cancelled") {
            return Ok(false);
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks SET state = ?, state_version = state_version + 1
             WHERE id = ? AND state = ? AND state_version = ? AND execution_mode = 'production'",
        )
        .bind(outcome_status)
        .bind(task_id)
        .bind(&state)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(DbError::CasConflict {
                message: format!("subagent task {task_id} changed while completing"),
            });
        }
        if let Some(run_id) = &run_id {
            sqlx::query("UPDATE agent_runs SET state = ? WHERE id = ? AND task_id = ?")
                .bind(outcome_status)
                .bind(run_id)
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
        }
        let event_id = format!("task:{task_id}:outcome");
        let event = harness_task_event(
            workspace_id,
            task_id,
            run_id.as_deref(),
            event_id.clone(),
            "task.outcome",
            now,
            json!({"taskId":task_id,"outcome":outcome}),
        )?;
        crate::harness_effect::insert_event(&mut tx, &event).await?;
        if let Some(parent_task_id) = parent_task_id {
            let signal = json!({"workspace_id":workspace_id,"taskId":parent_task_id,"outcomeEventId":event_id});
            sqlx::query(
                "INSERT INTO task_wake_outbox
                    (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
                 SELECT ?, ?, ?, 'pending', ?, NULL, NULL FROM agent_tasks parent
                 WHERE parent.id = ? AND parent.state NOT IN ('cancelling', 'completed', 'failed', 'cancelled')
                 ON CONFLICT(task_id, event_id) DO NOTHING",
            )
            .bind(&parent_task_id)
            .bind(format!("{event_id}:parent-wake"))
            .bind(serde_json::to_string(&signal)?)
            .bind(now.to_rfc3339())
            .bind(parent_task_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{TaskStatus, WakeDispatchState};

    use super::*;

    fn sample_task(id: &str, workspace_id: &str, key: &str, objective: &str) -> AgentTask {
        let now = Utc::now();
        AgentTask {
            id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            idempotency_key: key.to_string(),
            objective: objective.to_string(),
            session_key: None,
            created_by: "user_1".to_string(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        }
    }

    fn sample_wake(task_id: &str, event_id: &str, workspace_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.to_string(),
            event_id: event_id.to_string(),
            signal_json: serde_json::json!({"workspace_id": workspace_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn sample_report(run_id: &str, wake_event_id: &str) -> tinyiothub_core::agent_runs::RunReport {
        tinyiothub_core::agent_runs::RunReport {
            run_id: run_id.into(),
            workspace_id: "ws_1".into(),
            trigger: "task:task_1".into(),
            outcome: tinyiothub_core::agent_runs::Outcome::Acted,
            summary: "finished".into(),
            actions: vec![],
            verified: true,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some("task_1".into()),
            wake_event_id: Some(wake_event_id.into()),
        }
    }

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

    #[tokio::test]
    async fn same_key_concurrent_create_returns_one_task_and_one_wake() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect both pumps");
        let wake = sample_wake("task_1", "event_1", "ws_1");
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let mut joins = Vec::new();
        for _ in 0..2 {
            let db = db.clone();
            let task = task.clone();
            let wake = wake.clone();
            let barrier = barrier.clone();
            joins.push(tokio::spawn(async move {
                barrier.wait().await;
                db.create_task_with_wake(&task, &wake).await.expect("create")
            }));
        }
        let a = joins.remove(0).await.expect("first join");
        let b = joins.remove(0).await.expect("second join");
        assert_eq!(a.task.id, b.task.id);
        assert_ne!(a.deduplicated, b.deduplicated);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("wake count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn same_key_different_objective_conflicts_without_new_wake() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let wake = sample_wake("task_1", "event_1", "ws_1");
        db.create_task_with_wake(&sample_task("task_1", "ws_1", "key_1", "A"), &wake)
            .await
            .expect("first create");
        let error = db
            .create_task_with_wake(&sample_task("task_2", "ws_1", "key_1", "B"), &wake)
            .await
            .expect_err("objective conflict");
        assert!(matches!(error, TaskStoreError::ObjectiveConflict));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("wake count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn stale_transition_writes_neither_status_nor_wake() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        let error = db
            .transition_task(
                "ws_1",
                "task_1",
                1,
                TaskStatus::Paused,
                Some(&sample_wake("task_1", "event_2", "ws_1")),
            )
            .await
            .expect_err("stale version must conflict");
        assert!(matches!(error, TaskStoreError::ConcurrentUpdate));
        let stored = db.get_task("ws_1", "task_1").await.expect("get").expect("task");
        assert_eq!(stored.status, TaskStatus::Active);
        assert_eq!(stored.state_version, 0);
        assert!(db.wake_by_id("task_1", "event_2").await.expect("wake lookup").is_none());
    }

    #[tokio::test]
    async fn wake_failure_rolls_back_transition() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        let duplicate = sample_wake("task_1", "event_1", "ws_1");
        db.create_task_with_wake(&task, &duplicate).await.expect("create");

        db.transition_task("ws_1", "task_1", 0, TaskStatus::Paused, Some(&duplicate))
            .await
            .expect_err("duplicate wake must fail transaction");
        let stored = db.get_task("ws_1", "task_1").await.expect("get").expect("task");
        assert_eq!(stored.status, TaskStatus::Active);
        assert_eq!(stored.state_version, 0);
    }

    #[tokio::test]
    async fn write_first_transition_waits_for_an_interleaved_writer_instead_of_failing_storage() {
        let path = std::env::temp_dir().join(format!("tinyiothub-interleave-{}.db", uuid::Uuid::new_v4()));
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .expect("file-backed pool");
        crate::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool.clone());
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        // A deferred transaction takes a read snapshot and then a write lock.
        // A read-first writer would see SQLITE_BUSY_SNAPSHOT (code 517, not
        // covered by busy_timeout) as soon as this holder commits.
        let mut holder = pool.begin().await.expect("holder transaction");
        let _snapshot: (String,) = sqlx::query_as("SELECT status FROM agent_tasks WHERE id = 'task_1'")
            .fetch_one(&mut *holder)
            .await
            .expect("snapshot read");
        sqlx::query("UPDATE agent_tasks SET updated_at = updated_at WHERE id = 'task_1'")
            .execute(&mut *holder)
            .await
            .expect("hold write lock");

        let writer = db.clone();
        let pending = tokio::spawn(async move {
            writer
                .transition_task("ws_1", "task_1", 0, TaskStatus::Paused, None)
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        holder.commit().await.expect("release holder");
        let stored = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
            .await
            .expect("transition completes")
            .expect("join")
            .expect("write-first transition must wait instead of returning Storage");
        assert_eq!(stored.status, TaskStatus::Paused);

        pool.close().await;
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
    }

    #[tokio::test]
    async fn transition_and_bind_are_versioned_and_terminal_sets_ended_at() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        let waiting = db
            .transition_task(
                "ws_1",
                "task_1",
                0,
                TaskStatus::WaitingWatch,
                Some(&sample_wake("task_1", "event_2", "ws_1")),
            )
            .await
            .expect("transition");
        assert_eq!(waiting.state_version, 1);
        assert_eq!(waiting.status, TaskStatus::WaitingWatch);
        assert!(waiting.ended_at.is_none());
        assert!(db.wake_by_id("task_1", "event_2").await.expect("wake").is_some());

        let bound = db.bind_current_run("task_1", 1, Some("run_1")).await.expect("bind run");
        assert_eq!(bound.current_run_id.as_deref(), Some("run_1"));
        assert_eq!(bound.state_version, 2);
        let completed = db
            .transition_task("ws_1", "task_1", 2, TaskStatus::Completed, None)
            .await
            .expect("complete");
        assert_eq!(completed.state_version, 3);
        assert!(completed.ended_at.is_some());
    }

    #[tokio::test]
    async fn ledger_append_failure_rolls_back_task_fact() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        let events_before = db
            .runtime_events_after("ws_1", None, Some("task_1"), 10)
            .await
            .expect("events before")
            .len();
        sqlx::query(
            "CREATE TRIGGER reject_runtime_event BEFORE INSERT ON hub_ledger
             WHEN NEW.kind = 'runtime.v1' BEGIN SELECT RAISE(ABORT, 'injected ledger failure'); END",
        )
        .execute(db.pool())
        .await
        .expect("install failure injector");

        db.transition_task("ws_1", "task_1", 0, TaskStatus::Paused, None)
            .await
            .expect_err("ledger failure must reject the owning transition");
        let stored = db.get_task("ws_1", "task_1").await.expect("get").expect("task");
        assert_eq!(stored.status, TaskStatus::Active);
        assert_eq!(stored.state_version, 0);
        assert_eq!(
            db.runtime_events_after("ws_1", None, Some("task_1"), 10)
                .await
                .expect("events after")
                .len(),
            events_before
        );
        sqlx::query("DROP TRIGGER reject_runtime_event")
            .execute(db.pool())
            .await
            .expect("remove failure injector");
        db.transition_task("ws_1", "task_1", 0, TaskStatus::Paused, None)
            .await
            .expect("retry after ledger recovery");
        let events = db
            .runtime_events_after("ws_1", None, Some("task_1"), 10)
            .await
            .expect("replayed events");
        assert_eq!(
            events.last().map(|event| event.event_id.as_str()),
            Some("task:task_1:v1:paused")
        );
    }

    #[tokio::test]
    async fn cross_workspace_signal_is_rejected_without_rows() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        seed_workspace(&pool, "ws_2").await;
        let db = Db::new(pool);
        let error = db
            .create_task_with_wake(
                &sample_task("task_1", "ws_1", "key_1", "inspect"),
                &sample_wake("task_1", "event_1", "ws_2"),
            )
            .await
            .expect_err("cross-workspace wake");
        assert!(matches!(error, TaskStoreError::Validation(_)));
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasks")
            .fetch_one(db.pool())
            .await
            .expect("task count");
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("wake count");
        assert_eq!((tasks, wakes), (0, 0));
    }

    #[tokio::test]
    async fn recorded_wakes_are_idempotent_and_resolved_wakes_are_not_claimed() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        let wake = sample_wake("task_1", "event_2", "ws_1");
        assert!(db.record_task_wake(&wake).await.expect("record"));
        assert!(!db.record_task_wake(&wake).await.expect("duplicate"));
        assert_eq!(db.resolve_task_wakes("task_1").await.expect("resolve"), 2);
        assert!(db.claim_pending_wakes(10).await.expect("claim").is_empty());
        assert!(
            db.wake_by_id("task_1", "event_2")
                .await
                .expect("wake")
                .expect("stored")
                .resolved_at
                .is_some()
        );
    }

    #[tokio::test]
    async fn paused_or_cancelling_task_cannot_begin_a_run() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        let paused = db
            .transition_task("ws_1", "task_1", 0, TaskStatus::Paused, None)
            .await
            .expect("pause");
        let error = db
            .begin_run(&paused, "event_1", "run_1")
            .await
            .expect_err("paused Task must not begin a Run");
        assert!(matches!(error, TaskStoreError::Validation(_)), "{error:?}");

        let cancelling = db
            .transition_task("ws_1", "task_1", 1, TaskStatus::Cancelling, None)
            .await
            .expect("cancel");
        let error = db
            .begin_run(&cancelling, "event_1", "run_1")
            .await
            .expect_err("cancelling Task must not begin a Run");
        assert!(matches!(error, TaskStoreError::Validation(_)), "{error:?}");

        let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs")
            .fetch_one(db.pool())
            .await
            .expect("run count");
        assert_eq!(runs, 0);
        let wake = db
            .wake_by_id("task_1", "event_1")
            .await
            .expect("wake")
            .expect("stored wake");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
    }

    #[tokio::test]
    async fn terminal_transition_deletes_device_state_and_nonterminal_keeps_it() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        let seed_state = || {
            sqlx::query(
                "INSERT INTO runtime_device_state (workspace_id, task_id, device_id, state_json, updated_at)
                 VALUES ('ws_1', 'task_1', 'device_1', '{\"temperature\":\"21\"}', 'now')",
            )
            .execute(db.pool())
        };
        seed_state().await.expect("state");

        db.transition_task("ws_1", "task_1", 0, TaskStatus::WaitingJob, None)
            .await
            .expect("waiting");
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runtime_device_state")
            .fetch_one(db.pool())
            .await
            .expect("nonterminal state count");
        assert_eq!(rows, 1, "a live Task keeps its device projection");

        db.transition_task("ws_1", "task_1", 1, TaskStatus::Failed, None)
            .await
            .expect("fail");
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runtime_device_state")
            .fetch_one(db.pool())
            .await
            .expect("terminal state count");
        assert_eq!(rows, 0, "a terminal Task leaves no device projection behind");
    }

    #[tokio::test]
    async fn run_insert_deduplicates_task_wake_and_finish_updates_same_row() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        assert!(db.begin_run(&task, "event_1", "run_1").await.expect("first run"));
        assert!(!db.begin_run(&task, "event_1", "run_2").await.expect("replay"));
        let report = tinyiothub_core::agent_runs::RunReport {
            run_id: "run_1".to_string(),
            workspace_id: "ws_1".to_string(),
            trigger: "task:event_1".to_string(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "inspection complete".to_string(),
            actions: vec![],
            verified: true,
            duration_ms: 42,
            tool_calls: 1,
            tokens: 7,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some("task_1".into()),
            wake_event_id: Some("event_1".into()),
        };
        db.finish_run(&report).await.expect("finish");

        let row: (String, String, String, String, String, i64) = sqlx::query_as(
            "SELECT id, task_id, wake_event_id, outcome, status, json_extract(report, '$.action_count') FROM agent_runs",
        )
        .fetch_one(db.pool())
        .await
        .expect("stored run");
        assert_eq!(
            row,
            (
                "run_1".into(),
                "task_1".into(),
                "event_1".into(),
                "no_action_needed".into(),
                "completed".into(),
                0
            )
        );
        let report_json = sqlx::query_scalar::<_, String>("SELECT report FROM agent_runs WHERE id = 'run_1'")
            .fetch_one(db.pool())
            .await
            .expect("report json");
        let stored: tinyiothub_core::agent_runs::RunReport = serde_json::from_str(&report_json).expect("report parses");
        assert_eq!(stored.summary, "inspection complete");

        let current = db
            .get_task("ws_1", "task_1")
            .await
            .expect("get")
            .expect("task after first run");
        assert!(db.begin_run(&current, "event_2", "run_2").await.expect("next run"));
        let latest = db.latest_task_run("task_1").await.expect("latest completed report");
        assert_eq!(latest.expect("completed report").run_id, "run_1");
    }

    #[tokio::test]
    async fn finish_run_rejects_a_terminal_run_instead_of_overwriting_it() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        assert!(db.begin_run(&task, "event_1", "run_1").await.expect("run"));
        let mut report = tinyiothub_core::agent_runs::RunReport {
            run_id: "run_1".to_string(),
            workspace_id: "ws_1".to_string(),
            trigger: "task:event_1".to_string(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "first finish".to_string(),
            actions: vec![],
            verified: true,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some("task_1".into()),
            wake_event_id: Some("event_1".into()),
        };
        db.finish_run(&report).await.expect("first finish");
        report.summary = "stale duplicate finish".to_string();
        assert!(matches!(
            db.finish_run(&report).await,
            Err(TaskStoreError::ConcurrentUpdate)
        ));
        let stored: String = sqlx::query_scalar("SELECT summary FROM agent_runs WHERE id = 'run_1'")
            .fetch_one(db.pool())
            .await
            .expect("summary");
        assert_eq!(stored, "first finish");
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM hub_ledger WHERE json_extract(payload, '$.eventId') = 'run:run_1:finished'",
        )
        .fetch_one(db.pool())
        .await
        .expect("event count");
        assert_eq!(events, 1);
    }

    #[tokio::test]
    async fn claimed_active_and_waiting_task_wakes_retry_after_real_db_restart() {
        for (index, status) in [TaskStatus::Active, TaskStatus::WaitingWatch].into_iter().enumerate() {
            let path =
                std::env::temp_dir().join(format!("tinyiothub-wake-restart-{}-{index}.db", uuid::Uuid::new_v4()));
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true);
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_with(options.clone())
                .await
                .expect("file-backed sqlite");
            crate::test_helpers::run_all_migrations(&pool)
                .await
                .expect("migrations");
            seed_workspace(&pool, "ws_1").await;
            let db = Db::new(pool.clone());
            let mut task = sample_task("task_1", "ws_1", "key_1", "inspect");
            task.status = status;
            db.create_task_with_wake(&task, &sample_wake("task_1", "event_original", "ws_1"))
                .await
                .expect("create");
            assert_eq!(db.claim_pending_wakes(10).await.expect("claim before crash").len(), 1);
            pool.close().await;
            drop(db);

            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_with(options)
                .await
                .expect("restart sqlite");
            let db = Db::new(pool.clone());
            db.recover_claimed_wakes().await.expect("release crashed claims");
            let retried = db.claim_pending_wakes(10).await.expect("retry after restart");
            assert_eq!(retried.len(), 1, "status {status:?} must not strand its wake");
            assert_eq!(retried[0].event_id, "event_original");
            assert!(
                db.begin_run(&task, "event_original", "run_1")
                    .await
                    .expect("first dispatch")
            );
            assert!(
                !db.begin_run(&task, "event_original", "run_2")
                    .await
                    .expect("dedup replay")
            );
            pool.close().await;
            let _ = std::fs::remove_file(path);
        }
    }

    #[tokio::test]
    async fn storage_rejects_illegal_waiting_state_transition() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        db.transition_task("ws_1", "task_1", 0, TaskStatus::WaitingWatch, None)
            .await
            .expect("waiting watch");
        let error = db
            .transition_task("ws_1", "task_1", 1, TaskStatus::WaitingJob, None)
            .await
            .expect_err("illegal transition");
        assert!(matches!(error, TaskStoreError::Validation(_)));
        assert_eq!(
            db.get_task("ws_1", "task_1").await.expect("get").expect("task").status,
            TaskStatus::WaitingWatch
        );
    }

    #[tokio::test]
    async fn claiming_leases_pending_wake_and_dead_wake_retry_is_workspace_scoped() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        let claimed = db.claim_pending_wakes(10).await.expect("claim");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].dispatch_state, WakeDispatchState::Pending);
        assert!(claimed[0].dispatched_at.is_some());
        assert!(db.claim_pending_wakes(10).await.expect("second claim").is_empty());
        db.mark_wake("task_1", "event_1", WakeDispatchState::Dead)
            .await
            .expect("mark dead");
        assert!(!db.retry_dead_wake("ws_2", "event_1").await.expect("foreign retry"));
        assert!(db.retry_dead_wake("ws_1", "event_1").await.expect("retry"));
        let retried = db
            .wake_by_id("task_1", "event_1")
            .await
            .expect("wake")
            .expect("present");
        assert_eq!(retried.dispatch_state, WakeDispatchState::Pending);
        assert!(retried.dispatched_at.is_none());
        assert!(retried.resolved_at.is_none());
    }

    #[tokio::test]
    async fn requeue_is_not_clobbered_by_a_late_dispatched_ack() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");

        // Claim -> explicit requeue -> original dispatch's late ack.
        assert_eq!(db.claim_pending_wakes(10).await.expect("claim").len(), 1);
        db.mark_wake("task_1", "event_1", WakeDispatchState::Pending)
            .await
            .expect("requeue");
        db.mark_wake("task_1", "event_1", WakeDispatchState::Dispatched)
            .await
            .expect("late ack");
        let wake = db
            .wake_by_id("task_1", "event_1")
            .await
            .expect("wake")
            .expect("present");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
        assert!(wake.dispatched_at.is_none(), "the late ack must not re-lease the wake");
        assert_eq!(
            db.claim_pending_wakes(10).await.expect("re-claim").len(),
            1,
            "the requeued wake must remain claimable"
        );

        // Ack -> explicit requeue (reverse order) also converges to Pending.
        db.mark_wake("task_1", "event_1", WakeDispatchState::Dispatched)
            .await
            .expect("ack");
        db.mark_wake("task_1", "event_1", WakeDispatchState::Pending)
            .await
            .expect("requeue after ack");
        let wake = db
            .wake_by_id("task_1", "event_1")
            .await
            .expect("wake")
            .expect("present");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
        assert!(wake.dispatched_at.is_none());
        assert!(db.wake_by_id("task_1", "event_1").await.expect("read").is_some());
    }

    #[tokio::test]
    async fn finalize_intent_and_requeue_are_staged_together() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        assert_eq!(db.claim_pending_wakes(10).await.expect("claim").len(), 1);

        let intent = tinyiothub_core::agent_task::TaskFinalizeIntent {
            task_id: "task_1".into(),
            workspace_id: "ws_1".into(),
            wake_event_id: "event_1".into(),
            run_id: "run_1".into(),
            report: sample_report("run_1", "event_1"),
            report_persisted: false,
            report_emitted: false,
            undelivered: vec!["wake_event_id: event_2".into()],
        };
        db.requeue_with_finalize_intent(&intent).await.expect("stage intent");

        let wake = db
            .wake_by_id("task_1", "event_1")
            .await
            .expect("wake")
            .expect("present");
        assert_eq!(wake.dispatch_state, WakeDispatchState::Pending);
        assert!(wake.dispatched_at.is_none());
        assert_eq!(
            db.claim_pending_wakes(10).await.expect("re-claim").len(),
            1,
            "the staged requeue must be claimable"
        );
        let loaded = db
            .finalize_intent("task_1", "event_1")
            .await
            .expect("load intent")
            .expect("intent");
        assert_eq!(loaded.run_id, "run_1");
        assert!(!loaded.report_persisted);
        assert!(!loaded.report_emitted);
        assert_eq!(loaded.undelivered, vec!["wake_event_id: event_2".to_string()]);
        assert_eq!(loaded.report.outcome, tinyiothub_core::agent_runs::Outcome::Acted);

        db.complete_finalize_intent("task_1", "event_1")
            .await
            .expect("complete intent");
        assert!(db.finalize_intent("task_1", "event_1").await.expect("load").is_none());
    }

    #[tokio::test]
    async fn completed_run_report_is_readable_only_after_the_finish_commits() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        let task = sample_task("task_1", "ws_1", "key_1", "inspect");
        db.create_task_with_wake(&task, &sample_wake("task_1", "event_1", "ws_1"))
            .await
            .expect("create");
        assert!(
            db.begin_run(&task, "event_1", "run_1").await.expect("begin"),
            "the run row must be created"
        );
        assert!(
            db.completed_run_report("run_1")
                .await
                .expect("pre-finish read")
                .is_none(),
            "a running row is not a completed report"
        );

        let report = sample_report("run_1", "event_1");
        db.finish_run(&report).await.expect("finish");
        let stored = db
            .completed_run_report("run_1")
            .await
            .expect("completed read")
            .expect("completed report");
        assert_eq!(stored.run_id, "run_1");
        assert_eq!(stored.task_id.as_deref(), Some("task_1"));
        assert_eq!(stored.wake_event_id.as_deref(), Some("event_1"));
        assert_eq!(stored.outcome, tinyiothub_core::agent_runs::Outcome::Acted);
        assert!(
            db.completed_run_report("run_missing")
                .await
                .expect("missing read")
                .is_none(),
            "an unknown run has no completed report"
        );
    }

    #[tokio::test]
    async fn prune_finalize_intents_removes_only_old_settled_rows() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);

        for index in 1..=6 {
            let task_id = format!("task_{index}");
            let event_id = format!("event_{index}");
            let task = sample_task(&task_id, "ws_1", &format!("key_{index}"), "inspect");
            db.create_task_with_wake(&task, &sample_wake(&task_id, &event_id, "ws_1"))
                .await
                .expect("create");
            let mut report = sample_report(&format!("run_{index}"), &event_id);
            report.task_id = Some(task_id.clone());
            let intent = TaskFinalizeIntent {
                task_id,
                workspace_id: "ws_1".into(),
                wake_event_id: event_id,
                run_id: format!("run_{index}"),
                report,
                report_persisted: false,
                report_emitted: false,
                undelivered: vec![],
            };
            db.requeue_with_finalize_intent(&intent).await.expect("stage");
        }

        // 1: old + dead wake + active Task -> retained (retry_dead_wake can
        //    resurrect the wake; the unpersisted report has no other copy).
        db.mark_wake("task_1", "event_1", WakeDispatchState::Dead)
            .await
            .expect("dead wake");
        // 2: old + live wake + active Task -> retained.
        // 3: old + live wake + terminal Task -> retained (the terminal retry
        //    still needs to run its watch cleanup).
        db.transition_task("ws_1", "task_3", 0, TaskStatus::Failed, None)
            .await
            .expect("terminal task");
        // 4: recent + dead wake + active Task -> retained by the age cutoff.
        db.mark_wake("task_4", "event_4", WakeDispatchState::Dead)
            .await
            .expect("dead wake");
        // 5: old + dead wake + terminal Task -> pruned.
        db.mark_wake("task_5", "event_5", WakeDispatchState::Dead)
            .await
            .expect("dead wake");
        db.transition_task("ws_1", "task_5", 0, TaskStatus::Failed, None)
            .await
            .expect("terminal task");
        // 6: old + resolved wake + terminal Task -> pruned.
        db.resolve_task_wakes("task_6").await.expect("resolve wake");
        db.transition_task("ws_1", "task_6", 0, TaskStatus::Failed, None)
            .await
            .expect("terminal task");

        sqlx::query(
            "UPDATE task_finalize_intents SET updated_at = '2000-01-01T00:00:00Z'
             WHERE task_id IN ('task_1', 'task_2', 'task_3', 'task_5', 'task_6')",
        )
        .execute(db.pool())
        .await
        .expect("age intents");

        let cutoff = Utc::now() - chrono::Duration::days(7);
        assert_eq!(db.prune_finalize_intents(cutoff).await.expect("prune"), 2);
        assert_eq!(db.count_finalize_intents().await.expect("count"), 4);
        for (task_id, event_id) in [
            ("task_1", "event_1"),
            ("task_2", "event_2"),
            ("task_3", "event_3"),
            ("task_4", "event_4"),
        ] {
            assert!(
                db.finalize_intent(task_id, event_id).await.expect("load").is_some(),
                "a dead-wake intent on a live Task and any live-wake intent must be retained: {task_id}"
            );
        }
        for (task_id, event_id) in [("task_5", "event_5"), ("task_6", "event_6")] {
            assert!(
                db.finalize_intent(task_id, event_id).await.expect("load").is_none(),
                "an old intent with a dead/resolved wake on a terminal Task must be pruned: {task_id}"
            );
        }
    }

    #[tokio::test]
    async fn pending_finalize_intents_selects_live_wakes_on_nonterminal_tasks() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);

        for index in 1..=4 {
            let task_id = format!("task_{index}");
            let event_id = format!("event_{index}");
            let task = sample_task(&task_id, "ws_1", &format!("key_{index}"), "inspect");
            db.create_task_with_wake(&task, &sample_wake(&task_id, &event_id, "ws_1"))
                .await
                .expect("create");
            let mut report = sample_report(&format!("run_{index}"), &event_id);
            report.task_id = Some(task_id.clone());
            let intent = TaskFinalizeIntent {
                task_id,
                workspace_id: "ws_1".into(),
                wake_event_id: event_id,
                run_id: format!("run_{index}"),
                report,
                report_persisted: false,
                report_emitted: false,
                undelivered: vec![],
            };
            db.requeue_with_finalize_intent(&intent).await.expect("stage");
        }

        // task_2's wake is no longer live; task_3 is terminal.
        db.mark_wake("task_2", "event_2", WakeDispatchState::Dead)
            .await
            .expect("dead wake");
        db.transition_task("ws_1", "task_3", 0, TaskStatus::Failed, None)
            .await
            .expect("terminal task");
        // Deterministic created_at order: task_4 predates task_1.
        sqlx::query("UPDATE task_finalize_intents SET created_at = '2020-01-01T00:00:00Z' WHERE task_id = 'task_4'")
            .execute(db.pool())
            .await
            .expect("age task_4");
        sqlx::query("UPDATE task_finalize_intents SET created_at = '2021-01-01T00:00:00Z' WHERE task_id = 'task_1'")
            .execute(db.pool())
            .await
            .expect("age task_1");

        let pending = db.pending_finalize_intents().await.expect("pending intents");
        let selected = pending
            .iter()
            .map(|intent| (intent.task_id.as_str(), intent.wake_event_id.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            selected,
            [("task_4", "event_4"), ("task_1", "event_1")],
            "only live wakes on nonterminal Tasks, ordered by created_at"
        );
        assert_eq!(pending[0].run_id, "run_4");
        assert!(!pending[0].report_persisted);
    }

    #[tokio::test]
    async fn task_queries_are_scoped_and_unknown_status_is_unavailable() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        seed_workspace(&pool, "ws_2").await;
        let db = Db::new(pool);
        for (id, ws, key) in [("task_1", "ws_1", "key_1"), ("task_2", "ws_2", "key_2")] {
            db.create_task_with_wake(&sample_task(id, ws, key, "inspect"), &sample_wake(id, "event", ws))
                .await
                .expect("create");
        }
        db.transition_task("ws_2", "task_2", 0, TaskStatus::Failed, None)
            .await
            .expect("finish task_2");
        assert_eq!(db.list_tasks("ws_1", 50, 0).await.expect("list").len(), 1);
        assert!(db.get_task("ws_2", "task_1").await.expect("isolation").is_none());
        let active = db.list_nonterminal_tasks(None).await.expect("nonterminal");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "task_1");
        assert!(
            db.list_nonterminal_tasks(Some("ws_2"))
                .await
                .expect("filtered")
                .is_empty()
        );

        sqlx::query("UPDATE agent_tasks SET status = 'mystery' WHERE id = 'task_1'")
            .execute(db.pool())
            .await
            .expect("inject unknown status");
        let error = db.get_task("ws_1", "task_1").await.expect_err("unknown status");
        assert!(matches!(error, TaskStoreError::Unavailable(_)));
    }

    #[tokio::test]
    async fn migrated_schema_has_no_foreign_key_violations() {
        let pool = crate::test_helpers::test_pool().await;
        let violations: Vec<(String,)> = sqlx::query_as("SELECT \"table\" FROM pragma_foreign_key_check")
            .fetch_all(&pool)
            .await
            .expect("foreign key check");
        assert!(violations.is_empty(), "foreign key violations: {violations:?}");
    }
}
