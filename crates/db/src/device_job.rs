//! Platform `device_jobs` compatibility surface.
//!
//! Task 19C3 moved the production Device Job lifecycle to the workspace Harness
//! generation ([`crate::harness_job`]). The platform table, its `agent_tasks`
//! FK, and these read helpers remain for diagnostics and fixture parity, but
//! the platform *writers* are compiled only for tests (`cfg(test)` or the
//! `testing` feature). No production composition can reach them.

use chrono::{DateTime, Utc};
use serde_json::Value;
#[cfg(any(test, feature = "testing"))]
use sqlx::{Sqlite, Transaction};
#[cfg(any(test, feature = "testing"))]
use tinyiothub_core::agent_task::TaskWake;
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};

use crate::Db;
#[cfg(any(test, feature = "testing"))]
use crate::runtime_event::{append_runtime_event_tx, job_event};

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct DeviceJobRow {
    id: String,
    workspace_id: String,
    device_id: String,
    tool: String,
    args_json: String,
    state: String,
    stop_requested_at: Option<DateTime<Utc>>,
    progress_json: Option<String>,
    result_json: Option<String>,
    task_id: Option<String>,
    run_id: Option<String>,
    tool_call_id: Option<String>,
    driver_call_id: Option<String>,
    timeout_ms: i64,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl TryFrom<DeviceJobRow> for DeviceJob {
    type Error = DeviceJobError;

    fn try_from(row: DeviceJobRow) -> Result<Self, Self::Error> {
        let state = DeviceJobState::from_db(&row.state)
            .ok_or_else(|| DeviceJobError::Storage(format!("unknown device job state: {}", row.state)))?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            device_id: row.device_id,
            tool: row.tool,
            args_json: parse_json(&row.args_json)?,
            state,
            stop_requested_at: row.stop_requested_at,
            progress_json: row.progress_json.as_deref().map(parse_json).transpose()?,
            result_json: row.result_json.as_deref().map(parse_json).transpose()?,
            task_id: row.task_id,
            run_id: row.run_id,
            tool_call_id: row.tool_call_id,
            driver_call_id: row.driver_call_id,
            timeout_ms: u64::try_from(row.timeout_ms)
                .map_err(|_| DeviceJobError::Storage("negative device job timeout".to_string()))?,
            started_at: row.started_at,
            ended_at: row.ended_at,
        })
    }
}

fn parse_json(raw: &str) -> Result<Value, DeviceJobError> {
    serde_json::from_str(raw).map_err(|error| DeviceJobError::Storage(error.to_string()))
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn storage(error: impl std::fmt::Display) -> DeviceJobError {
    DeviceJobError::Storage(error.to_string())
}

#[cfg(any(test, feature = "testing"))]
fn create_error(error: sqlx::Error) -> DeviceJobError {
    if matches!(&error, sqlx::Error::Database(db) if db.is_unique_violation()) {
        DeviceJobError::Conflict(error.to_string())
    } else {
        storage(error)
    }
}

#[cfg(any(test, feature = "testing"))]
async fn find_job_by_id(
    executor: &mut sqlx::SqliteConnection,
    job_id: &str,
) -> Result<Option<DeviceJob>, DeviceJobError> {
    sqlx::query_as::<_, DeviceJobRow>(
        "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
         FROM device_jobs WHERE id = ?",
    )
    .bind(job_id)
    .fetch_optional(executor)
    .await
    .map_err(storage)?
    .map(DeviceJob::try_from)
    .transpose()
}

#[cfg(any(test, feature = "testing"))]
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
    .bind(wake.created_at)
    .bind(wake.dispatched_at)
    .bind(wake.resolved_at)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

#[cfg(any(test, feature = "testing"))]
fn validate_job_wake(job: &DeviceJob, wake: &TaskWake) -> Result<(), DeviceJobError> {
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

/// Single owner of platform `device_jobs` writes. Legacy approval flows call
/// this helper inside their own transaction instead of repeating the INSERT,
/// so Job state can only enter the table through this module.
#[cfg(any(test, feature = "testing"))]
pub(crate) async fn insert_job_projection(
    connection: &mut sqlx::SqliteConnection,
    job: &DeviceJob,
) -> Result<(), DeviceJobError> {
    sqlx::query(
        "INSERT INTO device_jobs
             (id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
              progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job.id)
    .bind(&job.workspace_id)
    .bind(&job.device_id)
    .bind(&job.tool)
    .bind(serde_json::to_string(&job.args_json).map_err(storage)?)
    .bind(job.state.as_str())
    .bind(job.stop_requested_at)
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
    .bind(i64::try_from(job.timeout_ms).map_err(|_| DeviceJobError::Validation("timeout is too large".to_string()))?)
    .bind(job.started_at)
    .bind(job.ended_at)
    .execute(&mut *connection)
    .await
    .map_err(create_error)?;
    Ok(())
}

impl Db {
    #[cfg(any(test, feature = "testing"))]
    pub async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
        self.create_job_and_event(job).await.map(|_| ())
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn create_job_and_event(
        &self,
        job: &DeviceJob,
    ) -> Result<crate::runtime_event::RuntimeEvent, DeviceJobError> {
        let mut tx = self.pool().begin().await.map_err(storage)?;
        insert_job_projection(&mut tx, job).await?;
        let event = append_runtime_event_tx(
            &mut tx,
            job_event(
                job,
                job.state.as_str(),
                serde_json::json!({"state": job.state.as_str()}),
            ),
        )
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(event)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        sqlx::query_as::<_, DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
             FROM device_jobs WHERE workspace_id = ? AND id = ?",
        )
        .bind(workspace_id)
        .bind(job_id)
        .fetch_optional(self.pool())
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        let mut connection = self.pool().acquire().await.map_err(storage)?;
        find_job_by_id(&mut connection, job_id).await
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn get_job_by_tool_call(
        &self,
        workspace_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        sqlx::query_as::<_, DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
             FROM device_jobs WHERE workspace_id = ? AND tool_call_id = ?",
        )
        .bind(workspace_id)
        .bind(tool_call_id)
        .fetch_optional(self.pool())
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn get_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        sqlx::query_as::<_, DeviceJobRow>(
            "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                    progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
             FROM device_jobs WHERE workspace_id = ? AND task_id = ? AND id = ?",
        )
        .bind(workspace_id)
        .bind(task_id)
        .bind(job_id)
        .fetch_optional(self.pool())
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
        self.mark_running_and_event(job_id, driver_call_id)
            .await
            .map(|(job, _)| job)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn mark_running_and_event(
        &self,
        job_id: &str,
        driver_call_id: &str,
    ) -> Result<(DeviceJob, crate::runtime_event::RuntimeEvent), DeviceJobError> {
        let mut tx = self.pool().begin().await.map_err(storage)?;
        let updated = sqlx::query_as::<_, DeviceJobRow>(
            "UPDATE device_jobs SET state = 'running', driver_call_id = ?
             WHERE id = ? AND state = 'accepted'
             RETURNING id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                       progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at",
        )
        .bind(driver_call_id)
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()?;
        if let Some(job) = updated {
            let event = append_runtime_event_tx(
                &mut tx,
                job_event(&job, "running", serde_json::json!({"state":"running"})),
            )
            .await
            .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            return Ok((job, event));
        }
        let current = find_job_by_id(&mut tx, job_id).await?.ok_or(DeviceJobError::NotFound)?;
        Err(DeviceJobError::Conflict(format!(
            "job '{}' is {} rather than accepted",
            current.id,
            current.state.as_str()
        )))
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn finish_job(
        &self,
        job_id: &str,
        state: DeviceJobState,
        result: &Value,
        wake: Option<&TaskWake>,
    ) -> Result<bool, DeviceJobError> {
        self.finish_job_and_event(job_id, state, result, wake)
            .await
            .map(|(finished, _)| finished)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn finish_job_and_event(
        &self,
        job_id: &str,
        state: DeviceJobState,
        result: &Value,
        wake: Option<&TaskWake>,
    ) -> Result<(bool, Option<crate::runtime_event::RuntimeEvent>), DeviceJobError> {
        if !state.is_terminal() {
            return Err(DeviceJobError::Validation(
                "terminal result has an active state".to_string(),
            ));
        }
        let mut tx = self.pool().begin().await.map_err(storage)?;
        // Write first: the UPDATE acquires RESERVED before any read, avoiding
        // the deferred SHARED -> RESERVED upgrade that fails immediately.
        let updated_job = sqlx::query_as::<_, DeviceJobRow>(
            "UPDATE device_jobs SET state = ?, result_json = ?, ended_at = ?
             WHERE id = ? AND state IN ('accepted', 'running')
             RETURNING id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                       progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
                       timeout_ms, started_at, ended_at",
        )
        .bind(state.as_str())
        .bind(serde_json::to_string(result).map_err(storage)?)
        .bind(Utc::now())
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()?;
        let Some(updated_job) = updated_job else {
            let current = find_job_by_id(&mut tx, job_id).await?.ok_or(DeviceJobError::NotFound)?;
            if current.state.is_terminal() {
                return Ok((false, None));
            }
            return Err(DeviceJobError::Conflict(format!(
                "job '{}' is {} rather than terminalizable",
                current.id,
                current.state.as_str()
            )));
        };
        if let Some(wake) = wake {
            validate_job_wake(&updated_job, wake)?;
            insert_job_wake(&mut tx, wake).await?;
        }
        let event = append_runtime_event_tx(
            &mut tx,
            job_event(
                &updated_job,
                state.as_str(),
                serde_json::json!({"state":state.as_str()}),
            ),
        )
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok((true, Some(event)))
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn update_progress(&self, job_id: &str, progress: &Value) -> Result<bool, DeviceJobError> {
        Ok(self.update_progress_revision(job_id, progress).await?.is_some())
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn update_progress_revision(
        &self,
        job_id: &str,
        progress: &Value,
    ) -> Result<Option<i64>, DeviceJobError> {
        self.update_progress_revision_and_event(job_id, progress)
            .await
            .map(|(revision, _)| revision)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn update_progress_revision_and_event(
        &self,
        job_id: &str,
        progress: &Value,
    ) -> Result<(Option<i64>, Option<crate::runtime_event::RuntimeEvent>), DeviceJobError> {
        let progress_json = serde_json::to_string(progress).map_err(storage)?;
        let mut tx = self.pool().begin().await.map_err(storage)?;
        let updated = sqlx::query(
            "UPDATE device_jobs
             SET progress_json = ?, progress_revision = progress_revision + 1
             WHERE id = ? AND state IN ('accepted', 'running')
               AND (progress_json IS NULL OR progress_json != ?)",
        )
        .bind(&progress_json)
        .bind(job_id)
        .bind(&progress_json)
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected();
        if updated == 0 {
            if find_job_by_id(&mut tx, job_id).await?.is_none() {
                return Err(DeviceJobError::NotFound);
            }
            tx.commit().await.map_err(storage)?;
            return Ok((None, None));
        }
        let job = find_job_by_id(&mut tx, job_id).await?.ok_or(DeviceJobError::NotFound)?;
        let revision = sqlx::query_scalar::<_, i64>("SELECT progress_revision FROM device_jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        let mut input = job_event(
            &job,
            &format!("progress:v{revision}"),
            serde_json::json!({"revision":revision,"progress":progress}),
        );
        input.event_type = "job.progress".into();
        let event = append_runtime_event_tx(&mut tx, input).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok((Some(revision), Some(event)))
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn request_stop(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<DeviceJob, DeviceJobError> {
        self.request_stop_and_event(workspace_id, task_id, job_id)
            .await
            .map(|(job, _)| job)
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn request_stop_and_event(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<(DeviceJob, crate::runtime_event::RuntimeEvent), DeviceJobError> {
        let mut tx = self.pool().begin().await.map_err(storage)?;
        let updated = sqlx::query_as::<_, DeviceJobRow>(
            "UPDATE device_jobs SET stop_requested_at = COALESCE(stop_requested_at, ?)
             WHERE id = ? AND workspace_id = ? AND task_id = ? AND state IN ('accepted', 'running')
             RETURNING id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                       progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at",
        )
        .bind(Utc::now())
        .bind(job_id)
        .bind(workspace_id)
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .map(DeviceJob::try_from)
        .transpose()?;
        let job = match updated {
            Some(job) => job,
            None => sqlx::query_as::<_, DeviceJobRow>(
                "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                        progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
                 FROM device_jobs WHERE id = ? AND workspace_id = ? AND task_id = ?",
            )
            .bind(job_id)
            .bind(workspace_id)
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
            .map(DeviceJob::try_from)
            .transpose()?
            .ok_or(DeviceJobError::NotFound)?,
        };
        let event = append_runtime_event_tx(
            &mut tx,
            job_event(
                &job,
                "stop_requested",
                serde_json::json!({"stopRequestedAt":job.stop_requested_at}),
            ),
        )
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok((job, event))
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn active_jobs(
        &self,
        workspace_id: Option<&str>,
        unknown_only: bool,
    ) -> Result<Vec<DeviceJob>, DeviceJobError> {
        let rows = if unknown_only {
            sqlx::query_as::<_, DeviceJobRow>(
                "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                        progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
                 FROM device_jobs
                 WHERE (? IS NULL OR workspace_id = ?)
                   AND state IN ('cancelled_unknown', 'interrupted')
                 ORDER BY started_at, id",
            )
            .bind(workspace_id)
            .bind(workspace_id)
            .fetch_all(self.pool())
            .await
        } else {
            sqlx::query_as::<_, DeviceJobRow>(
                "SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
                        progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at
                 FROM device_jobs
                 WHERE (? IS NULL OR workspace_id = ?) AND state IN ('accepted', 'running')
                 ORDER BY started_at, id",
            )
            .bind(workspace_id)
            .bind(workspace_id)
            .fetch_all(self.pool())
            .await
        }
        .map_err(storage)?;
        rows.into_iter().map(DeviceJob::try_from).collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};

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
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 't1', 'now', 'now')")
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("seed workspace");
    }

    /// Seed the retired platform Task tables directly. The platform Task
    /// writer was deleted in Task 19C4; these fixtures only exercise the
    /// test-gated platform Job writer's foreign-key references.
    async fn seed_task(db: &Db, task_id: &str, workspace_id: &str) {
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, created_by, status,
                  state_version, created_at, updated_at)
             VALUES (?, ?, ?, 'exercise device jobs', 'user_1', 'active', 0, 'now', 'now')",
        )
        .bind(task_id)
        .bind(workspace_id)
        .bind(format!("key_{task_id}"))
        .execute(db.pool())
        .await
        .expect("seed task");
        sqlx::query(
            "INSERT INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at)
             VALUES (?, ?, ?, 'pending', 'now')",
        )
        .bind(task_id)
        .bind(format!("create_{task_id}"))
        .bind(json!({"workspace_id":workspace_id}).to_string())
        .execute(db.pool())
        .await
        .expect("seed task wake");
    }

    fn sample_job(job_id: &str, task_id: &str) -> DeviceJob {
        DeviceJob {
            id: job_id.to_string(),
            workspace_id: "ws_1".to_string(),
            device_id: "dev_1".to_string(),
            tool: "set_speed".to_string(),
            args_json: json!({"speed":2}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(task_id.to_string()),
            run_id: Some("run_1".to_string()),
            tool_call_id: Some(format!("call_{job_id}")),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    fn result_wake(job_id: &str, task_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.to_string(),
            event_id: format!("job:{job_id}:terminal"),
            signal_json: json!({"workspace_id":"ws_1","job_id":job_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    async fn fixture() -> Db {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = Db::new(pool);
        seed_task(&db, "task_1", "ws_1").await;
        seed_task(&db, "task_2", "ws_1").await;
        db
    }

    #[tokio::test]
    async fn tool_call_identity_is_unique_within_a_workspace_only() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        seed_workspace(&pool, "ws_other").await;
        let db = Db::new(pool);
        seed_task(&db, "task_1", "ws_1").await;
        seed_task(&db, "task_other", "ws_other").await;

        let mut first = sample_job("job_a", "task_1");
        first.tool_call_id = Some("call_shared".into());
        let mut second = first.clone();
        second.id = "job_b".into();
        second.workspace_id = "ws_other".into();
        second.task_id = Some("task_other".into());
        db.create_job(&first).await.expect("first workspace job");
        db.create_job(&second)
            .await
            .expect("same tool call in another workspace");

        let scoped = db
            .get_job_by_tool_call("ws_1", "call_shared")
            .await
            .expect("scoped lookup")
            .expect("job");
        assert_eq!(scoped.id, "job_a");
        let scoped = db
            .get_job_by_tool_call("ws_other", "call_shared")
            .await
            .expect("scoped lookup")
            .expect("job");
        assert_eq!(scoped.id, "job_b");

        let mut duplicate = first.clone();
        duplicate.id = "job_dupe".into();
        assert!(
            db.create_job(&duplicate).await.is_err(),
            "duplicate tool call in one workspace must fail"
        );
    }

    #[tokio::test]
    async fn terminal_fact_and_wake_commit_once() {
        let db = fixture().await;
        let job = sample_job("job_1", "task_1");
        db.create_job(&job).await.expect("create job");
        assert!(
            db.finish_job(
                &job.id,
                DeviceJobState::Done,
                &json!({"ok":true}),
                Some(&result_wake(&job.id, "task_1"))
            )
            .await
            .expect("first terminal")
        );
        assert!(
            !db.finish_job(
                &job.id,
                DeviceJobState::Done,
                &json!({"ok":true}),
                Some(&result_wake(&job.id, "task_1"))
            )
            .await
            .expect("duplicate terminal")
        );
        let stored = db.get_job("ws_1", &job.id).await.expect("get").expect("job");
        assert_eq!(stored.state, DeviceJobState::Done);
        assert!(stored.ended_at.is_some());
        let wakes: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = 'task_1' AND event_id = 'job:job_1:terminal'",
        )
        .fetch_one(db.pool())
        .await
        .expect("wake count");
        assert_eq!(wakes, 1);
    }

    #[tokio::test]
    async fn invalid_wake_rolls_back_terminal_fact() {
        let db = fixture().await;
        let job = sample_job("job_rollback", "task_1");
        db.create_job(&job).await.expect("create job");
        let error = db
            .finish_job(
                &job.id,
                DeviceJobState::Done,
                &json!({"ok":true}),
                Some(&result_wake(&job.id, "task_2")),
            )
            .await
            .expect_err("mismatched wake must fail");
        assert!(matches!(error, DeviceJobError::Validation(_)));
        assert_eq!(
            db.get_job("ws_1", &job.id).await.expect("get").expect("job").state,
            DeviceJobState::Accepted
        );
    }

    #[tokio::test]
    async fn persistent_stop_is_owned_active_and_idempotent() {
        let db = fixture().await;
        let job = sample_job("job_stop", "task_1");
        let other = sample_job("job_other", "task_2");
        db.create_job(&job).await.expect("create job");
        db.create_job(&other).await.expect("create other");
        let first = db.request_stop("ws_1", "task_1", &job.id).await.expect("stop");
        let second = db.request_stop("ws_1", "task_1", &job.id).await.expect("repeat");
        assert_eq!(first.stop_requested_at, second.stop_requested_at);
        assert!(first.stop_requested_at.is_some());
        assert!(matches!(
            db.request_stop("ws_1", "task_2", &job.id).await,
            Err(DeviceJobError::NotFound)
        ));
        assert_eq!(
            db.get_job("ws_1", &other.id)
                .await
                .expect("get")
                .expect("other")
                .stop_requested_at,
            None
        );
        db.finish_job(&job.id, DeviceJobState::Cancelled, &json!({}), None)
            .await
            .expect("finish");
        let terminal = db.request_stop("ws_1", "task_1", &job.id).await.expect("terminal stop");
        assert_eq!(terminal.stop_requested_at, first.stop_requested_at);
    }

    #[tokio::test]
    async fn live_job_events_carry_tool_arguments_and_terminal_result() {
        let db = fixture().await;
        let job = sample_job("job_live_details", "task_1");
        let accepted = db.create_job_and_event(&job).await.expect("accepted event");
        assert_eq!(accepted.data["state"], "accepted");
        assert_eq!(accepted.data["tool"], "set_speed");
        assert_eq!(accepted.data["argsJson"], json!({"speed":2}));

        let (_, terminal) = db
            .finish_job_and_event(&job.id, DeviceJobState::Done, &json!({"actualSpeed":2}), None)
            .await
            .expect("terminal event");
        let terminal = terminal.expect("new terminal event");
        assert_eq!(terminal.data["state"], "done");
        assert_eq!(terminal.data["tool"], "set_speed");
        assert_eq!(terminal.data["argsJson"], json!({"speed":2}));
        assert_eq!(terminal.data["resultJson"], json!({"actualSpeed":2}));
        assert_eq!(
            db.runtime_event_by_id("ws_1", &terminal.event_id)
                .await
                .expect("persisted event")
                .expect("event exists")
                .data,
            terminal.data
        );
        let stored = db.get_job("ws_1", &job.id).await.expect("reload job").expect("job");
        assert_eq!(stored.tool, "set_speed");
        assert_eq!(stored.args_json, json!({"speed":2}));
        assert_eq!(stored.result_json, Some(json!({"actualSpeed":2})));
    }

    #[tokio::test]
    async fn running_and_active_queries_preserve_recovery_states() {
        let db = fixture().await;
        let job = sample_job("job_active", "task_1");
        db.create_job(&job).await.expect("create job");
        let running = db.mark_running(&job.id, "driver_1").await.expect("running");
        assert_eq!(running.state, DeviceJobState::Running);
        assert_eq!(running.driver_call_id.as_deref(), Some("driver_1"));
        assert!(matches!(
            db.mark_running(&job.id, "driver_2").await,
            Err(DeviceJobError::Conflict(_))
        ));
        assert!(
            db.update_progress(&job.id, &json!({"message":"working"}))
                .await
                .expect("progress")
        );
        assert_eq!(db.active_jobs(None, false).await.expect("active").len(), 1);
        db.finish_job(&job.id, DeviceJobState::CancelledUnknown, &json!({}), None)
            .await
            .expect("unknown");
        assert!(db.active_jobs(None, false).await.expect("active").is_empty());
        let interrupted = sample_job("job_interrupted", "task_1");
        db.create_job(&interrupted).await.expect("interrupted job");
        db.finish_job(
            &interrupted.id,
            DeviceJobState::Interrupted,
            &json!({"reason":"worker call lost"}),
            None,
        )
        .await
        .expect("interrupted");
        let unknown = db.active_jobs(None, true).await.expect("unknown");
        assert_eq!(unknown.len(), 2, "interrupted Jobs stay visible for manual disposition");
        assert!(unknown.iter().any(|job| job.id == "job_interrupted"));
    }
}
