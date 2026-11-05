//! Supervisor health current-state row and dead-wake recovery facts (Task 14).
//!
//! `supervisor_state` is a single mutable row per workspace store. Normal
//! scheduler ticks advance it; only stalled/recovered transitions append
//! permanent `session_events`. Dead wake retry reuses the original event row,
//! so confirmation is always identity-preserving and idempotent.

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::SqlSafeStr;
use tinyiothub_core::harness::HarnessEventInput;

use crate::{Db, error::Result};

pub(crate) fn migration() -> sqlx::migrate::Migration {
    sqlx::migrate::Migration::new(
        20261103000008,
        "harness_supervisor".into(),
        sqlx::migrate::MigrationType::Simple,
        include_str!("../migrations/20261103000008_harness_supervisor.sql").into_sql_str(),
        false,
    )
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SupervisorStateRow {
    pub tick_seq: i64,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub stalled_reason: Option<String>,
}

/// One enabled cron Workflow's persisted scheduling boundary. A missing
/// `next_slot` means the scheduler has never initialized this plan.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SupervisorWorkflowScheduleRow {
    pub workflow_id: String,
    pub trigger_id: String,
    pub enabled_at: DateTime<Utc>,
    pub next_slot: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupervisorMetricsRow {
    pub enabled_workflows: i64,
    pub pending_wakes: i64,
    pub oldest_pending_at: Option<DateTime<Utc>>,
    pub active_mains: i64,
    pub active_subagents: i64,
    pub schedules: Vec<SupervisorWorkflowScheduleRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct HarnessDeadWakeRow {
    pub task_id: String,
    pub event_id: String,
    pub signal_json: String,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub retried_by: Option<String>,
    pub retried_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadWakeRetryOutcome {
    /// The original dead row became pending again.
    Retried,
    /// The original row was already pending; this is the same durable fact.
    AlreadyPending,
    /// No dead or pending row with this event identity exists.
    NotFound,
}

impl Db {
    pub async fn supervisor_state(&self, workspace_id: &str) -> Result<SupervisorStateRow> {
        let pool = self.workspace_pool(workspace_id).await?;
        let row = sqlx::query_as::<_, SupervisorStateRow>(
            "SELECT tick_seq, last_started_at, last_completed_at, last_error, stalled_reason
             FROM supervisor_state WHERE id = 1",
        )
        .fetch_one(&pool)
        .await?;
        Ok(row)
    }

    /// Advance the single current-state row. No permanent event is written on
    /// a normal tick.
    pub async fn begin_supervisor_tick(&self, workspace_id: &str, started_at: DateTime<Utc>) -> Result<i64> {
        let pool = self.workspace_pool(workspace_id).await?;
        let tick_seq = sqlx::query_scalar::<_, i64>(
            "UPDATE supervisor_state
             SET tick_seq = tick_seq + 1, last_started_at = ?
             WHERE id = 1
             RETURNING tick_seq",
        )
        .bind(timestamp(started_at))
        .fetch_one(&pool)
        .await?;
        Ok(tick_seq)
    }

    /// A successful tick clears the last error. Recovery from a stalled state
    /// is decided by [`Db::record_supervisor_transition`].
    pub async fn complete_supervisor_tick(&self, workspace_id: &str, completed_at: DateTime<Utc>) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query(
            "UPDATE supervisor_state
             SET last_completed_at = ?, last_error = NULL
             WHERE id = 1",
        )
        .bind(timestamp(completed_at))
        .execute(&pool)
        .await?;
        Ok(())
    }

    /// A failed tick still completed, but leaves a bounded recent-error fact.
    pub async fn fail_supervisor_tick(
        &self,
        workspace_id: &str,
        completed_at: DateTime<Utc>,
        error: &str,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        sqlx::query(
            "UPDATE supervisor_state
             SET last_completed_at = ?, last_error = ?
             WHERE id = 1",
        )
        .bind(timestamp(completed_at))
        .bind(error)
        .execute(&pool)
        .await?;
        Ok(())
    }

    /// Persist a stalled/recovered transition and its audit event atomically.
    /// `event` is only supplied when the persisted reason actually changes.
    pub async fn record_supervisor_transition(
        &self,
        workspace_id: &str,
        stalled_reason: Option<&str>,
        event: Option<&HarnessEventInput>,
    ) -> Result<()> {
        let pool = self.workspace_pool(workspace_id).await?;
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE supervisor_state SET stalled_reason = ? WHERE id = 1")
            .bind(stalled_reason)
            .execute(&mut *tx)
            .await?;
        if let Some(event) = event {
            crate::harness_effect::insert_event(&mut tx, event).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn supervisor_metrics(&self, workspace_id: &str) -> Result<SupervisorMetricsRow> {
        let pool = self.workspace_pool(workspace_id).await?;
        let (enabled_workflows, pending_wakes, oldest_pending_at, active_mains, active_subagents): (
            i64,
            i64,
            Option<DateTime<Utc>>,
            i64,
            i64,
        ) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM agent_workflows WHERE enabled = 1),
                (SELECT COUNT(*) FROM task_wake_outbox WHERE dispatch_state = 'pending'),
                (SELECT MIN(created_at) FROM task_wake_outbox WHERE dispatch_state = 'pending'),
                (SELECT COUNT(*) FROM agent_tasks
                  WHERE execution_mode = 'production'
                    AND parent_task_id IS NULL
                    AND state NOT IN ('completed', 'failed', 'cancelled')),
                (SELECT COUNT(*) FROM agent_tasks
                  WHERE execution_mode = 'production'
                    AND parent_task_id IS NOT NULL
                    AND state NOT IN ('completed', 'failed', 'cancelled'))",
        )
        .fetch_one(&pool)
        .await?;
        let schedules = sqlx::query_as::<_, SupervisorWorkflowScheduleRow>(
            "SELECT w.workflow_id,
                    json_extract(r.definition_json, '$.trigger.trigger_id') AS trigger_id,
                    w.updated_at AS enabled_at,
                    c.next_slot
             FROM agent_workflows w
             JOIN workflow_revisions r
               ON r.workflow_id = w.workflow_id AND r.version = w.current_version
             LEFT JOIN workflow_cron_cursors c
               ON c.workflow_id = w.workflow_id
              AND c.trigger_id = json_extract(r.definition_json, '$.trigger.trigger_id')
             WHERE w.enabled = 1
               AND json_extract(r.definition_json, '$.trigger.type') = 'cron'
             ORDER BY w.workflow_id",
        )
        .fetch_all(&pool)
        .await?;
        Ok(SupervisorMetricsRow {
            enabled_workflows,
            pending_wakes,
            oldest_pending_at,
            active_mains,
            active_subagents,
            schedules,
        })
    }

    /// Bounded dead-wake page for the Supervisor detail. Dead rows retain the
    /// original event identity and target Task.
    pub async fn harness_dead_wakes(&self, workspace_id: &str, limit: i64) -> Result<Vec<HarnessDeadWakeRow>> {
        let pool = self.workspace_pool(workspace_id).await?;
        let rows = sqlx::query_as::<_, HarnessDeadWakeRow>(
            "SELECT task_id, event_id, signal_json, failure_reason, created_at,
                    retried_by, retried_at
             FROM task_wake_outbox
             WHERE dispatch_state = 'dead'
             ORDER BY created_at, event_id
             LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(&pool)
        .await?;
        Ok(rows)
    }

    /// Retry exactly the supplied original event ID. The outbox primary key
    /// stays unchanged; no replacement event or bulk mutation is possible.
    pub async fn retry_harness_dead_wake(
        &self,
        workspace_id: &str,
        event_id: &str,
        actor: &str,
        now: DateTime<Utc>,
    ) -> Result<DeadWakeRetryOutcome> {
        let pool = self.workspace_pool(workspace_id).await?;
        let affected = sqlx::query(
            "UPDATE task_wake_outbox
             SET dispatch_state = 'pending', dispatched_at = NULL, resolved_at = NULL,
                 retried_by = ?, retried_at = ?
             WHERE event_id = ? AND dispatch_state = 'dead'",
        )
        .bind(actor)
        .bind(timestamp(now))
        .bind(event_id)
        .execute(&pool)
        .await?
        .rows_affected();
        if affected > 0 {
            tracing::info!(
                metric = "harness_dead_wake_retried",
                %workspace_id,
                %event_id,
                %actor,
                "Harness dead wake retried"
            );
            return Ok(DeadWakeRetryOutcome::Retried);
        }
        let state = sqlx::query_scalar::<_, String>("SELECT dispatch_state FROM task_wake_outbox WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(&pool)
            .await?;
        match state.as_deref() {
            Some("pending") => Ok(DeadWakeRetryOutcome::AlreadyPending),
            _ => Ok(DeadWakeRetryOutcome::NotFound),
        }
    }
}
