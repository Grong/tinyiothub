use chrono::{DateTime, Utc};
use sqlx::{Sqlite, Transaction};
use tinyiothub_core::agent_task::{AgentTask, CreateTaskOutcome, TaskStoreError, TaskWake};

use crate::Db;
use crate::runtime_event::{RuntimeEventInput, append_runtime_event_tx, task_event};

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
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let task = find_task(&mut tx, workspace_id, &wake.task_id)
            .await?
            .ok_or_else(|| TaskStoreError::Validation("wake task or workspace does not match".to_string()))?;
        if task.workspace_id != workspace_id {
            return Err(TaskStoreError::Validation("wake task workspace differs".to_string()));
        }
        let inserted = sqlx::query(
            "INSERT INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(task_id, event_id) DO NOTHING",
        )
        .bind(&wake.task_id)
        .bind(&wake.event_id)
        .bind(&wake.signal_json)
        .bind(wake.dispatch_state.as_str())
        .bind(wake.created_at)
        .bind(wake.dispatched_at)
        .bind(wake.resolved_at)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        tx.commit().await.map_err(unavailable)?;
        Ok(inserted == 1)
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
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let current = find_task(&mut tx, workspace_id, task_id)
            .await?
            .ok_or(TaskStoreError::ConcurrentUpdate)?;
        if current.state_version != expected_version {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        if !current.status.can_transition_to(next) {
            return Err(TaskStoreError::Validation(format!(
                "task state transition {} -> {} is forbidden",
                current.status.as_str(),
                next.as_str()
            )));
        }
        let affected = sqlx::query(
            "UPDATE agent_tasks
             SET status = ?, state_version = state_version + 1, updated_at = ?, ended_at = ?
             WHERE workspace_id = ? AND id = ? AND state_version = ?",
        )
        .bind(next.as_str())
        .bind(now)
        .bind(ended_at)
        .bind(workspace_id)
        .bind(task_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 0 {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        if let Some(wake) = wake {
            insert_wake(&mut tx, wake).await?;
        }
        let task = find_task(&mut tx, workspace_id, task_id)
            .await?
            .ok_or_else(|| TaskStoreError::Unavailable("updated task disappeared".to_string()))?;
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
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
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
        if inserted == 1 {
            let event = append_runtime_event_tx(
                &mut tx,
                RuntimeEventInput {
                    event_id: format!("run:{run_id}:started"),
                    event_type: "run.started".into(),
                    occurred_at: Utc::now(),
                    workspace_id: task.workspace_id.clone(),
                    task_id: Some(task.id.clone()),
                    run_id: Some(run_id.to_string()),
                    tool_call_id: None,
                    job_id: None,
                    approval_id: None,
                    device_id: None,
                    source_id: None,
                    source_seq: None,
                    evidence_id: None,
                    data: serde_json::json!({}),
                },
            )
            .await
            .map_err(unavailable)?;
            tx.commit().await.map_err(unavailable)?;
            return Ok((true, Some(event)));
        }
        let duplicate: Option<(String,)> =
            sqlx::query_as("SELECT id FROM agent_runs WHERE task_id = ? AND wake_event_id = ?")
                .bind(&task.id)
                .bind(wake_event_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(unavailable)?;
        if duplicate.is_some() {
            tx.commit().await.map_err(unavailable)?;
            Ok((false, None))
        } else {
            Err(TaskStoreError::Unavailable(
                "agent run conflict was not a task/wake replay".to_string(),
            ))
        }
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
             WHERE id = ? AND workspace_id = ?",
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
        sqlx::query_as::<_, TaskWakeRow>(
            "UPDATE task_wake_outbox
             SET dispatched_at = ?
             WHERE (task_id, event_id) IN (
                 SELECT task_id, event_id FROM task_wake_outbox
                 WHERE dispatch_state = 'pending' AND dispatched_at IS NULL AND resolved_at IS NULL
                 ORDER BY created_at, task_id, event_id LIMIT ?
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
        .collect()
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
        sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = ?,
                 dispatched_at = CASE ? WHEN 'dispatched' THEN ? WHEN 'pending' THEN NULL ELSE dispatched_at END,
                 resolved_at = CASE ? WHEN 'dead' THEN ? WHEN 'pending' THEN NULL ELSE resolved_at END
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL",
        )
        .bind(state)
        .bind(state)
        .bind(now)
        .bind(state)
        .bind(now)
        .bind(task_id)
        .bind(event_id)
        .execute(self.pool())
        .await
        .map_err(unavailable)?;
        Ok(())
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

        assert!(db.begin_run(&task, "event_2", "run_2").await.expect("next run"));
        let latest = db.latest_task_run("task_1").await.expect("latest completed report");
        assert_eq!(latest.expect("completed report").run_id, "run_1");
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
