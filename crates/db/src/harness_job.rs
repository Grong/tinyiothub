//! Workspace-Harness-backed Device Job lifecycle (Task 19C3).
//!
//! Every Job fact lives in the owning workspace's Harness SQLite generation.
//! This module is the only SQL owner for the workspace `device_jobs` lifecycle:
//! create, reads, state transitions, terminal state + Task wake (one
//! transaction), progress revisions, idempotent stop, cross-workspace sweeps,
//! and the unknown-outcome disposition used by the rollout drain.
//!
//! Cross-workspace sweeps degrade per unreadable workspace with a warning and
//! skip. Point reads fail closed when a workspace cannot be read, so an
//! undecidable lookup never silently reports "no Job" (Task 19A N1).

use chrono::Utc;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tinyiothub_core::agent_task::TaskWake;
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};

use crate::Db;
use crate::device_job::DeviceJobRow;
use crate::error::DbError;
use crate::runtime_admin::UnknownDispositionOutcome;

const JOB_COLUMNS: &str = "id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                           progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                           timeout_ms, started_at, ended_at";

fn db_job_storage(error: DbError) -> DeviceJobError {
    DeviceJobError::Storage(error.to_string())
}

fn storage(error: impl std::fmt::Display) -> DeviceJobError {
    DeviceJobError::Storage(error.to_string())
}

fn try_job(row: DeviceJobRow) -> Result<DeviceJob, DeviceJobError> {
    DeviceJob::try_from(row)
}

fn validate_wake(job: &DeviceJob, wake: &TaskWake) -> Result<(), DeviceJobError> {
    if job.task_id.as_deref() != Some(wake.task_id.as_str()) {
        return Err(DeviceJobError::Validation("job and wake task id differ".to_string()));
    }
    let signal: Value = serde_json::from_str(&wake.signal_json)
        .map_err(|error| DeviceJobError::Validation(format!("invalid wake signal: {error}")))?;
    if signal.get("workspace_id").and_then(Value::as_str) != Some(job.workspace_id.as_str()) {
        return Err(DeviceJobError::Validation(
            "job and wake workspace id differ".to_string(),
        ));
    }
    Ok(())
}

async fn insert_job_wake(tx: &mut Transaction<'_, Sqlite>, wake: &TaskWake) -> Result<(), DeviceJobError> {
    sqlx::query(
        "INSERT INTO task_wake_outbox
             (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&wake.task_id)
    .bind(&wake.event_id)
    .bind(&wake.signal_json)
    .bind(wake.dispatch_state.as_str())
    .bind(wake.created_at.to_rfc3339())
    .bind(wake.dispatched_at.map(|value| value.to_rfc3339()))
    .bind(wake.resolved_at.map(|value| value.to_rfc3339()))
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

async fn find_job(pool: &SqlitePool, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
    sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {JOB_COLUMNS} FROM device_jobs WHERE workspace_id = ? AND id = ?"
    )))
    .bind(workspace_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(storage)?
    .map(try_job)
    .transpose()
}

async fn classify_insert_conflict(pool: &SqlitePool, job: &DeviceJob) -> DeviceJobError {
    let id_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_jobs WHERE id = ?)")
        .bind(&job.id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
    if id_exists {
        return DeviceJobError::Conflict(format!("workspace job {} already exists", job.id));
    }
    if let Some(tool_call_id) = &job.tool_call_id {
        let tool_call_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_jobs WHERE workspace_id = ? AND tool_call_id = ?)")
                .bind(&job.workspace_id)
                .bind(tool_call_id)
                .fetch_one(pool)
                .await
                .unwrap_or(false);
        if tool_call_exists {
            return DeviceJobError::Conflict(format!("workspace tool call {tool_call_id} already has a Job"));
        }
    }
    DeviceJobError::Conflict(format!("workspace job {} violates a unique constraint", job.id))
}

async fn job_counts_pool(pool: &SqlitePool) -> Result<(u64, u64), DeviceJobError> {
    let (active, unresolved): (i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(*) FROM device_jobs WHERE state IN ('accepted', 'running')),
           (SELECT COUNT(*) FROM device_jobs job
             WHERE job.state IN ('cancelled_unknown', 'interrupted')
               AND NOT EXISTS (
                 SELECT 1 FROM runtime_unknown_dispositions disposition
                 WHERE disposition.job_id = job.id
               ))",
    )
    .fetch_one(pool)
    .await
    .map_err(storage)?;
    Ok((
        u64::try_from(active).unwrap_or(0),
        u64::try_from(unresolved).unwrap_or(0),
    ))
}

async fn job_state_counts_pool(pool: &SqlitePool) -> Result<Vec<(String, i64)>, DeviceJobError> {
    sqlx::query_as("SELECT state, COUNT(*) FROM device_jobs GROUP BY state")
        .fetch_all(pool)
        .await
        .map_err(storage)
}

impl Db {
    /// Insert one Job into the owning workspace generation. Insert-only: a
    /// deterministic id that already exists is a Conflict, so a retried submit
    /// cannot reach a worker twice.
    pub async fn create_workspace_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
        let timeout_ms = i64::try_from(job.timeout_ms)
            .map_err(|_| DeviceJobError::Validation("timeout is too large".to_string()))?;
        let pool = self.workspace_pool(&job.workspace_id).await.map_err(db_job_storage)?;
        let inserted = sqlx::query(
            "INSERT INTO device_jobs
                (id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                 progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                 timeout_ms, started_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&job.id)
        .bind(&job.workspace_id)
        .bind(&job.device_id)
        .bind(&job.tool)
        .bind(serde_json::to_string(&job.args_json).map_err(storage)?)
        .bind(job.state.as_str())
        .bind(job.stop_requested_at.map(|value| value.to_rfc3339()))
        .bind(
            job.progress_json
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(storage)?,
        )
        .bind(
            job.result_json
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(storage)?,
        )
        .bind(&job.task_id)
        .bind(&job.run_id)
        .bind(&job.tool_call_id)
        .bind(&job.driver_call_id)
        .bind(timeout_ms)
        .bind(job.started_at.to_rfc3339())
        .bind(job.ended_at.map(|value| value.to_rfc3339()))
        .execute(&pool)
        .await;
        match inserted {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(database)) if database.is_unique_violation() => {
                Err(classify_insert_conflict(&pool, job).await)
            }
            Err(error) => Err(storage(error)),
        }
    }

    /// Upsert the workspace Job projection used by Effect settlement.
    pub async fn upsert_workspace_job_projection(
        &self,
        workspace_id: &str,
        job: &DeviceJob,
    ) -> std::result::Result<(), DbError> {
        let timeout_ms = i64::try_from(job.timeout_ms).map_err(|_| DbError::Validation {
            message: "timeout is too large".to_string(),
        })?;
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query(
            "INSERT INTO device_jobs
                (id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                 progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                 timeout_ms, started_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 device_id = excluded.device_id,
                 tool = excluded.tool,
                 args_json = excluded.args_json,
                 state = excluded.state,
                 stop_requested_at = excluded.stop_requested_at,
                 progress_json = excluded.progress_json,
                 result_json = excluded.result_json,
                 task_id = excluded.task_id,
                 run_id = excluded.run_id,
                 tool_call_id = excluded.tool_call_id,
                 driver_call_id = excluded.driver_call_id,
                 timeout_ms = excluded.timeout_ms,
                 started_at = excluded.started_at,
                 ended_at = excluded.ended_at",
        )
        .bind(&job.id)
        .bind(workspace_id)
        .bind(&job.device_id)
        .bind(&job.tool)
        .bind(serde_json::to_string(&job.args_json)?)
        .bind(job.state.as_str())
        .bind(job.stop_requested_at.map(|value| value.to_rfc3339()))
        .bind(job.progress_json.as_ref().map(serde_json::to_string).transpose()?)
        .bind(job.result_json.as_ref().map(serde_json::to_string).transpose()?)
        .bind(&job.task_id)
        .bind(&job.run_id)
        .bind(&job.tool_call_id)
        .bind(&job.driver_call_id)
        .bind(timeout_ms)
        .bind(job.started_at.to_rfc3339())
        .bind(job.ended_at.map(|value| value.to_rfc3339()))
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn get_workspace_job(
        &self,
        workspace_id: &str,
        job_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        let Some(pool) = self
            .try_existing_workspace_pool(workspace_id)
            .await
            .map_err(db_job_storage)?
        else {
            return Ok(None);
        };
        find_job(&pool, workspace_id, job_id).await
    }

    pub async fn get_workspace_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        let Some(pool) = self
            .try_existing_workspace_pool(workspace_id)
            .await
            .map_err(db_job_storage)?
        else {
            return Ok(None);
        };
        sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_COLUMNS} FROM device_jobs WHERE workspace_id = ? AND task_id = ? AND id = ?"
        )))
        .bind(workspace_id)
        .bind(task_id)
        .bind(job_id)
        .fetch_optional(&pool)
        .await
        .map_err(storage)?
        .map(try_job)
        .transpose()
    }

    pub async fn get_workspace_job_by_tool_call(
        &self,
        workspace_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        let Some(pool) = self
            .try_existing_workspace_pool(workspace_id)
            .await
            .map_err(db_job_storage)?
        else {
            return Ok(None);
        };
        sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_COLUMNS} FROM device_jobs WHERE workspace_id = ? AND tool_call_id = ?"
        )))
        .bind(workspace_id)
        .bind(tool_call_id)
        .fetch_optional(&pool)
        .await
        .map_err(storage)?
        .map(try_job)
        .transpose()
    }

    /// Resolve the workspace that owns a Job id. Cross-workspace sweeps degrade
    /// per unreadable workspace, but an absent or mutation-bound lookup fails
    /// closed: an unreadable workspace could be the owner (Task 19A N1).
    pub async fn find_workspace_for_job(&self, job_id: &str, mutating: bool) -> Result<Option<String>, DeviceJobError> {
        let mut matched: Option<String> = None;
        let mut skipped = 0usize;
        for workspace_id in self.harness_workspaces().await.map_err(storage)? {
            let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
                skipped += 1;
                continue;
            };
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM device_jobs WHERE id = ?")
                .bind(job_id)
                .fetch_one(&pool)
                .await
            {
                Ok(0) => {}
                Ok(_) => {
                    if matched.is_some() {
                        return Err(DeviceJobError::Storage(format!(
                            "device job {job_id} exists in more than one workspace"
                        )));
                    }
                    matched = Some(workspace_id);
                }
                Err(error) => {
                    tracing::warn!(%workspace_id, %error, "device job lookup failed; skipping workspace");
                    skipped += 1;
                }
            }
        }
        if skipped > 0 && (matched.is_none() || mutating) {
            return Err(DeviceJobError::Storage(format!(
                "device job {job_id}: {skipped} workspace store(s) unreadable; refusing cross-workspace lookup"
            )));
        }
        Ok(matched)
    }

    pub async fn mark_workspace_job_running(
        &self,
        workspace_id: &str,
        job_id: &str,
        driver_call_id: &str,
    ) -> Result<DeviceJob, DeviceJobError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(db_job_storage)?;
        let updated = sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE device_jobs SET state = 'running', driver_call_id = ?
             WHERE workspace_id = ? AND id = ? AND state = 'accepted'
             RETURNING {JOB_COLUMNS}"
        )))
        .bind(driver_call_id)
        .bind(workspace_id)
        .bind(job_id)
        .fetch_optional(&pool)
        .await
        .map_err(storage)?
        .map(try_job)
        .transpose()?;
        if let Some(job) = updated {
            return Ok(job);
        }
        let current = find_job(&pool, workspace_id, job_id)
            .await?
            .ok_or(DeviceJobError::NotFound)?;
        Err(DeviceJobError::Conflict(format!(
            "job '{}' is {} rather than accepted",
            current.id,
            current.state.as_str()
        )))
    }

    /// Terminal state, result and Task wake commit in one workspace
    /// transaction. A replay returns `false` and writes nothing.
    pub async fn finish_workspace_job(
        &self,
        workspace_id: &str,
        job_id: &str,
        state: DeviceJobState,
        result: &Value,
        wake: Option<&TaskWake>,
    ) -> Result<bool, DeviceJobError> {
        if !state.is_terminal() {
            return Err(DeviceJobError::Validation(
                "terminal result has an active state".to_string(),
            ));
        }
        let pool = self.workspace_pool(workspace_id).await.map_err(db_job_storage)?;
        let mut tx = pool.begin().await.map_err(storage)?;
        // Write first: the UPDATE acquires RESERVED before any read, avoiding
        // the deferred SHARED -> RESERVED upgrade that fails immediately.
        let updated = sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE device_jobs SET state = ?, result_json = ?, ended_at = ?
             WHERE workspace_id = ? AND id = ? AND state IN ('accepted', 'running')
             RETURNING {JOB_COLUMNS}"
        )))
        .bind(state.as_str())
        .bind(serde_json::to_string(result).map_err(storage)?)
        .bind(Utc::now().to_rfc3339())
        .bind(workspace_id)
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .map(try_job)
        .transpose()?;
        let Some(job) = updated else {
            let current = sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLUMNS} FROM device_jobs WHERE workspace_id = ? AND id = ?"
            )))
            .bind(workspace_id)
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
            .ok_or(DeviceJobError::NotFound)?;
            let current = try_job(current)?;
            if current.state.is_terminal() {
                return Ok(false);
            }
            return Err(DeviceJobError::Conflict(format!(
                "job '{}' is {} rather than terminalizable",
                current.id,
                current.state.as_str()
            )));
        };
        if let Some(wake) = wake {
            validate_wake(&job, wake)?;
            insert_job_wake(&mut tx, wake).await?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(true)
    }

    pub async fn update_workspace_job_progress(
        &self,
        workspace_id: &str,
        job_id: &str,
        progress: &Value,
    ) -> Result<bool, DeviceJobError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(db_job_storage)?;
        let progress_json = serde_json::to_string(progress).map_err(storage)?;
        let mut tx = pool.begin().await.map_err(storage)?;
        let updated = sqlx::query(
            "UPDATE device_jobs
             SET progress_json = ?, progress_revision = progress_revision + 1
             WHERE workspace_id = ? AND id = ? AND state IN ('accepted', 'running')
               AND (progress_json IS NULL OR progress_json != ?)",
        )
        .bind(&progress_json)
        .bind(workspace_id)
        .bind(job_id)
        .bind(&progress_json)
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected();
        if updated == 0 {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE workspace_id = ? AND id = ?")
                .bind(workspace_id)
                .bind(job_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            if exists == 0 {
                return Err(DeviceJobError::NotFound);
            }
            return Ok(false);
        }
        tx.commit().await.map_err(storage)?;
        Ok(true)
    }

    /// Durable, idempotent stop request. A terminal Job is returned unchanged.
    pub async fn request_workspace_job_stop(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<DeviceJob, DeviceJobError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(db_job_storage)?;
        let mut tx = pool.begin().await.map_err(storage)?;
        let updated = sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE device_jobs SET stop_requested_at = COALESCE(stop_requested_at, ?)
             WHERE workspace_id = ? AND task_id = ? AND id = ? AND state IN ('accepted', 'running')
             RETURNING {JOB_COLUMNS}"
        )))
        .bind(Utc::now().to_rfc3339())
        .bind(workspace_id)
        .bind(task_id)
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .map(try_job)
        .transpose()?;
        let job = match updated {
            Some(job) => job,
            None => {
                let row = sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(format!(
                    "SELECT {JOB_COLUMNS} FROM device_jobs WHERE workspace_id = ? AND task_id = ? AND id = ?"
                )))
                .bind(workspace_id)
                .bind(task_id)
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?
                .ok_or(DeviceJobError::NotFound)?;
                try_job(row)?
            }
        };
        tx.commit().await.map_err(storage)?;
        Ok(job)
    }

    /// Workspace-scoped (`Some`) active/unknown sweep, or a cross-workspace
    /// sweep (`None`) that warns and skips an unreadable workspace.
    pub async fn active_workspace_jobs(
        &self,
        workspace_id: Option<&str>,
        unknown_only: bool,
    ) -> Result<Vec<DeviceJob>, DeviceJobError> {
        let mut jobs = Vec::new();
        match workspace_id {
            Some(workspace_id) => {
                let Some(pool) = self
                    .try_existing_workspace_pool(workspace_id)
                    .await
                    .map_err(db_job_storage)?
                else {
                    return Ok(jobs);
                };
                jobs.extend(active_jobs_pool(&pool, unknown_only).await?);
            }
            None => {
                for workspace_id in self.harness_workspaces().await.map_err(storage)? {
                    let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
                        continue;
                    };
                    match active_jobs_pool(&pool, unknown_only).await {
                        Ok(mut found) => jobs.append(&mut found),
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace job scan failed");
                        }
                    }
                }
            }
        }
        jobs.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(jobs)
    }

    pub async fn workspace_job_counts(&self, workspace_id: Option<&str>) -> Result<(u64, u64), DeviceJobError> {
        match workspace_id {
            Some(workspace_id) => {
                let Some(pool) = self
                    .try_existing_workspace_pool(workspace_id)
                    .await
                    .map_err(db_job_storage)?
                else {
                    return Ok((0, 0));
                };
                job_counts_pool(&pool).await
            }
            None => {
                let mut active = 0u64;
                let mut unresolved = 0u64;
                for workspace_id in self.harness_workspaces().await.map_err(storage)? {
                    let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
                        continue;
                    };
                    match job_counts_pool(&pool).await {
                        Ok((workspace_active, workspace_unresolved)) => {
                            active += workspace_active;
                            unresolved += workspace_unresolved;
                        }
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace job count failed");
                        }
                    }
                }
                Ok((active, unresolved))
            }
        }
    }

    pub async fn workspace_job_state_counts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<(String, i64)>, DeviceJobError> {
        match workspace_id {
            Some(workspace_id) => {
                let Some(pool) = self
                    .try_existing_workspace_pool(workspace_id)
                    .await
                    .map_err(db_job_storage)?
                else {
                    return Ok(Vec::new());
                };
                job_state_counts_pool(&pool).await
            }
            None => {
                let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
                for workspace_id in self.harness_workspaces().await.map_err(storage)? {
                    let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
                        continue;
                    };
                    match job_state_counts_pool(&pool).await {
                        Ok(rows) => {
                            for (state, count) in rows {
                                *counts.entry(state).or_insert(0) += count;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace job gauge failed");
                        }
                    }
                }
                Ok(counts.into_iter().collect())
            }
        }
    }

    /// Record an administrator's physical disposition for a workspace Job whose
    /// outcome is unknown. The Job row is not rewritten and no side effect is
    /// replayed.
    pub async fn record_workspace_unknown_disposition(
        &self,
        workspace_id: &str,
        job_id: &str,
        disposition: &str,
        recorded_by: &str,
    ) -> Result<UnknownDispositionOutcome, DeviceJobError> {
        let pool = self.workspace_pool(workspace_id).await.map_err(db_job_storage)?;
        let affected = sqlx::query(
            "INSERT OR IGNORE INTO runtime_unknown_dispositions
                 (job_id, workspace_id, task_id, device_id, disposition, recorded_by, recorded_at)
             SELECT id, workspace_id, task_id, device_id, ?, ?, ?
             FROM device_jobs
             WHERE workspace_id = ? AND id = ? AND state IN ('cancelled_unknown', 'interrupted')",
        )
        .bind(disposition)
        .bind(recorded_by)
        .bind(Utc::now().to_rfc3339())
        .bind(workspace_id)
        .bind(job_id)
        .execute(&pool)
        .await
        .map_err(storage)?
        .rows_affected();
        if affected == 1 {
            tracing::info!(
                metric = "runtime_unknown_disposition",
                %workspace_id,
                %job_id,
                "unknown Job disposition recorded"
            );
            return Ok(UnknownDispositionOutcome::Recorded);
        }
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM device_jobs WHERE workspace_id = ? AND id = ?")
                .bind(workspace_id)
                .bind(job_id)
                .fetch_optional(&pool)
                .await
                .map_err(storage)?;
        let Some(state) = state else {
            return Ok(UnknownDispositionOutcome::NotFound);
        };
        if state != "cancelled_unknown" && state != "interrupted" {
            return Ok(UnknownDispositionOutcome::NotUnknown);
        }
        Ok(UnknownDispositionOutcome::AlreadyRecorded)
    }
}

async fn active_jobs_pool(pool: &SqlitePool, unknown_only: bool) -> Result<Vec<DeviceJob>, DeviceJobError> {
    let sql = if unknown_only {
        format!(
            "SELECT {JOB_COLUMNS} FROM device_jobs
             WHERE state IN ('cancelled_unknown', 'interrupted')
             ORDER BY started_at, id"
        )
    } else {
        format!(
            "SELECT {JOB_COLUMNS} FROM device_jobs
             WHERE state IN ('accepted', 'running')
             ORDER BY started_at, id"
        )
    };
    sqlx::query_as::<_, DeviceJobRow>(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .map_err(storage)?
        .into_iter()
        .map(try_job)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use serde_json::json;
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};

    use crate::Db;
    use crate::runtime_admin::UnknownDispositionOutcome;

    const WS: &str = "ws-jobs";

    struct Fixture {
        root: PathBuf,
        db: Db,
    }

    impl Fixture {
        async fn new(workspaces: &[&str]) -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-job-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            for workspace_id in workspaces {
                sqlx::query(
                    "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
                     VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
                )
                .bind(workspace_id)
                .execute(db.pool())
                .await
                .expect("insert workspace");
                db.create_workspace_store(workspace_id)
                    .await
                    .expect("create workspace store");
            }
            Self { root, db }
        }

        async fn workspace_pool(&self, workspace_id: &str) -> SqlitePool {
            self.db.workspace_pool(workspace_id).await.expect("workspace pool")
        }

        async fn close(self) {
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    fn job(id: &str, workspace_id: &str, task_id: &str) -> DeviceJob {
        DeviceJob {
            id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            device_id: "device-1".to_string(),
            tool: "set_speed".to_string(),
            args_json: json!({"speed": 2}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(task_id.to_string()),
            run_id: Some("run-1".to_string()),
            tool_call_id: Some(format!("call-{id}")),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    fn wake(job_id: &str, task_id: &str, workspace_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.to_string(),
            event_id: format!("device_job:{job_id}:done"),
            signal_json: json!({"workspace_id": workspace_id, "job_id": job_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    async fn platform_jobs(db: &Db) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs")
            .fetch_one(db.pool())
            .await
            .expect("platform job count")
    }

    async fn platform_wakes(db: &Db) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(db.pool())
            .await
            .expect("platform wake count")
    }

    #[tokio::test]
    async fn lifecycle_matches_legacy_semantics_and_commits_the_wake_once() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        let job = job("job-1", WS, "task-1");
        db.create_workspace_job(&job).await.expect("create job");

        assert_eq!(platform_jobs(db).await, 0, "the platform device_jobs writer is retired");
        let stored = db.get_workspace_job(WS, "job-1").await.expect("read job").expect("job");
        assert_eq!(stored.state, DeviceJobState::Accepted);
        assert!(matches!(
            db.create_workspace_job(&job).await,
            Err(DeviceJobError::Conflict(_))
        ));

        let running = db
            .mark_workspace_job_running(WS, "job-1", "driver-1")
            .await
            .expect("mark running");
        assert_eq!(running.state, DeviceJobState::Running);
        assert_eq!(running.driver_call_id.as_deref(), Some("driver-1"));
        assert!(matches!(
            db.mark_workspace_job_running(WS, "job-1", "driver-2").await,
            Err(DeviceJobError::Conflict(_))
        ));

        assert!(
            db.update_workspace_job_progress(WS, "job-1", &json!({"percent": 50}))
                .await
                .expect("progress")
        );
        assert!(
            !db.update_workspace_job_progress(WS, "job-1", &json!({"percent": 50}))
                .await
                .expect("same progress is not a new revision")
        );

        let first_stop = db
            .request_workspace_job_stop(WS, "task-1", "job-1")
            .await
            .expect("stop");
        let second_stop = db
            .request_workspace_job_stop(WS, "task-1", "job-1")
            .await
            .expect("stop replay");
        assert_eq!(first_stop.stop_requested_at, second_stop.stop_requested_at);
        assert!(matches!(
            db.request_workspace_job_stop(WS, "task-2", "job-1").await,
            Err(DeviceJobError::NotFound)
        ));

        let wake = wake("job-1", "task-1", WS);
        assert!(
            db.finish_workspace_job(WS, "job-1", DeviceJobState::Done, &json!({"ok": true}), Some(&wake))
                .await
                .expect("finish")
        );
        assert!(
            !db.finish_workspace_job(WS, "job-1", DeviceJobState::Done, &json!({"ok": true}), Some(&wake))
                .await
                .expect("finish replay")
        );
        let terminal = db
            .get_workspace_job(WS, "job-1")
            .await
            .expect("read terminal")
            .expect("terminal");
        assert_eq!(terminal.state, DeviceJobState::Done);
        assert_eq!(terminal.result_json, Some(json!({"ok": true})));
        assert!(terminal.ended_at.is_some());

        let wake_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = 'task-1' AND event_id = ?")
                .bind("device_job:job-1:done")
                .fetch_one(&fixture.workspace_pool(WS).await)
                .await
                .expect("workspace wake");
        assert_eq!(wake_count, 1, "the terminal wake lives in the workspace outbox");
        assert_eq!(platform_wakes(db).await, 0, "no platform wake is written");
        assert!(
            db.active_workspace_jobs(Some(WS), false)
                .await
                .expect("active")
                .is_empty()
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn invalid_wake_rolls_back_the_terminal_fact() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-rollback", WS, "task-1"))
            .await
            .expect("create");

        let error = db
            .finish_workspace_job(
                WS,
                "job-rollback",
                DeviceJobState::Done,
                &json!({"ok": true}),
                Some(&wake("job-rollback", "task-2", WS)),
            )
            .await
            .expect_err("mismatched wake must fail");
        assert!(matches!(error, DeviceJobError::Validation(_)));
        assert_eq!(
            db.get_workspace_job(WS, "job-rollback")
                .await
                .expect("read")
                .expect("job")
                .state,
            DeviceJobState::Accepted
        );
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&fixture.workspace_pool(WS).await)
            .await
            .expect("wake count");
        assert_eq!(wakes, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn unknown_and_interrupted_jobs_stay_visible_for_disposition() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-active", WS, "task-1"))
            .await
            .expect("active");
        db.create_workspace_job(&job("job-unknown", WS, "task-1"))
            .await
            .expect("unknown");
        db.finish_workspace_job(WS, "job-unknown", DeviceJobState::CancelledUnknown, &json!({}), None)
            .await
            .expect("unknown finish");
        db.create_workspace_job(&job("job-interrupted", WS, "task-1"))
            .await
            .expect("interrupted");
        db.finish_workspace_job(
            WS,
            "job-interrupted",
            DeviceJobState::Interrupted,
            &json!({"reason": "worker call lost"}),
            None,
        )
        .await
        .expect("interrupted finish");

        let active = db.active_workspace_jobs(Some(WS), false).await.expect("active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "job-active");

        let unknown = db.active_workspace_jobs(Some(WS), true).await.expect("unknown");
        let ids: Vec<&str> = unknown.iter().map(|job| job.id.as_str()).collect();
        assert_eq!(
            ids,
            ["job-unknown", "job-interrupted"],
            "the legacy sweep includes interrupted Jobs"
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn cross_workspace_sweeps_include_two_workspaces_and_skip_a_corrupt_one() {
        let fixture = Fixture::new(&["ws-a", "ws-b", "ws-corrupt"]).await;
        let db = &fixture.db;
        let mut first = job("job-a", "ws-a", "task-a");
        first.started_at = Utc::now() - chrono::Duration::seconds(10);
        let mut second = job("job-b", "ws-b", "task-b");
        second.started_at = Utc::now();
        db.create_workspace_job(&first).await.expect("job a");
        db.create_workspace_job(&second).await.expect("job b");
        db.create_workspace_job(&job("job-corrupt", "ws-corrupt", "task-c"))
            .await
            .expect("job corrupt");
        db.mark_workspace_unavailable("ws-corrupt", "injected corruption")
            .await
            .expect("mark unavailable");

        let active = db.active_workspace_jobs(None, false).await.expect("sweep");
        let ids: Vec<&str> = active.iter().map(|job| job.id.as_str()).collect();
        assert_eq!(ids, ["job-a", "job-b"], "a corrupt workspace is skipped, not fatal");

        // A pure read can still return the unique readable match.
        assert_eq!(
            db.find_workspace_for_job("job-a", false).await.expect("read lookup"),
            Some("ws-a".to_string())
        );
        // The unreadable workspace may be the owner: point reads must not fail open.
        assert!(matches!(
            db.find_workspace_for_job("job-corrupt", false).await,
            Err(DeviceJobError::Storage(_))
        ));
        // Mutations must fail closed whenever any workspace is unreadable.
        assert!(matches!(
            db.find_workspace_for_job("job-a", true).await,
            Err(DeviceJobError::Storage(_))
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn cross_workspace_duplicate_job_ids_are_an_error_not_a_first_match() {
        let fixture = Fixture::new(&["ws-a", "ws-b"]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-dup", "ws-a", "task-a"))
            .await
            .expect("job a");
        db.create_workspace_job(&job("job-dup", "ws-b", "task-b"))
            .await
            .expect("job b");

        assert!(matches!(
            db.find_workspace_for_job("job-dup", false).await,
            Err(DeviceJobError::Storage(message)) if message.contains("more than one workspace")
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn disposition_is_recorded_in_the_workspace_and_releases_the_drain() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-unknown", WS, "task-1"))
            .await
            .expect("job");
        db.finish_workspace_job(
            WS,
            "job-unknown",
            DeviceJobState::CancelledUnknown,
            &json!({"reason": "cancel_unknown"}),
            None,
        )
        .await
        .expect("unknown finish");

        let (active, unresolved) = db.workspace_job_counts(Some(WS)).await.expect("counts");
        assert_eq!((active, unresolved), (0, 1));
        assert_eq!(
            db.record_workspace_unknown_disposition(WS, "job-unknown", "valve closed", "admin")
                .await
                .expect("record"),
            UnknownDispositionOutcome::Recorded
        );
        assert_eq!(
            db.record_workspace_unknown_disposition(WS, "job-unknown", "valve closed", "admin")
                .await
                .expect("replay"),
            UnknownDispositionOutcome::AlreadyRecorded
        );
        let (_, unresolved) = db.workspace_job_counts(Some(WS)).await.expect("counts");
        assert_eq!(unresolved, 0, "a recorded disposition releases the drain");
        assert_eq!(
            db.record_workspace_unknown_disposition(WS, "missing", "x", "admin")
                .await
                .expect("missing"),
            UnknownDispositionOutcome::NotFound
        );
        let platform_dispositions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runtime_unknown_dispositions")
            .fetch_one(db.pool())
            .await
            .expect("platform dispositions");
        assert_eq!(platform_dispositions, 0, "dispositions live in the workspace");
        fixture.close().await;
    }

    #[tokio::test]
    async fn timeout_overflow_is_validation_not_a_clamped_row() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        let mut oversized = job("job-timeout", WS, "task-1");
        oversized.timeout_ms = u64::MAX;
        assert!(matches!(
            db.create_workspace_job(&oversized).await,
            Err(DeviceJobError::Validation(message)) if message == "timeout is too large"
        ));
        assert!(
            db.get_workspace_job(WS, "job-timeout").await.expect("read").is_none(),
            "a rejected timeout must not persist a clamped row"
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn finishing_with_a_non_terminal_state_is_rejected() {
        let fixture = Fixture::new(&[WS]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-active", WS, "task-1"))
            .await
            .expect("create");
        let error = db
            .finish_workspace_job(WS, "job-active", DeviceJobState::Running, &json!({}), None)
            .await
            .expect_err("a non-terminal result must be rejected");
        assert!(matches!(error, DeviceJobError::Validation(_)));
        assert_eq!(
            db.get_workspace_job(WS, "job-active")
                .await
                .expect("read")
                .expect("job")
                .state,
            DeviceJobState::Accepted
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn state_gauges_are_workspace_scoped() {
        let fixture = Fixture::new(&["ws-a", "ws-b"]).await;
        let db = &fixture.db;
        db.create_workspace_job(&job("job-a", "ws-a", "task-a"))
            .await
            .expect("a");
        db.create_workspace_job(&job("job-b", "ws-b", "task-b"))
            .await
            .expect("b");
        db.mark_workspace_job_running("ws-b", "job-b", "driver-b")
            .await
            .expect("running");

        let mut rows = db.workspace_job_state_counts(Some("ws-b")).await.expect("scoped");
        rows.sort();
        assert_eq!(rows, vec![("running".to_string(), 1)]);
        let mut rows = db.workspace_job_state_counts(None).await.expect("sweep");
        rows.sort();
        assert_eq!(rows, vec![("accepted".to_string(), 1), ("running".to_string(), 1)]);
        fixture.close().await;
    }
}
