//! Runtime drain readiness, rollout gauges, and unknown-Job dispositions.
//!
//! Rollout drain observes durable rows only. It never changes Task status in
//! bulk: cancellation still goes through the existing per-Task Cancel path.

use chrono::DateTime;
use chrono::Utc;
use serde::Serialize;
use tinyiothub_core::agent_task::WakeDispatchState;
use tinyiothub_core::device_job::DeviceJobError;

use crate::Db;
use crate::runtime_event::AdminWake;

/// Workspace-outbox gauges for wakes written by the Job lifecycle.
///
/// The 19B Harness Task-outbox counters are still platform-only; this task
/// only guarantees that the `device_job_terminal` wake it moved is visible to
/// drain/metrics before 19C4 deletes the platform tables.
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
///
/// These queries read the platform runtime tables. Harness simulation
/// Tasks/Runs live in the separate per-workspace `harness.db`, so they are
/// never counted here; production Harness lists/recovery use their own
/// `execution_mode='production'` filters.
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

/// Count only the Job terminal wakes in a workspace outbox. Other Harness
/// Task wakes stay out of these gauges until 19C4 migrates the 19B counters.
async fn job_wake_counts_pool(pool: &sqlx::SqlitePool) -> Result<WorkspaceJobWakeCounts, DeviceJobError> {
    let (pending, dispatched, dead, oldest_pending): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           COALESCE(SUM(CASE WHEN dispatch_state = 'pending' AND resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN dispatch_state = 'dispatched' AND resolved_at IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN dispatch_state = 'dead' THEN 1 ELSE 0 END), 0),
           COALESCE(MAX(CASE WHEN dispatch_state = 'pending' AND resolved_at IS NULL
             THEN CAST(strftime('%s', 'now') - strftime('%s', created_at) AS INTEGER) END), 0)
         FROM task_wake_outbox
         WHERE json_extract(signal_json, '$.kind') = 'device_job_terminal'",
    )
    .fetch_one(pool)
    .await
    .map_err(storage)?;
    Ok(WorkspaceJobWakeCounts {
        pending: count(pending),
        dispatched: count(dispatched),
        dead: count(dead),
        oldest_pending_age_secs: count(oldest_pending.max(0)),
    })
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
        "SELECT task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at
         FROM task_wake_outbox
         WHERE dispatch_state = 'dead'
         ORDER BY created_at, task_id, event_id LIMIT ?",
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

async fn status_counts(
    db: &Db,
    sql: &'static str,
    workspace_id: Option<&str>,
) -> Result<Vec<(String, i64)>, DeviceJobError> {
    sqlx::query_as::<_, (String, i64)>(sql)
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_all(db.pool())
        .await
        .map_err(storage)
}

impl Db {
    /// Job terminal wakes in the workspace outbox. Diagnostics never create an
    /// empty workspace generation (Task 19C3 Low 2).
    pub async fn workspace_job_wake_counts(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<WorkspaceJobWakeCounts, DeviceJobError> {
        match workspace_id {
            Some(workspace_id) => {
                let Some(pool) = self.try_existing_workspace_pool(workspace_id).await.map_err(storage)? else {
                    return Ok(WorkspaceJobWakeCounts::default());
                };
                job_wake_counts_pool(&pool).await
            }
            None => {
                let mut totals = WorkspaceJobWakeCounts::default();
                for workspace_id in self.harness_workspaces().await.map_err(storage)? {
                    let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
                        continue;
                    };
                    match job_wake_counts_pool(&pool).await {
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

    /// Dead wakes from the workspace outboxes, next to the legacy platform
    /// list. Bounded like the platform query.
    pub async fn workspace_dead_wakes(
        &self,
        workspace_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AdminWake>, DeviceJobError> {
        let limit = limit.max(0);
        let mut wakes = Vec::new();
        match workspace_id {
            Some(workspace_id) => {
                let Some(pool) = self.try_existing_workspace_pool(workspace_id).await.map_err(storage)? else {
                    return Ok(wakes);
                };
                wakes = dead_wakes_pool(&pool, workspace_id, limit).await?;
            }
            None => {
                for workspace_id in self.harness_workspaces().await.map_err(storage)? {
                    let Some(pool) = self.try_existing_workspace_pool(&workspace_id).await.ok().flatten() else {
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
             WHERE event_id = ? AND dispatch_state = 'dead'",
        )
        .bind(event_id)
        .execute(&pool)
        .await
        .map_err(storage)?
        .rows_affected();
        Ok(affected > 0)
    }

    pub async fn runtime_drain_counts(&self, workspace_id: Option<&str>) -> Result<RuntimeDrainCounts, DeviceJobError> {
        let (nonterminal_tasks, active_runs, active_watches, pending_outbox): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT COUNT(*) FROM agent_tasks t
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM agent_tasks t
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND t.current_run_id IS NOT NULL
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM watch_rules w
                 JOIN agent_tasks t ON t.id = w.owner_task_id
                 WHERE (? IS NULL OR w.workspace_id = ?) AND w.owner_task_id IS NOT NULL
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM task_wake_outbox o
                 JOIN agent_tasks t ON t.id = o.task_id
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND o.dispatch_state = 'pending' AND o.resolved_at IS NULL)",
        )
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .map_err(storage)?;
        // Jobs are workspace facts since 19C3; the platform table has no writer.
        let (active_jobs, unresolved_unknown) = self.workspace_job_counts(workspace_id).await?;
        // The terminal wake this task moved also lives in the workspace
        // outbox; the platform query cannot see it (Task 19C3 M1).
        let job_wakes = self.workspace_job_wake_counts(workspace_id).await?;
        Ok(RuntimeDrainCounts {
            nonterminal_tasks: count(nonterminal_tasks),
            active_runs: count(active_runs),
            active_jobs,
            active_watches: count(active_watches),
            // The platform task_approvals table has no writers since 19C2.
            // Counting a pre-upgrade orphan would pin the drain forever.
            pending_approvals: 0,
            pending_outbox: count(pending_outbox) + job_wakes.pending,
            unresolved_unknown,
        })
    }

    pub async fn runtime_metrics(&self, workspace_id: Option<&str>) -> Result<RuntimeMetrics, DeviceJobError> {
        let drain = self.runtime_drain_counts(workspace_id).await?;
        let task_rows = status_counts(
            self,
            "SELECT status, COUNT(*) FROM agent_tasks
             WHERE (? IS NULL OR workspace_id = ?) GROUP BY status",
            workspace_id,
        )
        .await?;
        // Writer-less platform approvals are not live work (19C2). The
        // dashboard keeps the field for API compatibility but never counts
        // pre-upgrade orphans.
        let approval_rows: Vec<(String, i64)> = Vec::new();
        let outbox_rows = status_counts(
            self,
            "SELECT o.dispatch_state, COUNT(*) FROM task_wake_outbox o
             JOIN agent_tasks t ON t.id = o.task_id
             WHERE (? IS NULL OR t.workspace_id = ?)
               AND (o.dispatch_state = 'dead' OR o.resolved_at IS NULL)
             GROUP BY o.dispatch_state",
            workspace_id,
        )
        .await?;
        // Jobs are workspace facts since 19C3.
        let job_rows = self.workspace_job_state_counts(workspace_id).await?;
        let job_wakes = self.workspace_job_wake_counts(workspace_id).await?;
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
        let oldest_pending_approval: i64 = 0;
        let oldest_pending_outbox: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(0, CAST(strftime('%s', 'now') - strftime('%s', MIN(o.created_at)) AS INTEGER)), 0)
             FROM task_wake_outbox o JOIN agent_tasks t ON t.id = o.task_id
             WHERE (? IS NULL OR t.workspace_id = ?)
               AND o.dispatch_state = 'pending' AND o.resolved_at IS NULL",
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
        let mut approvals = ApprovalGauges {
            oldest_pending_age_secs: count(oldest_pending_approval),
            ..ApprovalGauges::default()
        };
        for (state, value) in approval_rows {
            match state.as_str() {
                "pending" => approvals.pending = count(value),
                "approved" => approvals.approved = count(value),
                "rejected" => approvals.rejected = count(value),
                "expired" => approvals.expired = count(value),
                "unavailable" => approvals.unavailable = count(value),
                _ => {}
            }
        }
        let mut outbox = OutboxGauges {
            oldest_pending_age_secs: count(oldest_pending_outbox),
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
        // The Job terminal wake lives in the workspace outbox (19C3 M1).
        outbox.pending += job_wakes.pending;
        outbox.dispatched += job_wakes.dispatched;
        outbox.dead += job_wakes.dead;
        outbox.oldest_pending_age_secs = outbox.oldest_pending_age_secs.max(job_wakes.oldest_pending_age_secs);
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
    use tinyiothub_core::agent_task::{TaskWake, WakeDispatchState};
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

    async fn seed_task(pool: &SqlitePool, workspace_id: &str, id: &str, status: &str) {
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, created_by, status,
                  state_version, created_at, updated_at)
             VALUES (?, ?, ?, 'objective', 'user_1', ?, 1, 'now', 'now')",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(format!("key_{id}"))
        .bind(status)
        .execute(pool)
        .await
        .expect("task");
    }

    async fn seed_watch(pool: &SqlitePool, workspace_id: &str, id: &str, owner_task_id: &str) {
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

    /// Seed a terminal workspace Job with a pending Job wake. The drain must
    /// count the wake from the workspace outbox because the platform outbox
    /// has no production writer (Task 19C3 M1).
    async fn seed_terminal_job_wake(db: &Db, workspace_id: &str, task_id: &str) {
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
    }

    #[tokio::test]
    async fn drain_counts_include_live_work_and_exclude_terminal_owner_watches() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        seed_task(&pool, "ws_1", "task_live", "active").await;
        seed_task(&pool, "ws_1", "task_done", "completed").await;
        seed_watch(&pool, "ws_1", "watch_live", "task_live").await;
        seed_watch(&pool, "ws_1", "watch_done", "task_done").await;

        let root = std::env::temp_dir().join(format!("tinyiothub-runtime-admin-{}", uuid::Uuid::new_v4()));
        let mut db = Db::new(pool);
        db.workspace_store_root = root.join("data/agents");
        db.create_workspace_store("ws_1").await.expect("workspace store");
        seed_terminal_job_wake(&db, "ws_1", "task_live").await;

        let counts = db.runtime_drain_counts(Some("ws_1")).await.expect("drain");
        let _ = tokio::fs::remove_dir_all(&root).await;
        assert_eq!(counts.nonterminal_tasks, 1);
        assert_eq!(
            counts.active_watches, 1,
            "a watch owned by a terminal Task must not keep the drain open"
        );
        assert_eq!(
            counts.pending_approvals, 0,
            "no production code writes the platform approval table"
        );
        assert_eq!(
            counts.pending_outbox, 1,
            "the pending device_job_terminal wake in the workspace outbox must pin the drain"
        );
        assert_eq!(counts.active_jobs, 0);
        assert_eq!(counts.unresolved_unknown, 0);
    }

    #[tokio::test]
    async fn workspace_dead_wakes_are_listed_and_retried() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        let root = std::env::temp_dir().join(format!("tinyiothub-runtime-admin-{}", uuid::Uuid::new_v4()));
        let mut db = Db::new(pool);
        db.workspace_store_root = root.join("data/agents");
        db.create_workspace_store("ws_1").await.expect("workspace store");
        db.create_workspace_job(&job("ws_1", "task_live")).await.expect("job");
        let workspace_pool = db.workspace_pool("ws_1").await.expect("workspace pool");
        sqlx::query(
            "INSERT INTO task_wake_outbox
                 (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
             VALUES ('task_live', 'device_job:job_ws:done', ?, 'dead',
                     '2026-10-29T00:00:00Z', NULL, '2026-10-29T00:00:01Z')",
        )
        .bind(job_wake("ws_1", "task_live").signal_json)
        .execute(&workspace_pool)
        .await
        .expect("dead wake");

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
        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}
