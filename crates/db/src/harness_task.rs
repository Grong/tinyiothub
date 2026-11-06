//! Workspace-Harness-backed Task/Run persistence for the Runtime `TaskStore`
//! port (Task 19A).
//!
//! Every fact lives in the owning workspace's Harness SQLite generation. All
//! SQL stays in this crate; the adapter in `apps/cloud` only translates between
//! the port types and these methods.
//!
//! Port methods without a workspace argument (wake acknowledgements, retained
//! finalize intents, cross-workspace recovery) enumerate the registered
//! workspaces whose Harness store already exists. A workspace without a store
//! is skipped so an un-migrated workspace never gets an empty generation
//! created as a side effect of recovery.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, Transaction};
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::{
    AgentTask, CreateTaskOutcome, TaskFinalizeIntent, TaskStatus, TaskStoreError, TaskWake, WakeDispatchState,
};
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};

use crate::Db;

const TASK_COLUMNS: &str = "id, run_id, state, state_version,
                            runtime_idempotency_key AS idempotency_key,
                            runtime_objective AS objective,
                            runtime_session_key AS session_key,
                            runtime_created_by AS created_by,
                            runtime_created_at AS created_at,
                            runtime_updated_at AS updated_at,
                            runtime_ended_at AS ended_at";

const TASK_SELECT: &str = "SELECT id, run_id, state, state_version,
            runtime_idempotency_key AS idempotency_key,
            runtime_objective AS objective,
            runtime_session_key AS session_key,
            runtime_created_by AS created_by,
            runtime_created_at AS created_at,
            runtime_updated_at AS updated_at,
            runtime_ended_at AS ended_at";

#[derive(Debug, sqlx::FromRow)]
struct AgentTaskRow {
    id: String,
    run_id: Option<String>,
    state: String,
    state_version: i64,
    idempotency_key: String,
    objective: String,
    session_key: Option<String>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl AgentTaskRow {
    fn into_task(self, workspace_id: &str) -> Result<AgentTask, TaskStoreError> {
        Ok(AgentTask {
            id: self.id,
            workspace_id: workspace_id.to_string(),
            idempotency_key: self.idempotency_key,
            objective: self.objective,
            session_key: self.session_key,
            created_by: self.created_by,
            status: status_from_state(&self.state)
                .ok_or_else(|| TaskStoreError::Unavailable(format!("unknown task state: {}", self.state)))?,
            current_run_id: self.run_id,
            state_version: self.state_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ended_at: self.ended_at,
        })
    }
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

impl TaskWakeRow {
    fn into_wake(self) -> Result<TaskWake, TaskStoreError> {
        Ok(TaskWake {
            task_id: self.task_id,
            event_id: self.event_id,
            signal_json: self.signal_json,
            dispatch_state: WakeDispatchState::from_db(&self.dispatch_state).ok_or_else(|| {
                TaskStoreError::Unavailable(format!("unknown wake dispatch state: {}", self.dispatch_state))
            })?,
            created_at: self.created_at,
            dispatched_at: self.dispatched_at,
            resolved_at: self.resolved_at,
        })
    }
}

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
    created_at: DateTime<Utc>,
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

fn unavailable(error: impl std::fmt::Display) -> TaskStoreError {
    TaskStoreError::Unavailable(error.to_string())
}

/// Result of a cross-workspace point lookup. `skipped` records stores that
/// could not be read; point lookups must not treat that as "absent" (N1).
struct WorkspaceLookup {
    matched: Option<String>,
    skipped: usize,
}

impl WorkspaceLookup {
    /// Read-only lookups may return a unique readable match even when another
    /// store was skipped; mutations and absent results must fail closed.
    fn resolve(self, subject: &str, mutating: bool) -> Result<Option<String>, TaskStoreError> {
        if self.skipped > 0 && (self.matched.is_none() || mutating) {
            return Err(TaskStoreError::Unavailable(format!(
                "{subject}: {} workspace store(s) unreadable; refusing cross-workspace lookup",
                self.skipped
            )));
        }
        Ok(self.matched)
    }
}

/// SQLite extended constraint codes for PRIMARY KEY (1555) and UNIQUE (2067),
/// plus the bare CONSTRAINT code (19) used by some bundled SQLite versions.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(database) => {
            matches!(database.code().as_deref(), Some("1555" | "2067" | "19"))
                || database.message().contains("UNIQUE constraint failed")
        }
        _ => false,
    }
}

/// Harness projection state `<->` port status. The Runtime vocabulary calls
/// the working state `active`, while Harness projections use `running`; an
/// interrupted projection still belongs to an active Task and is resumed by
/// the coordinator.
pub(crate) fn status_from_state(state: &str) -> Option<TaskStatus> {
    match state {
        "pending" | "running" | "model_calling" | "interrupted" => Some(TaskStatus::Active),
        other => TaskStatus::from_db(other),
    }
}

fn state_for_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Active => "running",
        other => other.as_str(),
    }
}

/// Every persisted Harness state that may legally precede `next`, derived from
/// the core `TaskStatus` transition matrix (F6). `Active` expands to all
/// Harness working projections so a crash-interrupted Task can still move.
fn predecessor_states(next: TaskStatus) -> Vec<&'static str> {
    const ALL: [TaskStatus; 9] = [
        TaskStatus::Active,
        TaskStatus::WaitingJob,
        TaskStatus::WaitingWatch,
        TaskStatus::NeedsApproval,
        TaskStatus::Paused,
        TaskStatus::Cancelling,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ];
    let mut states = Vec::new();
    for status in ALL {
        if !status.can_transition_to(next) {
            continue;
        }
        match status {
            TaskStatus::Active => {
                states.extend(["running", "pending", "model_calling", "interrupted"]);
            }
            other => states.push(other.as_str()),
        }
    }
    states
}

fn nonterminal_state_placeholders() -> String {
    // Terminal states are fixed; a literal keeps the query readable and the
    // planner able to use the state/activity index.
    format!(
        "state NOT IN ('{}', '{}', '{}')",
        TaskStatus::Completed.as_str(),
        TaskStatus::Failed.as_str(),
        TaskStatus::Cancelled.as_str()
    )
}

fn wake_signal_workspace(wake: &TaskWake) -> Result<String, TaskStoreError> {
    let signal: Value = serde_json::from_str(&wake.signal_json)
        .map_err(|error| TaskStoreError::Validation(format!("invalid wake signal: {error}")))?;
    signal
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| TaskStoreError::Validation("wake signal has no workspace_id".to_string()))
}

fn validate_wake(workspace_id: &str, task_id: &str, wake: &TaskWake) -> Result<(), TaskStoreError> {
    if wake_signal_workspace(wake)? != workspace_id || wake.task_id != task_id {
        return Err(TaskStoreError::Validation(
            "task and wake workspace or task id differ".to_string(),
        ));
    }
    Ok(())
}

fn event_input(
    workspace_id: &str,
    session_id: String,
    event_id: String,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    task_id: Option<String>,
    run_id: Option<String>,
    payload: Value,
) -> Result<HarnessEventInput, TaskStoreError> {
    let payload_json = serde_json::to_string(&payload).map_err(unavailable)?;
    Ok(HarnessEventInput {
        event_id,
        workspace_id: workspace_id.to_string(),
        session_id,
        execution_mode: ExecutionMode::Production,
        run_id,
        task_id,
        event_type: event_type.to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", Sha256::digest(payload_json.as_bytes())),
        payload_json: payload,
    })
}

fn task_event(
    workspace_id: &str,
    task: &AgentTask,
    event_type: &str,
    event_id: String,
    occurred_at: DateTime<Utc>,
) -> Result<HarnessEventInput, TaskStoreError> {
    event_input(
        workspace_id,
        format!("task:{}", task.id),
        event_id,
        event_type,
        occurred_at,
        Some(task.id.clone()),
        task.current_run_id.clone(),
        json!({"stateVersion": task.state_version}),
    )
}

async fn write_event(tx: &mut Transaction<'_, Sqlite>, event: &HarnessEventInput) -> Result<(), TaskStoreError> {
    crate::harness_effect::insert_event(&mut *tx, event)
        .await
        .map_err(unavailable)
}

/// A deduplicated create must never rewrite the original `task.created`
/// event. Its payload carries the create-time state version, so a replay after
/// the Task advanced would otherwise look like a digest conflict (O2).
async fn write_created_event_if_missing(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    task: &AgentTask,
) -> Result<(), TaskStoreError> {
    let event_id = format!("task:{}:created", task.id);
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_events WHERE event_id = ?")
        .bind(&event_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(unavailable)?;
    if exists > 0 {
        return Ok(());
    }
    let event = task_event(workspace_id, task, "task.created", event_id, Utc::now())?;
    write_event(tx, &event).await
}

fn run_report_json(report: &RunReport) -> Result<String, TaskStoreError> {
    let mut report_json = serde_json::to_value(report).map_err(unavailable)?;
    report_json["action_count"] = json!(report.actions.len());
    serde_json::to_string(&report_json).map_err(unavailable)
}

async fn upsert_finalize_intent(
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

impl Db {
    /// Registered workspaces that already own a Harness generation. Recovery
    /// must not create empty stores for workspaces that have not migrated.
    pub(crate) async fn harness_workspaces(&self) -> Result<Vec<String>, TaskStoreError> {
        let ids = self.find_all_workspace_ids().await.map_err(unavailable)?;
        let mut workspaces = Vec::new();
        for workspace_id in ids {
            let path = self
                .workspace_store_root
                .join(&workspace_id)
                .join("harness")
                .join("harness.db");
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                workspaces.push(workspace_id);
                continue;
            }
            match self.restore_journal_row(&workspace_id).await {
                Ok(Some(_)) => workspaces.push(workspace_id),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "workspace restore journal unreadable; skipping during store sweep"
                    );
                }
            }
        }
        Ok(workspaces)
    }

    /// Open a workspace store for a cross-workspace scan. One unreadable
    /// workspace must not fail a runtime-wide sweep (F4).
    pub(crate) async fn try_workspace_pool(&self, workspace_id: &str) -> Option<sqlx::SqlitePool> {
        match self.workspace_pool(workspace_id).await {
            Ok(pool) => Some(pool),
            Err(error) => {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    %error,
                    "workspace harness store unreadable; skipping during store sweep"
                );
                None
            }
        }
    }

    /// Resolve exactly one workspace for a point lookup. Multiple matching
    /// workspaces are an ambiguity error rather than a silent first match
    /// (F3).
    async fn unique_task_workspace(&self, task_id: &str) -> Result<WorkspaceLookup, TaskStoreError> {
        let mut matched = None;
        let mut skipped = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                skipped += 1;
                continue;
            };
            let found = match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_tasks WHERE id = ? AND execution_mode = 'production'",
            )
            .bind(task_id)
            .fetch_one(&pool)
            .await
            {
                Ok(found) => found,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "task lookup failed; skipping workspace"
                    );
                    skipped += 1;
                    continue;
                }
            };
            if found > 0 {
                if matched.is_some() {
                    return Err(TaskStoreError::Unavailable(format!(
                        "task {task_id} exists in more than one workspace"
                    )));
                }
                matched = Some(workspace_id);
            }
        }
        Ok(WorkspaceLookup { matched, skipped })
    }

    async fn unique_wake_workspace(&self, task_id: &str, event_id: &str) -> Result<WorkspaceLookup, TaskStoreError> {
        let mut matched = None;
        let mut skipped = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                skipped += 1;
                continue;
            };
            let found = match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
            )
            .bind(task_id)
            .bind(event_id)
            .fetch_one(&pool)
            .await
            {
                Ok(found) => found,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "wake lookup failed; skipping workspace"
                    );
                    skipped += 1;
                    continue;
                }
            };
            if found > 0 {
                if matched.is_some() {
                    return Err(TaskStoreError::Unavailable(format!(
                        "wake {task_id}/{event_id} exists in more than one workspace"
                    )));
                }
                matched = Some(workspace_id);
            }
        }
        Ok(WorkspaceLookup { matched, skipped })
    }

    async fn unique_run_workspace(&self, run_id: &str) -> Result<WorkspaceLookup, TaskStoreError> {
        let mut matched = None;
        let mut skipped = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                skipped += 1;
                continue;
            };
            let found = match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_runs WHERE id = ? AND execution_mode = 'production'",
            )
            .bind(run_id)
            .fetch_one(&pool)
            .await
            {
                Ok(found) => found,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "run lookup failed; skipping workspace"
                    );
                    skipped += 1;
                    continue;
                }
            };
            if found > 0 {
                if matched.is_some() {
                    return Err(TaskStoreError::Unavailable(format!(
                        "run {run_id} exists in more than one workspace"
                    )));
                }
                matched = Some(workspace_id);
            }
        }
        Ok(WorkspaceLookup { matched, skipped })
    }

    pub async fn harness_create_task_with_wake(
        &self,
        task: &AgentTask,
        wake: &TaskWake,
    ) -> Result<CreateTaskOutcome, TaskStoreError> {
        let signal_workspace = wake_signal_workspace(wake)?;
        if task.workspace_id != signal_workspace {
            return Err(TaskStoreError::Validation("task and wake workspace differ".to_string()));
        }
        if wake.task_id != task.id {
            return Err(TaskStoreError::Validation("task and wake task id differ".to_string()));
        }
        if task.idempotency_key.trim().is_empty() {
            return Err(TaskStoreError::Validation(
                "idempotency key must not be blank".to_string(),
            ));
        }
        let pool = self.workspace_pool(&task.workspace_id).await.map_err(unavailable)?;
        let mut tx = pool.begin().await.map_err(unavailable)?;
        // Scope conflict handling to the production idempotency index only. A
        // primary-key collision on `id` is not swallowed; it is classified
        // below as same-id/different-key (F2).
        let insert_result = sqlx::query(
            "INSERT INTO agent_tasks
                 (id, run_id, state, state_version, execution_mode, runtime_idempotency_key,
                  runtime_objective, runtime_session_key, runtime_created_by,
                  runtime_created_at, runtime_updated_at, runtime_ended_at)
             VALUES (?, ?, ?, ?, 'production', ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(runtime_idempotency_key)
                 WHERE execution_mode = 'production' AND runtime_idempotency_key <> ''
             DO NOTHING",
        )
        .bind(&task.id)
        .bind(&task.current_run_id)
        .bind(state_for_status(task.status))
        .bind(task.state_version)
        .bind(&task.idempotency_key)
        .bind(&task.objective)
        .bind(&task.session_key)
        .bind(&task.created_by)
        .bind(task.created_at)
        .bind(task.updated_at)
        .bind(task.ended_at)
        .execute(&mut *tx)
        .await;

        let inserted = match insert_result {
            Ok(done) => done.rows_affected(),
            Err(error) if is_unique_violation(&error) => {
                let original = self.create_conflict_original(&mut tx, task).await?;
                write_created_event_if_missing(&mut tx, &task.workspace_id, &original).await?;
                tx.commit().await.map_err(unavailable)?;
                return Ok(CreateTaskOutcome {
                    task: original,
                    deduplicated: true,
                });
            }
            Err(error) => return Err(unavailable(error)),
        };

        if inserted == 0 {
            let original = self.create_conflict_original(&mut tx, task).await?;
            write_created_event_if_missing(&mut tx, &task.workspace_id, &original).await?;
            tx.commit().await.map_err(unavailable)?;
            return Ok(CreateTaskOutcome {
                task: original,
                deduplicated: true,
            });
        }

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
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
        write_created_event_if_missing(&mut tx, &task.workspace_id, task).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(CreateTaskOutcome {
            task: task.clone(),
            deduplicated: false,
        })
    }

    /// Classify a create conflict without ever returning a different Task.
    /// Priority: same id (must be the same key and objective), then the
    /// idempotency key.
    async fn create_conflict_original(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        task: &AgentTask,
    ) -> Result<AgentTask, TaskStoreError> {
        let by_id = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
            "{TASK_SELECT} FROM agent_tasks WHERE id = ? AND execution_mode = 'production'"
        )))
        .bind(&task.id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
        if let Some(row) = by_id {
            let original = row.into_task(&task.workspace_id)?;
            if original.idempotency_key != task.idempotency_key {
                return Err(TaskStoreError::Unavailable(format!(
                    "task id {} already exists with another idempotency key",
                    task.id
                )));
            }
            if original.objective != task.objective {
                return Err(TaskStoreError::ObjectiveConflict);
            }
            return Ok(original);
        }
        let by_key = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
            "{TASK_SELECT} FROM agent_tasks
             WHERE runtime_idempotency_key = ? AND execution_mode = 'production'"
        )))
        .bind(&task.idempotency_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| TaskStoreError::Unavailable("idempotency conflict without original task".into()))?
        .into_task(&task.workspace_id)?;
        if by_key.objective != task.objective {
            return Err(TaskStoreError::ObjectiveConflict);
        }
        Ok(by_key)
    }

    pub async fn harness_get_task(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<Option<AgentTask>, TaskStoreError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(unavailable)?;
        sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
            "{TASK_SELECT} FROM agent_tasks WHERE id = ? AND execution_mode = 'production'"
        )))
        .bind(task_id)
        .fetch_optional(&pool)
        .await
        .map_err(unavailable)?
        .map(|row| row.into_task(workspace_id))
        .transpose()
    }

    pub async fn harness_list_tasks(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentTask>, TaskStoreError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(unavailable)?;
        let rows = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
            "{TASK_SELECT} FROM agent_tasks
             WHERE execution_mode = 'production'
             ORDER BY runtime_created_at DESC, rowid DESC LIMIT ? OFFSET ?"
        )))
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(|row| row.into_task(workspace_id)).collect()
    }

    pub async fn harness_wake_by_id(&self, task_id: &str, event_id: &str) -> Result<Option<TaskWake>, TaskStoreError> {
        let Some(workspace_id) = self
            .unique_wake_workspace(task_id, event_id)
            .await?
            .resolve(&format!("wake {task_id}/{event_id}"), false)?
        else {
            return Ok(None);
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        sqlx::query_as::<_, TaskWakeRow>(
            "SELECT task_id, event_id, signal_json, dispatch_state, created_at,
                    dispatched_at, resolved_at
             FROM task_wake_outbox WHERE task_id = ? AND event_id = ?",
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(&pool)
        .await
        .map_err(unavailable)?
        .map(TaskWakeRow::into_wake)
        .transpose()
    }

    pub async fn harness_record_wake(&self, wake: &TaskWake) -> Result<bool, TaskStoreError> {
        let workspace_id = wake_signal_workspace(wake)?;
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             SELECT ?, ?, ?, ?, ?, ?, ?
             WHERE EXISTS (
                 SELECT 1 FROM agent_tasks WHERE id = ? AND execution_mode = 'production'
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
        .execute(&pool)
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
                .fetch_one(&pool)
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

    pub async fn harness_resolve_task_wakes(&self, task_id: &str) -> Result<usize, TaskStoreError> {
        let Some(workspace_id) = self
            .unique_task_workspace(task_id)
            .await?
            .resolve(&format!("task {task_id}"), true)?
        else {
            return Ok(0);
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        let affected = sqlx::query(
            "UPDATE task_wake_outbox SET resolved_at = ?
             WHERE task_id = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now())
        .bind(task_id)
        .execute(&pool)
        .await
        .map_err(unavailable)?
        .rows_affected();
        Ok(affected as usize)
    }

    pub async fn harness_interrupt_unfinished_runs(&self, known_active: &[String]) -> Result<usize, TaskStoreError> {
        let mut marked = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            let mut statement = "SELECT r.id, r.task_id FROM agent_runs r
                 WHERE r.state IN ('running', 'model_calling')
                   AND r.execution_mode = 'production'"
                .to_string();
            if !known_active.is_empty() {
                let placeholders = vec!["?"; known_active.len()].join(", ");
                statement.push_str(&format!(" AND r.id NOT IN ({placeholders})"));
            }
            statement.push_str(
                " AND NOT EXISTS (
                     SELECT 1 FROM task_finalize_intents i
                     JOIN task_wake_outbox w
                       ON w.task_id = i.task_id AND w.event_id = i.wake_event_id
                     JOIN agent_tasks t ON t.id = i.task_id
                     WHERE i.run_id = r.id AND w.resolved_at IS NULL
                       AND t.state NOT IN ('completed', 'failed', 'cancelled')
                 )
                 ORDER BY r.id",
            );
            let mut query = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(statement));
            for run_id in known_active {
                query = query.bind(run_id);
            }
            let runs = match query.fetch_all(&pool).await {
                Ok(runs) => runs,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "interrupted-run scan failed; skipping workspace"
                    );
                    continue;
                }
            };
            for (run_id, task_id) in runs {
                let now = Utc::now();
                let mut tx = pool.begin().await.map_err(unavailable)?;
                let updated = sqlx::query(
                    "UPDATE agent_runs SET state = 'interrupted'
                     WHERE id = ? AND task_id = ? AND state IN ('running', 'model_calling')",
                )
                .bind(&run_id)
                .bind(&task_id)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?
                .rows_affected();
                if updated == 0 {
                    tx.commit().await.map_err(unavailable)?;
                    continue;
                }
                let event = event_input(
                    &workspace_id,
                    format!("task:{task_id}"),
                    format!("run:{run_id}:interrupted"),
                    "run.interrupted",
                    now,
                    Some(task_id.clone()),
                    Some(run_id.clone()),
                    json!({"state": "interrupted"}),
                )?;
                write_event(&mut tx, &event).await?;
                tx.commit().await.map_err(unavailable)?;
                marked += 1;
            }
        }
        Ok(marked)
    }

    pub async fn harness_transition_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        next: TaskStatus,
        wake: Option<&TaskWake>,
    ) -> Result<AgentTask, TaskStoreError> {
        if let Some(wake) = wake {
            validate_wake(workspace_id, task_id, wake)?;
        }
        let predecessors = predecessor_states(next);
        if predecessors.is_empty() {
            return Err(TaskStoreError::Validation(format!(
                "task state {} has no legal predecessor",
                state_for_status(next)
            )));
        }
        let pool = self.workspace_pool(workspace_id).await.map_err(unavailable)?;
        let now = Utc::now();
        let ended_at = next.is_terminal().then_some(now);
        let mut tx = pool.begin().await.map_err(unavailable)?;
        // The conditioned UPDATE is deliberately the first statement so SQLite
        // takes the write lock before any read, matching the legacy store and
        // avoiding a deferred SHARED -> RESERVED upgrade failure under
        // contention (F1).
        let placeholders = vec!["?"; predecessors.len()].join(", ");
        let statement = format!(
            "UPDATE agent_tasks
             SET state = ?, state_version = state_version + 1,
                 runtime_updated_at = ?, runtime_ended_at = ?
             WHERE id = ? AND state_version = ? AND execution_mode = 'production'
               AND state IN ({placeholders})
             RETURNING {TASK_COLUMNS}"
        );
        let mut query = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(statement))
            .bind(state_for_status(next))
            .bind(now)
            .bind(ended_at)
            .bind(task_id)
            .bind(expected_version);
        for predecessor in &predecessors {
            query = query.bind(*predecessor);
        }
        let updated = query.fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let Some(updated) = updated else {
            let current = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
                "{TASK_SELECT} FROM agent_tasks WHERE id = ? AND execution_mode = 'production'"
            )))
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(unavailable)?;
            return match current {
                None => Err(TaskStoreError::ConcurrentUpdate),
                Some(row) => {
                    let current = row.into_task(workspace_id)?;
                    if current.state_version != expected_version {
                        Err(TaskStoreError::ConcurrentUpdate)
                    } else {
                        Err(TaskStoreError::Validation(format!(
                            "task state transition {} -> {} is forbidden",
                            current.status.as_str(),
                            next.as_str()
                        )))
                    }
                }
            };
        };
        let updated = updated.into_task(workspace_id)?;
        if let Some(wake) = wake {
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
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        }
        let event = task_event(
            workspace_id,
            &updated,
            &format!("task.{}", updated.status.as_str()),
            format!(
                "task:{}:v{}:{}",
                updated.id,
                updated.state_version,
                updated.status.as_str()
            ),
            now,
        )?;
        write_event(&mut tx, &event).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(updated)
    }

    pub async fn harness_bind_current_run(
        &self,
        task_id: &str,
        expected_version: i64,
        run_id: Option<&str>,
    ) -> Result<AgentTask, TaskStoreError> {
        let workspace_id = self
            .unique_task_workspace(task_id)
            .await?
            .resolve(&format!("task {task_id}"), true)?
            .ok_or(TaskStoreError::ConcurrentUpdate)?;
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        let now = Utc::now();
        let mut tx = pool.begin().await.map_err(unavailable)?;
        let updated = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE agent_tasks
             SET run_id = ?, state_version = state_version + 1, runtime_updated_at = ?
             WHERE id = ? AND state_version = ? AND execution_mode = 'production'
             RETURNING {TASK_COLUMNS}"
        )))
        .bind(run_id)
        .bind(now)
        .bind(task_id)
        .bind(expected_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        .ok_or(TaskStoreError::ConcurrentUpdate)?
        .into_task(&workspace_id)?;
        let event = task_event(
            &workspace_id,
            &updated,
            "task.run_bound",
            format!("task:{}:v{}:run_bound", updated.id, updated.state_version),
            now,
        )?;
        write_event(&mut tx, &event).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(updated)
    }

    pub async fn harness_begin_run(
        &self,
        task: &AgentTask,
        wake_event_id: &str,
        run_id: &str,
    ) -> Result<bool, TaskStoreError> {
        if !task.status.can_transition_to(TaskStatus::Active)
            || matches!(task.status, TaskStatus::Paused | TaskStatus::Cancelling)
        {
            return Err(TaskStoreError::Validation(format!(
                "task state transition {} -> active is forbidden",
                task.status.as_str()
            )));
        }
        let pool = self.workspace_pool(&task.workspace_id).await.map_err(unavailable)?;
        let now = Utc::now();
        let mut tx = pool.begin().await.map_err(unavailable)?;
        let inserted = sqlx::query(
            "INSERT INTO agent_runs
                 (id, task_id, state, execution_mode, runtime_wake_event_id,
                  runtime_created_at, runtime_updated_at)
             VALUES (?, ?, 'running', 'production', ?, ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(run_id)
        .bind(&task.id)
        .bind(wake_event_id)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if inserted == 0 {
            let duplicate = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_runs WHERE task_id = ? AND runtime_wake_event_id = ?",
            )
            .bind(&task.id)
            .bind(wake_event_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(unavailable)?;
            return if duplicate > 0 {
                tx.commit().await.map_err(unavailable)?;
                Ok(false)
            } else {
                Err(TaskStoreError::Unavailable(
                    "agent run conflict was not a task/wake replay".to_string(),
                ))
            };
        }
        // The working-state predecessor set comes from the same TaskStatus
        // matrix as transition_task (F6), not a second hardcoded list.
        let predecessors = predecessor_states(TaskStatus::Active);
        let placeholders = vec!["?"; predecessors.len()].join(", ");
        let statement = format!(
            "UPDATE agent_tasks
             SET state = 'running', run_id = ?, state_version = state_version + 1, runtime_updated_at = ?
             WHERE id = ? AND state_version = ? AND execution_mode = 'production'
               AND state IN ({placeholders})
             RETURNING {TASK_COLUMNS}"
        );
        let mut query = sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(statement))
            .bind(run_id)
            .bind(now)
            .bind(&task.id)
            .bind(task.state_version);
        for predecessor in &predecessors {
            query = query.bind(*predecessor);
        }
        let updated = query
            .fetch_optional(&mut *tx)
            .await
            .map_err(unavailable)?
            .ok_or(TaskStoreError::ConcurrentUpdate)?
            .into_task(&task.workspace_id)?;
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
        let event = event_input(
            &task.workspace_id,
            format!("task:{}/run:{run_id}", task.id),
            format!("run:{run_id}:started"),
            "run.started",
            now,
            Some(task.id.clone()),
            Some(run_id.to_string()),
            json!({"taskStatus": updated.status.as_str()}),
        )?;
        write_event(&mut tx, &event).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(true)
    }

    pub async fn harness_finish_run(&self, report: &RunReport) -> Result<(), TaskStoreError> {
        let pool = self.workspace_pool(&report.workspace_id).await.map_err(unavailable)?;
        let report_json = run_report_json(report)?;
        let now = Utc::now();
        let mut tx = pool.begin().await.map_err(unavailable)?;
        let affected = sqlx::query(
            "UPDATE agent_runs
             SET runtime_outcome = ?, runtime_summary = ?, runtime_report_json = ?,
                 runtime_verified = ?, runtime_tool_calls = ?, runtime_tokens = ?,
                 runtime_duration_ms = ?, state = 'completed', runtime_updated_at = ?
             WHERE id = ? AND state = 'running'",
        )
        .bind(report.outcome.as_str())
        .bind(&report.summary)
        .bind(&report_json)
        .bind(report.verified)
        .bind(i64::from(report.tool_calls))
        .bind(report.tokens as i64)
        .bind(report.duration_ms as i64)
        .bind(now)
        .bind(&report.run_id)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?
        .rows_affected();
        if affected == 0 {
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        let event = event_input(
            &report.workspace_id,
            format!(
                "task:{}/run:{}",
                report.task_id.clone().unwrap_or_default(),
                report.run_id
            ),
            format!("run:{}:finished", report.run_id),
            "run.finished",
            now,
            report.task_id.clone(),
            Some(report.run_id.clone()),
            json!({"outcome": report.outcome.as_str()}),
        )?;
        write_event(&mut tx, &event).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    async fn completed_run_in(&self, workspace_id: &str, run_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(unavailable)?;
        let report: Option<(String,)> = sqlx::query_as(
            "SELECT runtime_report_json FROM agent_runs
             WHERE id = ? AND state = 'completed' AND runtime_report_json IS NOT NULL",
        )
        .bind(run_id)
        .fetch_optional(&pool)
        .await
        .map_err(unavailable)?;
        report
            .map(|(json,)| serde_json::from_str(&json).map_err(unavailable))
            .transpose()
    }

    pub async fn harness_latest_task_run(&self, task_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        let Some(workspace_id) = self
            .unique_task_workspace(task_id)
            .await?
            .resolve(&format!("task {task_id}"), false)?
        else {
            return Ok(None);
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        let report: Option<(String,)> = sqlx::query_as(
            "SELECT runtime_report_json FROM agent_runs
             WHERE task_id = ? AND state = 'completed' AND runtime_report_json IS NOT NULL
             ORDER BY runtime_created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(&pool)
        .await
        .map_err(unavailable)?;
        report
            .map(|(json,)| serde_json::from_str(&json).map_err(unavailable))
            .transpose()
    }

    pub async fn harness_claim_pending_wakes(&self, limit: i64) -> Result<Vec<TaskWake>, TaskStoreError> {
        let limit = limit.max(0);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut candidates: Vec<(String, TaskWake)> = Vec::new();
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            let rows = match sqlx::query_as::<_, TaskWakeRow>(
                "SELECT o.task_id, o.event_id, o.signal_json, o.dispatch_state, o.created_at,
                        o.dispatched_at, o.resolved_at
                 FROM task_wake_outbox o
                 JOIN agent_tasks t ON t.id = o.task_id
                 WHERE o.dispatch_state = 'pending' AND o.dispatched_at IS NULL
                   AND o.resolved_at IS NULL AND t.state != 'paused'
                 ORDER BY o.created_at, o.task_id, o.event_id LIMIT ?",
            )
            .bind(limit)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "wake claim scan failed; skipping workspace"
                    );
                    continue;
                }
            };
            for row in rows {
                candidates.push((workspace_id.clone(), row.into_wake()?));
            }
        }
        candidates.sort_by(|left, right| {
            left.1
                .created_at
                .cmp(&right.1.created_at)
                .then_with(|| left.1.task_id.cmp(&right.1.task_id))
                .then_with(|| left.1.event_id.cmp(&right.1.event_id))
        });
        candidates.truncate(limit as usize);

        // Claim each workspace's selected rows in one conditioned statement,
        // so selection and claim are atomic within that store (F5).
        let mut groups: std::collections::HashMap<String, Vec<TaskWake>> = std::collections::HashMap::new();
        for (workspace_id, wake) in candidates {
            groups.entry(workspace_id).or_default().push(wake);
        }
        let now = Utc::now();
        let mut claimed = Vec::new();
        for (workspace_id, selected) in groups {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            let placeholders = vec!["(?, ?)"; selected.len()].join(", ");
            let statement = format!(
                "UPDATE task_wake_outbox SET dispatched_at = ?
                 WHERE (task_id, event_id) IN ({placeholders})
                   AND dispatch_state = 'pending' AND dispatched_at IS NULL
                   AND resolved_at IS NULL
                 RETURNING task_id, event_id, signal_json, dispatch_state, created_at,
                           dispatched_at, resolved_at"
            );
            let mut query = sqlx::query_as::<_, TaskWakeRow>(sqlx::AssertSqlSafe(statement)).bind(now);
            for wake in &selected {
                query = query.bind(&wake.task_id).bind(&wake.event_id);
            }
            match query.fetch_all(&pool).await {
                Ok(rows) => {
                    for row in rows {
                        claimed.push(row.into_wake()?);
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "wake claim update failed; skipping workspace"
                    );
                }
            }
        }
        claimed.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.task_id.cmp(&right.task_id))
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        Ok(claimed)
    }

    pub async fn harness_recover_claimed_wakes(&self) -> Result<usize, TaskStoreError> {
        let mut affected = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            match sqlx::query(
                "UPDATE task_wake_outbox SET dispatched_at = NULL
                 WHERE dispatch_state = 'pending' AND dispatched_at IS NOT NULL",
            )
            .execute(&pool)
            .await
            {
                Ok(done) => affected += done.rows_affected(),
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "wake recovery failed; skipping workspace"
                    );
                }
            }
        }
        Ok(affected as usize)
    }

    pub async fn harness_mark_wake(
        &self,
        task_id: &str,
        event_id: &str,
        state: WakeDispatchState,
    ) -> Result<(), TaskStoreError> {
        let Some(workspace_id) = self
            .unique_wake_workspace(task_id, event_id)
            .await?
            .resolve(&format!("wake {task_id}/{event_id}"), true)?
        else {
            return Ok(());
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        let state = state.as_str();
        let now = Utc::now();
        sqlx::query(
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
        .execute(&pool)
        .await
        .map_err(unavailable)?;
        Ok(())
    }

    pub async fn harness_mark_wake_dead(
        &self,
        task_id: &str,
        event_id: &str,
        reason: &str,
    ) -> Result<(), TaskStoreError> {
        let Some(workspace_id) = self
            .unique_wake_workspace(task_id, event_id)
            .await?
            .resolve(&format!("wake {task_id}/{event_id}"), true)?
        else {
            return Ok(());
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'dead', resolved_at = ?, failure_reason = ?
             WHERE task_id = ? AND event_id = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now())
        .bind(reason)
        .bind(task_id)
        .bind(event_id)
        .execute(&pool)
        .await
        .map_err(unavailable)?;
        Ok(())
    }

    pub async fn harness_requeue_with_finalize_intent(
        &self,
        intent: &TaskFinalizeIntent,
    ) -> Result<(), TaskStoreError> {
        let pool = self.workspace_pool(&intent.workspace_id).await.map_err(unavailable)?;
        let mut tx = pool.begin().await.map_err(unavailable)?;
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
            return Err(TaskStoreError::ConcurrentUpdate);
        }
        upsert_finalize_intent(&mut tx, intent, Utc::now()).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    pub async fn harness_put_finalize_intent(&self, intent: &TaskFinalizeIntent) -> Result<(), TaskStoreError> {
        let pool = self.workspace_pool(&intent.workspace_id).await.map_err(unavailable)?;
        let mut tx = pool.begin().await.map_err(unavailable)?;
        upsert_finalize_intent(&mut tx, intent, Utc::now()).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    pub async fn harness_finalize_intent(
        &self,
        task_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskFinalizeIntent>, TaskStoreError> {
        let Some(workspace_id) = self
            .unique_task_workspace(task_id)
            .await?
            .resolve(&format!("task {task_id}"), false)?
        else {
            return Ok(None);
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        sqlx::query_as::<_, FinalizeIntentRow>(
            "SELECT task_id, workspace_id, wake_event_id, run_id, report_json,
                    report_persisted, report_emitted, undelivered_json, created_at
             FROM task_finalize_intents WHERE task_id = ? AND wake_event_id = ?",
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(&pool)
        .await
        .map_err(unavailable)?
        .map(FinalizeIntentRow::into_intent)
        .transpose()
    }

    pub async fn harness_pending_finalize_intents(&self) -> Result<Vec<TaskFinalizeIntent>, TaskStoreError> {
        let mut intents = Vec::new();
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            let rows = match sqlx::query_as::<_, FinalizeIntentRow>(
                "SELECT i.task_id, i.workspace_id, i.wake_event_id, i.run_id, i.report_json,
                        i.report_persisted, i.report_emitted, i.undelivered_json, i.created_at
                 FROM task_finalize_intents i
                 JOIN task_wake_outbox w
                   ON w.task_id = i.task_id AND w.event_id = i.wake_event_id
                 JOIN agent_tasks t ON t.id = i.task_id
                 WHERE w.resolved_at IS NULL
                   AND t.state NOT IN ('completed', 'failed', 'cancelled')
                 ORDER BY i.created_at ASC, i.task_id ASC, i.wake_event_id ASC",
            )
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "pending finalize-intent scan failed; skipping workspace"
                    );
                    continue;
                }
            };
            for row in rows {
                intents.push((row.created_at, row.into_intent()?));
            }
        }
        intents.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.task_id.cmp(&right.1.task_id))
                .then_with(|| left.1.wake_event_id.cmp(&right.1.wake_event_id))
        });
        Ok(intents.into_iter().map(|(_, intent)| intent).collect())
    }

    pub async fn harness_complete_finalize_intent(&self, task_id: &str, event_id: &str) -> Result<(), TaskStoreError> {
        let Some(workspace_id) = self
            .unique_task_workspace(task_id)
            .await?
            .resolve(&format!("task {task_id}"), true)?
        else {
            return Ok(());
        };
        let pool = self.workspace_pool(&workspace_id).await.map_err(unavailable)?;
        sqlx::query("DELETE FROM task_finalize_intents WHERE task_id = ? AND wake_event_id = ?")
            .bind(task_id)
            .bind(event_id)
            .execute(&pool)
            .await
            .map_err(unavailable)?;
        Ok(())
    }

    pub async fn harness_completed_run_report(&self, run_id: &str) -> Result<Option<RunReport>, TaskStoreError> {
        let Some(workspace_id) = self
            .unique_run_workspace(run_id)
            .await?
            .resolve(&format!("run {run_id}"), false)?
        else {
            return Ok(None);
        };
        self.completed_run_in(&workspace_id, run_id).await
    }

    pub async fn harness_count_finalize_intents(&self) -> Result<i64, TaskStoreError> {
        let mut count = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_finalize_intents")
                .fetch_one(&pool)
                .await
            {
                Ok(workspace_count) => count += workspace_count,
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "finalize-intent count failed; skipping workspace"
                    );
                }
            }
        }
        Ok(count)
    }

    pub async fn harness_prune_finalize_intents(&self, older_than: DateTime<Utc>) -> Result<usize, TaskStoreError> {
        let mut affected = 0;
        for workspace_id in self.harness_workspaces().await? {
            let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                continue;
            };
            match sqlx::query(
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
                         AND t.state NOT IN ('completed', 'failed', 'cancelled')
                   )",
            )
            .bind(older_than)
            .execute(&pool)
            .await
            {
                Ok(done) => affected += done.rows_affected(),
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "finalize-intent prune failed; skipping workspace"
                    );
                }
            }
        }
        Ok(affected as usize)
    }

    pub async fn harness_retry_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, TaskStoreError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(unavailable)?;
        let affected = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'pending', dispatched_at = NULL, resolved_at = NULL
             WHERE event_id = ? AND dispatch_state = 'dead'
               AND task_id IN (
                   SELECT id FROM agent_tasks WHERE execution_mode = 'production'
               )",
        )
        .bind(event_id)
        .execute(&pool)
        .await
        .map_err(unavailable)?
        .rows_affected();
        Ok(affected > 0)
    }

    pub async fn harness_list_nonterminal_tasks(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<AgentTask>, TaskStoreError> {
        let scoped = workspace_id.is_some();
        let workspaces = match workspace_id {
            Some(workspace_id) => vec![workspace_id.to_string()],
            None => self.harness_workspaces().await?,
        };
        let mut tasks = Vec::new();
        for workspace_id in workspaces {
            let pool = if scoped {
                self.workspace_pool(&workspace_id).await.map_err(unavailable)?
            } else {
                let Some(pool) = self.try_workspace_pool(&workspace_id).await else {
                    continue;
                };
                pool
            };
            let rows = match sqlx::query_as::<_, AgentTaskRow>(sqlx::AssertSqlSafe(format!(
                "{TASK_SELECT} FROM agent_tasks
                 WHERE execution_mode = 'production' AND {}
                 ORDER BY runtime_updated_at, id",
                nonterminal_state_placeholders()
            )))
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(error) if !scoped => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        %error,
                        "nonterminal task scan failed; skipping workspace"
                    );
                    continue;
                }
                Err(error) => return Err(unavailable(error)),
            };
            for row in rows {
                tasks.push(row.into_task(&workspace_id)?);
            }
        }
        tasks.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATUSES: [TaskStatus; 9] = [
        TaskStatus::Active,
        TaskStatus::WaitingJob,
        TaskStatus::WaitingWatch,
        TaskStatus::NeedsApproval,
        TaskStatus::Paused,
        TaskStatus::Cancelling,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ];

    /// One bidirectional mapping check for F6: every port status maps to a
    /// persisted Harness state and back, aliases resolve to the working state,
    /// and both transition_task and begin_run consume the same derived
    /// predecessor set.
    #[test]
    fn status_state_mapping_and_predecessors_are_bidirectional() {
        for status in ALL_STATUSES {
            assert_eq!(
                status_from_state(state_for_status(status)),
                Some(status),
                "status {status:?} must round-trip"
            );
        }
        assert_eq!(state_for_status(TaskStatus::Active), "running");
        for alias in ["pending", "model_calling", "interrupted"] {
            assert_eq!(status_from_state(alias), Some(TaskStatus::Active));
        }

        let active_predecessors = predecessor_states(TaskStatus::Active);
        for expected in [
            "running",
            "pending",
            "model_calling",
            "interrupted",
            "waiting_job",
            "waiting_watch",
            "paused",
            "needs_approval",
        ] {
            assert!(
                active_predecessors.contains(&expected),
                "begin_run/transition_task predecessors must include {expected}"
            );
        }
        for forbidden in ["completed", "failed", "cancelled", "cancelling"] {
            assert!(
                !active_predecessors.contains(&forbidden),
                "predecessors must not include {forbidden}"
            );
        }

        // Completed comes from Active/WaitingJob/WaitingWatch/NeedsApproval,
        // never from Paused/Cancelling/terminal statuses.
        let completed_predecessors = predecessor_states(TaskStatus::Completed);
        for expected in [
            "running",
            "pending",
            "model_calling",
            "interrupted",
            "waiting_job",
            "waiting_watch",
            "needs_approval",
        ] {
            assert!(completed_predecessors.contains(&expected));
        }
        for forbidden in ["paused", "cancelling", "completed", "failed", "cancelled"] {
            assert!(!completed_predecessors.contains(&forbidden));
        }
    }
}
