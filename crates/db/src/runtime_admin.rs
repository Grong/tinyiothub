//! Runtime drain readiness, rollout gauges, and unknown-Job dispositions.
//!
//! Rollout drain observes durable rows only. It never changes Task status in
//! bulk: cancellation still goes through the existing per-Task Cancel path.

use serde::Serialize;
use tinyiothub_core::device_job::DeviceJobError;

use crate::Db;

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
    pub async fn runtime_drain_counts(&self, workspace_id: Option<&str>) -> Result<RuntimeDrainCounts, DeviceJobError> {
        let (
            nonterminal_tasks,
            active_runs,
            active_jobs,
            active_watches,
            pending_approvals,
            pending_outbox,
            unresolved_unknown,
        ): (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT COUNT(*) FROM agent_tasks t
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM agent_tasks t
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND t.current_run_id IS NOT NULL
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM device_jobs j
                 WHERE (? IS NULL OR j.workspace_id = ?) AND j.state IN ('accepted', 'running')),
               (SELECT COUNT(*) FROM watch_rules w
                 JOIN agent_tasks t ON t.id = w.owner_task_id
                 WHERE (? IS NULL OR w.workspace_id = ?) AND w.owner_task_id IS NOT NULL
                   AND t.status NOT IN ('completed', 'failed', 'cancelled')),
               (SELECT COUNT(*) FROM task_approvals a
                 WHERE (? IS NULL OR a.workspace_id = ?) AND a.state = 'pending'),
               (SELECT COUNT(*) FROM task_wake_outbox o
                 JOIN agent_tasks t ON t.id = o.task_id
                 WHERE (? IS NULL OR t.workspace_id = ?)
                   AND o.dispatch_state = 'pending' AND o.resolved_at IS NULL),
               (SELECT COUNT(*) FROM device_jobs j
                 WHERE (? IS NULL OR j.workspace_id = ?)
                   AND j.state IN ('cancelled_unknown', 'interrupted')
                   AND NOT EXISTS (
                     SELECT 1 FROM runtime_unknown_dispositions d WHERE d.job_id = j.id
                   ))",
        )
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(workspace_id)
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
        Ok(RuntimeDrainCounts {
            nonterminal_tasks: count(nonterminal_tasks),
            active_runs: count(active_runs),
            active_jobs: count(active_jobs),
            active_watches: count(active_watches),
            pending_approvals: count(pending_approvals),
            pending_outbox: count(pending_outbox),
            unresolved_unknown: count(unresolved_unknown),
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
        let approval_rows = status_counts(
            self,
            "SELECT state, COUNT(*) FROM task_approvals
             WHERE (? IS NULL OR workspace_id = ?) GROUP BY state",
            workspace_id,
        )
        .await?;
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
        let job_rows = status_counts(
            self,
            "SELECT state, COUNT(*) FROM device_jobs
             WHERE (? IS NULL OR workspace_id = ?) GROUP BY state",
            workspace_id,
        )
        .await?;
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
        let oldest_pending_approval: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(0, CAST(strftime('%s', 'now') - strftime('%s', MIN(requested_at)) AS INTEGER)), 0)
             FROM task_approvals WHERE (? IS NULL OR workspace_id = ?) AND state = 'pending'",
        )
        .bind(workspace_id)
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .map_err(storage)?;
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
        // Write-first: the INSERT ... SELECT atomically validates the Job state
        // and records the disposition without holding a read snapshot first.
        let affected = sqlx::query(
            "INSERT OR IGNORE INTO runtime_unknown_dispositions
                 (job_id, workspace_id, task_id, device_id, disposition, recorded_by, recorded_at)
             SELECT id, workspace_id, task_id, device_id, ?, ?, ?
             FROM device_jobs
             WHERE workspace_id = ? AND id = ? AND state IN ('cancelled_unknown', 'interrupted')",
        )
        .bind(disposition)
        .bind(recorded_by)
        .bind(chrono::Utc::now())
        .bind(workspace_id)
        .bind(job_id)
        .execute(self.pool())
        .await
        .map_err(storage)?
        .rows_affected();
        if affected == 1 {
            let device_id: Option<String> =
                sqlx::query_scalar("SELECT device_id FROM runtime_unknown_dispositions WHERE job_id = ?")
                    .bind(job_id)
                    .fetch_optional(self.pool())
                    .await
                    .map_err(storage)?;
            tracing::info!(
                metric = "runtime_unknown_disposition",
                %workspace_id,
                %job_id,
                device_id = ?device_id,
                "unknown Job disposition recorded"
            );
            return Ok(UnknownDispositionOutcome::Recorded);
        }
        let row: Option<(String,)> = sqlx::query_as("SELECT state FROM device_jobs WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(job_id)
            .fetch_optional(self.pool())
            .await
            .map_err(storage)?;
        let Some((state,)) = row else {
            return Ok(UnknownDispositionOutcome::NotFound);
        };
        if state != "cancelled_unknown" && state != "interrupted" {
            return Ok(UnknownDispositionOutcome::NotUnknown);
        }
        Ok(UnknownDispositionOutcome::AlreadyRecorded)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

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

    #[tokio::test]
    async fn drain_counts_include_live_work_and_exclude_terminal_owner_watches() {
        let pool = crate::test_helpers::test_pool().await;
        seed_workspace(&pool, "ws_1").await;
        seed_task(&pool, "ws_1", "task_live", "active").await;
        seed_task(&pool, "ws_1", "task_done", "completed").await;
        seed_watch(&pool, "ws_1", "watch_live", "task_live").await;
        seed_watch(&pool, "ws_1", "watch_done", "task_done").await;
        sqlx::query(
            "INSERT INTO task_approvals
                 (id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json, reason,
                  state, decision, decider_id, requested_at, decided_at, expires_at, decision_event_id, updated_at)
             VALUES ('approval_1', 'ws_1', 'task_live', 'run_1', 'call_1', 'dev_1', 'set_speed', '{}',
                     'confirm', 'pending', NULL, NULL, '2026-10-29T00:00:00Z', NULL,
                     '2026-10-30T00:00:00Z', 'decision_1', '2026-10-29T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("approval");
        for (event_id, resolved_at) in [("event_live", None), ("event_resolved", Some("2026-10-29T00:00:00Z"))] {
            sqlx::query(
                "INSERT INTO task_wake_outbox
                     (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
                 VALUES ('task_live', ?, '{\"workspace_id\":\"ws_1\"}', 'pending',
                         '2026-10-29T00:00:00Z', NULL, ?)",
            )
            .bind(event_id)
            .bind(resolved_at)
            .execute(&pool)
            .await
            .expect("wake");
        }

        let counts = Db::new(pool).runtime_drain_counts(Some("ws_1")).await.expect("drain");
        assert_eq!(counts.nonterminal_tasks, 1);
        assert_eq!(
            counts.active_watches, 1,
            "a watch owned by a terminal Task must not keep the drain open"
        );
        assert_eq!(counts.pending_approvals, 1);
        assert_eq!(counts.pending_outbox, 1);
        assert_eq!(counts.unresolved_unknown, 0);
    }
}
