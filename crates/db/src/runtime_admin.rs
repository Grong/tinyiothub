//! Runtime drain readiness, rollout gauges, and unknown-Job dispositions.
//!
//! Rollout drain observes durable rows only. It never changes Task status in
//! bulk: cancellation still goes through the existing per-Task Cancel path.
//!
//! Task/Run/Wake facts are workspace-Harness facts (Task 19C4). Every count
//! below sweeps the owning workspace generations; an unreadable workspace is
//! skipped with a warning, matching the 19C3 Job sweep policy. The retired
//! platform `agent_tasks` / `agent_runs` / `task_wake_outbox` / `device_jobs`
//! tables have no production reader or writer.

use chrono::DateTime;
use chrono::Utc;
use serde::Serialize;
use tinyiothub_core::agent_task::WakeDispatchState;
use tinyiothub_core::device_job::DeviceJobError;

use crate::Db;
use crate::harness_task::status_from_state;
use crate::runtime_event::AdminWake;

/// Workspace-outbox gauges for one or all workspaces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceJobWakeCounts {
    pub pending: u64,
    pub dispatched: u64,
    pub dead: u64,
    pub oldest_pending_age_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDrainCounts {
    pub nonterminal_tasks: u64,
    pub active_runs: u64,
    pub active_jobs: u64,
    pub active_watches: u64,
    pub pending_approvals: u64,
    pub pending_outbox: u64,
    pub unresolved_unknown: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGauges {
    pub active: u64,
    pub waiting_job: u64,
    pub waiting_watch: u64,
    pub needs_approval: u64,
    pub paused: u64,
    pub cancelling: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGauges {
    pub pending: u64,
    pub approved: u64,
    pub rejected: u64,
    pub expired: u64,
    pub unavailable: u64,
    pub oldest_pending_age_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxGauges {
    pub pending: u64,
    pub dispatched: u64,
    pub dead: u64,
    pub oldest_pending_age_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobGauges {
    pub accepted: u64,
    pub running: u64,
    pub done: u64,
    pub failed: u64,
    pub rejected: u64,
    pub cancelled: u64,
    pub cancelled_unknown: u64,
    pub interrupted: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchGauges {
    pub active: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGauges {
    pub count: u64,
    pub bytes: u64,
    pub expired: u64,
}

/// Durable gauges for rollout dashboards. These are observations, not quotas.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetrics {
    pub drain: RuntimeDrainCounts,
    pub tasks: TaskGauges,
    pub approvals: ApprovalGauges,
    pub outbox: OutboxGauges,
    pub jobs: JobGauges,
    pub watches: WatchGauges,
    pub evidence: EvidenceGauges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownDispositionOutcome {
    Recorded,
    AlreadyRecorded,
    NotFound,
    NotUnknown,
}

fn storage(error: impl std::fmt::Display) -> DeviceJobError {
    DeviceJobError::Storage(error.to_string())
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

const NONTERMINAL: &str = "state NOT IN ('completed', 'failed', 'cancelled')";

async fn wake_counts_pool(
    pool: &sqlx::SqlitePool,
    kind: Option<&str>,
) -> Result<WorkspaceJobWakeCounts, DeviceJobError> {
    let sql = if kind.is_some() {
        "SELECT
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'pending' AND w.resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'dispatched' AND w.resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'dead' THEN 1 ELSE 0 END), 0),
           COALESCE(MAX(CASE WHEN w.dispatch_state = 'pending' AND w.resolved_at IS NULL
             THEN CAST(strftime('%s', 'now') - strftime('%s', w.created_at) AS INTEGER) END), 0)
         FROM task_wake_outbox w
         JOIN agent_tasks t ON t.id = w.task_id AND t.execution_mode = 'production'
         WHERE json_extract(w.signal_json, '$.kind') = ?"
    } else {
        "SELECT
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'pending' AND w.resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'dispatched' AND w.resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN w.dispatch_state = 'dead' THEN 1 ELSE 0 END), 0),
           COALESCE(MAX(CASE WHEN w.dispatch_state = 'pending' AND w.resolved_at IS NULL
             THEN CAST(strftime('%s', 'now') - strftime('%s', w.created_at) AS INTEGER) END), 0)
         FROM task_wake_outbox w
         JOIN agent_tasks t ON t.id = w.task_id AND t.execution_mode = 'production'"
    };
    let mut query = sqlx::query_as::<_, (i64, i64, i64, i64)>(sql);
    if let Some(kind) = kind {
        query = query.bind(kind);
    }
    let (pending, dispatched, dead, oldest_pending) = query.fetch_one(pool).await.map_err(storage)?;
    Ok(WorkspaceJobWakeCounts {
        pending: count(pending),
        dispatched: count(dispatched),
        dead: count(dead),
        oldest_pending_age_secs: count(oldest_pending.max(0)),
    })
}

async fn task_run_counts_pool(pool: &sqlx::SqlitePool) -> Result<(i64, i64), DeviceJobError> {
    let statement = format!(
        "SELECT
           (SELECT COUNT(*) FROM agent_tasks WHERE execution_mode = 'production' AND {NONTERMINAL}),
           (SELECT COUNT(*) FROM agent_tasks
             WHERE execution_mode = 'production' AND run_id IS NOT NULL AND {NONTERMINAL})"
    );
    sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(statement))
        .fetch_one(pool)
        .await
        .map_err(storage)
}

async fn task_state_counts_pool(pool: &sqlx::SqlitePool) -> Result<Vec<(String, i64)>, DeviceJobError> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT state, COUNT(*) FROM agent_tasks WHERE execution_mode = 'production' GROUP BY state",
    )
    .fetch_all(pool)
    .await
    .map_err(storage)?;
    let mut counts = Vec::new();
    for (state, value) in rows {
        if let Some(status) = status_from_state(&state) {
            counts.push((status.as_str().to_string(), value));
        }
    }
    Ok(counts)
}

async fn wake_state_counts_pool(pool: &sqlx::SqlitePool) -> Result<Vec<(String, i64)>, DeviceJobError> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT w.dispatch_state, COUNT(*)
         FROM task_wake_outbox w
         JOIN agent_tasks t ON t.id = w.task_id AND t.execution_mode = 'production'
         WHERE w.dispatch_state = 'dead' OR w.resolved_at IS NULL
         GROUP BY w.dispatch_state",
    )
    .fetch_all(pool)
    .await
    .map_err(storage)
}

async fn active_watch_count_pool(db: &Db, workspace_id: &str, pool: &sqlx::SqlitePool) -> Result<i64, DeviceJobError> {
    let statement = format!("SELECT id FROM agent_tasks WHERE execution_mode = 'production' AND {NONTERMINAL}");
    let task_ids: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(storage)?;
    if task_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; task_ids.len()].join(", ");
    let statement = format!(
        "SELECT COUNT(*) FROM watch_rules
         WHERE workspace_id = ? AND owner_task_id IN ({placeholders})"
    );
    let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(statement)).bind(workspace_id);
    for task_id in task_ids {
        query = query.bind(task_id);
    }
    query.fetch_one(db.pool()).await.map_err(storage)
}

async fn dead_wakes_pool(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    limit: i64,
) -> Result<Vec<AdminWake>, DeviceJobError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
        ),
    >(
        "SELECT w.task_id, w.event_id, w.signal_json, w.dispatch_state,
                w.created_at, w.dispatched_at, w.resolved_at
         FROM task_wake_outbox w
         JOIN agent_tasks t ON t.id = w.task_id AND t.execution_mode = 'production'
         WHERE w.dispatch_state = 'dead'
         ORDER BY w.created_at, w.task_id, w.event_id LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(storage)?;
    rows.into_iter()
        .map(
            |(task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)| {
                Ok(AdminWake {
                    workspace_id: workspace_id.to_string(),
                    task_id,
                    event_id,
                    signal_json: serde_json::from_str(&signal_json)
                        .map_err(|error| DeviceJobError::Storage(error.to_string()))?,
                    dispatch_state: WakeDispatchState::from_db(&dispatch_state)
                        .ok_or_else(|| DeviceJobError::Storage(format!("unknown wake state: {dispatch_state}")))?,
                    created_at,
                    dispatched_at,
                    resolved_at,
                })
            },
        )
        .collect()
}

/// A scoped read must never fall back to the fleet when the requested
/// workspace has no harness generation: `None` (fleet) and `Some(ws)` with no
/// generation are different answers.
enum ScopedPool {
    Missing,
    Fleet,
    Available(sqlx::SqlitePool),
}

async fn scope_pool(db: &Db, workspace_id: Option<&str>) -> Result<ScopedPool, DeviceJobError> {
    match workspace_id {
        Some(workspace_id) => Ok(
            match db.try_existing_workspace_pool(workspace_id).await.map_err(storage)? {
                Some(pool) => ScopedPool::Available(pool),
                None => ScopedPool::Missing,
            },
        ),
        None => Ok(ScopedPool::Fleet),
    }
}

async fn sweep_workspaces(db: &Db, workspace_id: Option<&str>) -> Result<Vec<String>, DeviceJobError> {
    match workspace_id {
        Some(workspace_id) => Ok(vec![workspace_id.to_string()]),
        None => db.harness_workspaces().await.map_err(storage),
    }
}

impl Db {
    /// All Task/Job wakes in the workspace outbox. Diagnostics never create an
    /// empty workspace generation (Task 19C3 Low 2).
    pub async fn workspace_wake_counts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<WorkspaceJobWakeCounts, DeviceJobError> {
        match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => wake_counts_pool(&pool, None).await,
            ScopedPool::Missing => Ok(WorkspaceJobWakeCounts::default()),
            ScopedPool::Fleet => {
                let mut totals = WorkspaceJobWakeCounts::default();
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match wake_counts_pool(&pool, None).await {
                        Ok(counts) => {
                            totals.pending += counts.pending;
                            totals.dispatched += counts.dispatched;
                            totals.dead += counts.dead;
                            totals.oldest_pending_age_secs =
                                totals.oldest_pending_age_secs.max(counts.oldest_pending_age_secs);
                        }
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace wake gauge failed");
                        }
                    }
                }
                Ok(totals)
            }
        }
    }

    /// Job terminal wakes in the workspace outbox (Job diagnostic view).
    pub async fn workspace_job_wake_counts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<WorkspaceJobWakeCounts, DeviceJobError> {
        match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => wake_counts_pool(&pool, Some("device_job_terminal")).await,
            ScopedPool::Missing => Ok(WorkspaceJobWakeCounts::default()),
            ScopedPool::Fleet => {
                let mut totals = WorkspaceJobWakeCounts::default();
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match wake_counts_pool(&pool, Some("device_job_terminal")).await {
                        Ok(counts) => {
                            totals.pending += counts.pending;
                            totals.dispatched += counts.dispatched;
                            totals.dead += counts.dead;
                            totals.oldest_pending_age_secs =
                                totals.oldest_pending_age_secs.max(counts.oldest_pending_age_secs);
                        }
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace job wake gauge failed");
                        }
                    }
                }
                Ok(totals)
            }
        }
    }

    /// Dead wakes from the workspace outboxes. Bounded like the platform
    /// query it replaces.
    pub async fn workspace_dead_wakes(
        &self,
        workspace_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AdminWake>, DeviceJobError> {
        let limit = limit.max(0);
        let mut wakes = Vec::new();
        match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => {
                wakes = dead_wakes_pool(&pool, workspace_id.unwrap_or_default(), limit).await?;
            }
            ScopedPool::Missing => {}
            ScopedPool::Fleet => {
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match dead_wakes_pool(&pool, &workspace_id, limit).await {
                        Ok(mut found) => wakes.append(&mut found),
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace dead wake scan failed");
                        }
                    }
                }
            }
        }
        wakes.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.task_id.cmp(&right.task_id))
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        wakes.truncate(limit as usize);
        Ok(wakes)
    }

    /// Retry one dead workspace wake. Returns `false` when the workspace has no
    /// generation or the event is not a dead wake there.
    pub async fn retry_workspace_dead_wake(&self, workspace_id: &str, event_id: &str) -> Result<bool, DeviceJobError> {
        let Some(pool) = self.try_existing_workspace_pool(workspace_id).await.map_err(storage)? else {
            return Ok(false);
        };
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
        .map_err(storage)?
        .rows_affected();
        Ok(affected > 0)
    }

    pub async fn runtime_drain_counts(&self, workspace_id: Option<&str>) -> Result<RuntimeDrainCounts, DeviceJobError> {
        let (nonterminal_tasks, active_runs) = match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => task_run_counts_pool(&pool).await?,
            ScopedPool::Missing => (0, 0),
            ScopedPool::Fleet => {
                let mut totals = (0i64, 0i64);
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match task_run_counts_pool(&pool).await {
                        Ok((tasks, runs)) => {
                            totals.0 += tasks;
                            totals.1 += runs;
                        }
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace task gauge failed");
                        }
                    }
                }
                totals
            }
        };
        let active_watches = match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => {
                active_watch_count_pool(self, workspace_id.unwrap_or_default(), &pool).await?
            }
            ScopedPool::Missing => 0,
            ScopedPool::Fleet => {
                let mut total = 0i64;
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match active_watch_count_pool(self, &workspace_id, &pool).await {
                        Ok(value) => total += value,
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace watch gauge failed");
                        }
                    }
                }
                total
            }
        };
        // Jobs are workspace facts since 19C3.
        let (active_jobs, unresolved_unknown) = self.workspace_job_counts(workspace_id).await?;
        let wakes = self.workspace_wake_counts(workspace_id).await?;
        Ok(RuntimeDrainCounts {
            nonterminal_tasks: count(nonterminal_tasks),
            active_runs: count(active_runs),
            active_jobs,
            active_watches: count(active_watches),
            // Workspace Decisions are the only approval source (19C2). The
            // drain field stays for API compatibility; a requested approval is
            // an Effect blocker, not a Task/Run/Wake fact this query owns.
            pending_approvals: 0,
            pending_outbox: wakes.pending,
            unresolved_unknown,
        })
    }

    pub async fn runtime_metrics(&self, workspace_id: Option<&str>) -> Result<RuntimeMetrics, DeviceJobError> {
        let drain = self.runtime_drain_counts(workspace_id).await?;

        let mut task_rows: Vec<(String, i64)> = Vec::new();
        match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => task_rows = task_state_counts_pool(&pool).await?,
            ScopedPool::Missing => {}
            ScopedPool::Fleet => {
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match task_state_counts_pool(&pool).await {
                        Ok(mut rows) => task_rows.append(&mut rows),
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace task metrics failed");
                        }
                    }
                }
            }
        }
        let mut outbox_rows: Vec<(String, i64)> = Vec::new();
        match scope_pool(self, workspace_id).await? {
            ScopedPool::Available(pool) => outbox_rows = wake_state_counts_pool(&pool).await?,
            ScopedPool::Missing => {}
            ScopedPool::Fleet => {
                for workspace_id in sweep_workspaces(self, None).await? {
                    let Ok(Some(pool)) = self.try_existing_workspace_pool(&workspace_id).await else {
                        continue;
                    };
                    match wake_state_counts_pool(&pool).await {
                        Ok(mut rows) => outbox_rows.append(&mut rows),
                        Err(error) => {
                            tracing::warn!(%workspace_id, %error, "workspace outbox metrics failed");
                        }
                    }
                }
            }
        }
        // Jobs are workspace facts since 19C3.
        let job_rows = self.workspace_job_state_counts(workspace_id).await?;
        let wakes = self.workspace_wake_counts(workspace_id).await?;
        let (watch_active, watch_total): (i64, i64) = sqlx::query_as(
            "SELECT
               COALESCE(SUM(CASE WHEN owner_task_id IS NOT NULL THEN 1 ELSE 0 END), 0),
               COUNT(*)
             FROM watch_rules WHERE (? IS NULL OR workspace_id = ?)",
        )
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .map_err(storage)?;
        let (evidence_count, evidence_bytes, expired_evidence): (i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0),
                    COALESCE(SUM(CASE WHEN expires_at <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now') THEN 1 ELSE 0 END), 0)
             FROM media_evidence WHERE (? IS NULL OR workspace_id = ?)",
        )
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .map_err(storage)?;

        let mut tasks = TaskGauges::default();
        for (state, value) in task_rows {
            match state.as_str() {
                "active" => tasks.active = count(value),
                "waiting_job" => tasks.waiting_job = count(value),
                "waiting_watch" => tasks.waiting_watch = count(value),
                "needs_approval" => tasks.needs_approval = count(value),
                "paused" => tasks.paused = count(value),
                "cancelling" => tasks.cancelling = count(value),
                "completed" => tasks.completed = count(value),
                "failed" => tasks.failed = count(value),
                "cancelled" => tasks.cancelled = count(value),
                _ => {}
            }
        }
        let approvals = ApprovalGauges::default();
        let mut outbox = OutboxGauges {
            oldest_pending_age_secs: wakes.oldest_pending_age_secs,
            ..OutboxGauges::default()
        };
        for (state, value) in outbox_rows {
            match state.as_str() {
                "pending" => outbox.pending = count(value),
                "dispatched" => outbox.dispatched = count(value),
                "dead" => outbox.dead = count(value),
                _ => {}
            }
        }
        let mut jobs = JobGauges::default();
        for (state, value) in job_rows {
            match state.as_str() {
                "accepted" => jobs.accepted = count(value),
                "running" => jobs.running = count(value),
                "done" => jobs.done = count(value),
                "failed" => jobs.failed = count(value),
                "rejected" => jobs.rejected = count(value),
                "cancelled" => jobs.cancelled = count(value),
                "cancelled_unknown" => jobs.cancelled_unknown = count(value),
                "interrupted" => jobs.interrupted = count(value),
                _ => {}
            }
        }
        Ok(RuntimeMetrics {
            drain,
            tasks,
            approvals,
            outbox,
            jobs,
            watches: WatchGauges {
                active: count(watch_active),
                total: count(watch_total),
            },
            evidence: EvidenceGauges {
                count: count(evidence_count),
                bytes: count(evidence_bytes),
                expired: count(expired_evidence),
            },
        })
    }

    /// Record an administrator's physical disposition for a Job whose physical
    /// outcome is unknown (`cancelled_unknown` or `interrupted`). The Job row
    /// itself is not rewritten and no side effect is replayed.
    pub async fn record_unknown_disposition(
        &self,
        workspace_id: &str,
        job_id: &str,
        disposition: &str,
        recorded_by: &str,
    ) -> Result<UnknownDispositionOutcome, DeviceJobError> {
        self.record_workspace_unknown_disposition(workspace_id, job_id, disposition, recorded_by)
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use sqlx::SqlitePool;
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};

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
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES (?, 'workspace', 't1', 'now', 'now')",
        )
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    }

    fn task(workspace_id: &str, id: &str) -> AgentTask {
        let now = Utc::now();
        AgentTask {
            id: id.into(),
            workspace_id: workspace_id.into(),
            idempotency_key: format!("key-{id}"),
            objective: "inspect".into(),
            session_key: None,
            created_by: "user_1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        }
    }

    fn wake(workspace_id: &str, task_id: &str, event_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.into(),
            event_id: event_id.into(),
            signal_json: json!({"workspace_id": workspace_id, "task_id": task_id}).to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn job(workspace_id: &str, task_id: &str) -> DeviceJob {
        DeviceJob {
            id: "job_ws".into(),
            workspace_id: workspace_id.into(),
            device_id: "device-1".into(),
            tool: "set_speed".into(),
            args_json: json!({"speed": 2}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some(task_id.into()),
            run_id: None,
            tool_call_id: Some("call_ws".into()),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    fn job_wake(workspace_id: &str, task_id: &str) -> TaskWake {
        TaskWake {
            task_id: task_id.into(),
            event_id: "device_job:job_ws:done".into(),
            signal_json: json!({
                "kind": "device_job_terminal",
                "workspace_id": workspace_id,
                "job_id": "job_ws"
            })
            .to_string(),
            dispatch_state: WakeDispatchState::Pending,
            created_at: Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        }
    }

    fn test_db(pool: SqlitePool) -> Db {
        let root = std::env::temp_dir().join(format!("tinyiothub-runtime-admin-{}", uuid::Uuid::new_v4()));
        Db::new(pool).with_workspace_store_root(root.join("data/agents"))
    }

    async fn seed_task(db: &Db, workspace_id: &str, task_id: &str, run: bool) {
        db.harness_create_task_with_wake(
            &task(workspace_id, task_id),
            &wake(workspace_id, task_id, &format!("{task_id}-wake")),
        )
        .await
        .expect("task");
        if run {
            let current = db
                .harness_get_task(workspace_id, task_id)
                .await
                .expect("get")
                .expect("task");
            db.harness_begin_run(&current, &format!("{task_id}-wake"), &format!("run-{task_id}"))
                .await
                .expect("begin run");
        }
    }

    async fn seed_owned_watch(pool: &SqlitePool, workspace_id: &str, id: &str, owner_task_id: &str) {
        sqlx::query(
            "INSERT INTO watch_rules
                 (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
             VALUES (?, ?, 'availability', '{\"thing_id\":\"dev_1\",\"online\":true}', 0, 60, 'now', ?)",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(owner_task_id)
        .execute(pool)
        .await
        .expect("watch");
    }

    async fn seed_dead_job_wake(db: &Db, workspace_id: &str, task_id: &str) {
        db.create_workspace_job(&job(workspace_id, task_id)).await.expect("job");
        db.finish_workspace_job(
            workspace_id,
            "job_ws",
            DeviceJobState::Done,
            &json!({"ok": true}),
            Some(&job_wake(workspace_id, task_id)),
        )
        .await
        .expect("finish");
        db.harness_mark_wake_dead(task_id, "device_job:job_ws:done", "dispatch_failed")
            .await
            .expect("dead wake");
    }

    #[tokio::test]
    async fn drain_and_metrics_count_workspace_tasks_runs_wakes_and_owned_watches() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = test_db(pool.clone());
        db.create_workspace_store("ws_1").await.expect("workspace store");

        seed_task(&db, "ws_1", "task_live", true).await;
        seed_task(&db, "ws_1", "task_done", false).await;
        let cancelling = db
            .harness_transition_task("ws_1", "task_done", 0, TaskStatus::Cancelling, None)
            .await
            .expect("cancelling");
        db.harness_transition_task(
            "ws_1",
            "task_done",
            cancelling.state_version,
            TaskStatus::Cancelled,
            None,
        )
        .await
        .expect("cancel terminal");
        seed_owned_watch(&pool, "ws_1", "watch_live", "task_live").await;
        seed_owned_watch(&pool, "ws_1", "watch_done", "task_done").await;
        seed_dead_job_wake(&db, "ws_1", "task_live").await;

        let counts = db.runtime_drain_counts(Some("ws_1")).await.expect("drain");
        assert_eq!(counts.nonterminal_tasks, 1);
        assert_eq!(counts.active_runs, 1);
        assert_eq!(
            counts.active_watches, 1,
            "a watch owned by a terminal Task must not keep the drain open"
        );
        assert_eq!(counts.pending_approvals, 0);
        assert_eq!(
            counts.pending_outbox, 1,
            "the unresolved task_done wake pins the drain; the dead Job wake does not"
        );
        assert_eq!(counts.active_jobs, 0);
        assert_eq!(counts.unresolved_unknown, 0);

        let metrics = db.runtime_metrics(Some("ws_1")).await.expect("metrics");
        assert_eq!(metrics.tasks.active, 1);
        assert_eq!(metrics.tasks.cancelled, 1);
        assert_eq!(metrics.outbox.dead, 1, "the dead workspace Job wake is visible");
        assert_eq!(metrics.watches.active, 2);
        assert_eq!(metrics.watches.total, 2);
        assert_eq!(metrics.drain, counts);
        let _ = tokio::fs::remove_dir_all(db.workspace_store_root.parent().expect("root")).await;
    }

    #[tokio::test]
    async fn workspace_dead_wakes_are_listed_and_retried() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let db = test_db(pool);
        db.create_workspace_store("ws_1").await.expect("workspace store");
        seed_task(&db, "ws_1", "task_live", false).await;
        seed_dead_job_wake(&db, "ws_1", "task_live").await;

        let counts = db.workspace_job_wake_counts(Some("ws_1")).await.expect("counts");
        assert_eq!(counts.dead, 1);
        let dead = db.workspace_dead_wakes(Some("ws_1"), 200).await.expect("list");
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].event_id, "device_job:job_ws:done");
        assert_eq!(dead[0].workspace_id, "ws_1");

        assert!(
            db.retry_workspace_dead_wake("ws_1", "device_job:job_ws:done")
                .await
                .expect("retry")
        );
        assert!(
            !db.retry_workspace_dead_wake("ws_1", "device_job:job_ws:done")
                .await
                .expect("second retry is not dead")
        );
        let pending = db.workspace_job_wake_counts(Some("ws_1")).await.expect("counts");
        assert_eq!((pending.pending, pending.dead), (1, 0));
        let _ = tokio::fs::remove_dir_all(db.workspace_store_root.parent().expect("root")).await;
    }

    #[tokio::test]
    async fn scoped_reads_do_not_fall_back_to_the_fleet_when_the_store_is_missing() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_empty").await;
        seed_workspace(&pool, "ws_data").await;
        let db = test_db(pool);
        // ws_data owns a Task; ws_empty is registered but never created a
        // Harness generation.
        seed_task(&db, "ws_data", "task_data", false).await;

        let drain = db.runtime_drain_counts(Some("ws_empty")).await.expect("scoped drain");
        assert_eq!(drain.nonterminal_tasks, 0, "must not see another workspace");
        assert_eq!(drain.active_runs, 0);
        assert_eq!(drain.active_watches, 0);
        assert_eq!(drain.pending_outbox, 0);
        assert_eq!(drain.active_jobs, 0);
        let metrics = db.runtime_metrics(Some("ws_empty")).await.expect("scoped metrics");
        assert_eq!(metrics.tasks.active, 0);
        assert_eq!(metrics.outbox.pending, 0);
        assert_eq!(metrics.jobs.accepted, 0);
        let wakes = db.workspace_wake_counts(Some("ws_empty")).await.expect("scoped wakes");
        assert_eq!(wakes.pending, 0);
        let dead = db
            .workspace_dead_wakes(Some("ws_empty"), 200)
            .await
            .expect("scoped dead");
        assert!(dead.is_empty());

        // The fleet-wide path still sees the readable workspace.
        let fleet = db.runtime_drain_counts(None).await.expect("fleet drain");
        assert_eq!(fleet.nonterminal_tasks, 1);
        let _ = tokio::fs::remove_dir_all(db.workspace_store_root.parent().expect("root")).await;
    }

    #[tokio::test]
    async fn runtime_sweeps_degrade_per_unreadable_workspace() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_good").await;
        seed_workspace(&pool, "ws_bad").await;
        let db = test_db(pool);
        db.create_workspace_store("ws_good").await.expect("good store");
        db.create_workspace_store("ws_bad").await.expect("bad store");
        seed_task(&db, "ws_good", "task_good", false).await;

        db.close_workspace_store("ws_bad").await.expect("close bad store");
        let path = db.workspace_store_root.join("ws_bad").join("harness/harness.db");
        tokio::fs::write(&path, b"not a sqlite database")
            .await
            .expect("corrupt store");
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.clone().into_os_string();
            sidecar.push(suffix);
            let _ = tokio::fs::remove_file(sidecar).await;
        }

        let counts = db
            .runtime_drain_counts(None)
            .await
            .expect("unscoped sweep degrades instead of failing");
        assert_eq!(counts.nonterminal_tasks, 1, "the readable workspace still counts");
        let metrics = db.runtime_metrics(None).await.expect("unscoped metrics degrade");
        assert_eq!(metrics.tasks.active, 1);
        assert!(
            db.runtime_drain_counts(Some("ws_bad")).await.is_err(),
            "a scoped read of an unreadable workspace fails closed"
        );
        let _ = tokio::fs::remove_dir_all(db.workspace_store_root.parent().expect("root")).await;
    }
}
