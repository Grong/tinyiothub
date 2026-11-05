//! Bounded Supervisor health and in-context dead-wake recovery (Spec T17).
//!
//! Health is derived from the single mutable `supervisor_state` row plus
//! durable Workflow/wake/Task facts. AI heartbeat is never process liveness.
//! In-process observations use a monotonic anchor so a wall-clock jump cannot
//! fake a tick deadline; persisted UTC is still compared after restart to
//! detect a real rollback. Normal ticks never append permanent events — only
//! `stalled`/`recovered` transitions do.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::Digest;
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput, WorkflowTrigger};
use tinyiothub_storage::Db;
use tinyiothub_storage::error::DbError;
use tinyiothub_storage::harness_supervisor::{
    DeadWakeRetryOutcome, HarnessDeadWakeRow, SupervisorMetricsRow, SupervisorStateRow,
};

pub use tinyiothub_storage::harness_supervisor::DeadWakeRetryOutcome as RetryOutcome;

use super::next_wake_at;

/// A tick that starts and does not finish within this window is stalled.
pub const DEFAULT_TICK_DEADLINE: Duration = Duration::from_secs(30);
/// A planned boundary may be late by this much before it is called stalled.
pub const DEFAULT_DUE_GRACE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorStatus {
    /// No enabled Workflow and no active work: a normal idle scheduler.
    Idle,
    /// Ticks are current and the next planned wake is in the future.
    Healthy,
    /// A concrete stalled/degraded fact exists.
    Stalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StalledReason {
    TickDeadlineExceeded,
    DueNotStarted,
    ClockRollback,
}

impl StalledReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TickDeadlineExceeded => "tick_deadline_exceeded",
            Self::DueNotStarted => "due_not_started",
            Self::ClockRollback => "clock_rollback",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::TickDeadlineExceeded => "调度器停滞：tick 超过截止时间未完成",
            Self::DueNotStarted => "调度器停滞：计划唤醒到期未开始",
            Self::ClockRollback => "调度器停滞：持久时间回拨",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorHealthSnapshot {
    pub status: SupervisorStatus,
    pub stalled_reason: Option<StalledReason>,
    pub stalled_label: Option<&'static str>,
    pub tick_seq: i64,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_completed_at: Option<DateTime<Utc>>,
    /// Planned time only. It is never a promise of on-time execution.
    pub next_wake_at: Option<DateTime<Utc>>,
    pub oldest_pending_at: Option<DateTime<Utc>>,
    pub scheduling_delay_seconds: i64,
    pub active_mains: i64,
    pub active_subagents: i64,
    pub enabled_workflows: i64,
    pub pending_wakes: i64,
    pub last_error: Option<String>,
    /// True when no Main/Subagent is active. This is deliberately separate
    /// from `stalled`: "no new Runs" is not a scheduler failure.
    pub no_new_runs: bool,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadWakeSummary {
    pub event_id: String,
    pub task_id: String,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub retried_by: Option<String>,
    pub retried_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct SupervisorHealth {
    db: Db,
    workspace_id: String,
    tick_deadline: Duration,
    due_grace: Duration,
    wall_anchor: DateTime<Utc>,
    monotonic_anchor: Instant,
}

impl SupervisorHealth {
    pub fn new(db: Db, workspace_id: impl Into<String>) -> Self {
        Self::with_limits(db, workspace_id, DEFAULT_TICK_DEADLINE, DEFAULT_DUE_GRACE)
    }

    pub fn with_limits(db: Db, workspace_id: impl Into<String>, tick_deadline: Duration, due_grace: Duration) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
            tick_deadline,
            due_grace,
            wall_anchor: Utc::now(),
            monotonic_anchor: Instant::now(),
        }
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Production clock: wall time advanced from a monotonic anchor, so a
    /// forward/backward wall-clock jump inside the process cannot change
    /// tick-deadline or due-boundary math.
    pub fn monotonic_now(&self) -> DateTime<Utc> {
        self.wall_anchor + chrono::Duration::from_std(self.monotonic_anchor.elapsed()).unwrap_or_default()
    }

    pub async fn record_tick_started(&self, now: DateTime<Utc>) -> Result<i64, DbError> {
        self.db.begin_supervisor_tick(&self.workspace_id, now).await
    }

    pub async fn record_tick_completed(&self, now: DateTime<Utc>) -> Result<(), DbError> {
        self.db.complete_supervisor_tick(&self.workspace_id, now).await
    }

    pub async fn record_tick_error(&self, now: DateTime<Utc>, error: &str) -> Result<(), DbError> {
        self.db.fail_supervisor_tick(&self.workspace_id, now, error).await
    }

    /// Read-only health snapshot. It never writes an event.
    pub async fn snapshot(&self) -> Result<SupervisorHealthSnapshot, DbError> {
        self.snapshot_at(self.monotonic_now()).await
    }

    /// Explicit-time read-only evaluation used by diagnostics and tests.
    pub async fn snapshot_at(&self, now: DateTime<Utc>) -> Result<SupervisorHealthSnapshot, DbError> {
        let state = self.db.supervisor_state(&self.workspace_id).await?;
        let metrics = self.db.supervisor_metrics(&self.workspace_id).await?;
        let definitions = self.db.list_enabled_workflows(&self.workspace_id).await?;
        let reason = self.evaluate(&state, &metrics, now);
        Ok(self.snapshot_from(&state, &metrics, &definitions, reason, now))
    }

    /// Evaluate and persist a stalled/recovered transition. Only the
    /// transition writes a `session_events` fact.
    pub async fn observe(&self) -> Result<SupervisorHealthSnapshot, DbError> {
        self.observe_at(self.monotonic_now()).await
    }

    pub async fn observe_at(&self, now: DateTime<Utc>) -> Result<SupervisorHealthSnapshot, DbError> {
        let state = self.db.supervisor_state(&self.workspace_id).await?;
        let metrics = self.db.supervisor_metrics(&self.workspace_id).await?;
        let definitions = self.db.list_enabled_workflows(&self.workspace_id).await?;
        let reason = self.evaluate(&state, &metrics, now);

        let previous = state.stalled_reason.as_deref();
        let next = reason.map(StalledReason::as_str);
        if previous != next {
            let event = if previous.is_some() != next.is_some() {
                Some(self.transition_event(&state, previous, next, reason, now)?)
            } else {
                None
            };
            self.db
                .record_supervisor_transition(&self.workspace_id, next, event.as_ref())
                .await?;
        }
        Ok(self.snapshot_from(&state, &metrics, &definitions, reason, now))
    }

    pub async fn dead_wakes(&self, limit: i64) -> Result<Vec<DeadWakeSummary>, DbError> {
        let rows = self.db.harness_dead_wakes(&self.workspace_id, limit).await?;
        Ok(rows.into_iter().map(DeadWakeSummary::from).collect())
    }

    /// Confirm the original dead wake only. The outbox identity is
    /// `(task_id, event_id)`; the original event identity is reused, so a
    /// repeated confirmation returns the same pending fact and no bulk or
    /// replacement retry path exists. A terminal/cancelling/interrupted target
    /// Task is refused because its wake may have died from cancellation.
    pub async fn retry_dead_wake(
        &self,
        workspace_id: &str,
        task_id: &str,
        event_id: &str,
        actor: &str,
    ) -> Result<DeadWakeRetryOutcome, DbError> {
        if workspace_id != self.workspace_id {
            return Ok(DeadWakeRetryOutcome::NotFound);
        }
        if actor.trim().is_empty() {
            return Err(DbError::Validation {
                message: "retry actor must not be blank".to_string(),
            });
        }
        self.db
            .retry_harness_dead_wake(workspace_id, task_id, event_id, actor.trim(), Utc::now())
            .await
    }

    fn evaluate(
        &self,
        state: &SupervisorStateRow,
        metrics: &SupervisorMetricsRow,
        now: DateTime<Utc>,
    ) -> Option<StalledReason> {
        if state.last_completed_at.is_some_and(|completed| completed > now) {
            return Some(StalledReason::ClockRollback);
        }
        let unfinished_tick = match (state.last_started_at, state.last_completed_at) {
            (Some(started), Some(completed)) => started > completed,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if unfinished_tick {
            let started = state.last_started_at.expect("unfinished tick has a start");
            if now.signed_duration_since(started) > chrono::Duration::from_std(self.tick_deadline).unwrap_or_default() {
                return Some(StalledReason::TickDeadlineExceeded);
            }
            // A tick is in flight and within its deadline: not stalled.
            return None;
        }
        let due_at = self.due_at(metrics);
        if due_at.is_some_and(|due| due + chrono::Duration::from_std(self.due_grace).unwrap_or_default() <= now) {
            return Some(StalledReason::DueNotStarted);
        }
        None
    }

    fn due_at(&self, metrics: &SupervisorMetricsRow) -> Option<DateTime<Utc>> {
        metrics
            .schedules
            .iter()
            .map(|schedule| schedule.next_slot.unwrap_or(schedule.enabled_at))
            .min()
    }

    fn next_wake_at(
        &self,
        metrics: &SupervisorMetricsRow,
        definitions: &[tinyiothub_core::harness::WorkflowRevision],
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        metrics
            .schedules
            .iter()
            .filter_map(|schedule| {
                if let Some(next_slot) = schedule.next_slot {
                    return Some(next_slot);
                }
                let revision = definitions
                    .iter()
                    .find(|revision| revision.workflow_id == schedule.workflow_id)?;
                if !matches!(revision.definition.trigger, WorkflowTrigger::Cron { .. }) {
                    return None;
                }
                next_wake_at(&revision.definition, now)
            })
            .min()
    }

    fn scheduling_delay_seconds(&self, metrics: &SupervisorMetricsRow, now: DateTime<Utc>) -> i64 {
        let pending_delay = metrics
            .oldest_pending_at
            .map(|oldest| now.signed_duration_since(oldest).num_seconds().max(0))
            .unwrap_or(0);
        let due_delay = self
            .due_at(metrics)
            .filter(|due| *due < now)
            .map(|due| now.signed_duration_since(due).num_seconds().max(0))
            .unwrap_or(0);
        pending_delay.max(due_delay)
    }

    fn snapshot_from(
        &self,
        state: &SupervisorStateRow,
        metrics: &SupervisorMetricsRow,
        definitions: &[tinyiothub_core::harness::WorkflowRevision],
        reason: Option<StalledReason>,
        now: DateTime<Utc>,
    ) -> SupervisorHealthSnapshot {
        let no_active_work = metrics.active_mains == 0 && metrics.active_subagents == 0 && metrics.pending_wakes == 0;
        let status = match reason {
            Some(_) => SupervisorStatus::Stalled,
            None if metrics.enabled_workflows == 0 && no_active_work => SupervisorStatus::Idle,
            None => SupervisorStatus::Healthy,
        };
        SupervisorHealthSnapshot {
            status,
            stalled_reason: reason,
            stalled_label: reason.map(StalledReason::label),
            tick_seq: state.tick_seq,
            last_started_at: state.last_started_at,
            last_completed_at: state.last_completed_at,
            next_wake_at: self.next_wake_at(metrics, definitions, now),
            oldest_pending_at: metrics.oldest_pending_at,
            scheduling_delay_seconds: self.scheduling_delay_seconds(metrics, now),
            active_mains: metrics.active_mains,
            active_subagents: metrics.active_subagents,
            enabled_workflows: metrics.enabled_workflows,
            pending_wakes: metrics.pending_wakes,
            last_error: state.last_error.clone(),
            no_new_runs: metrics.active_mains == 0 && metrics.active_subagents == 0,
            observed_at: now,
        }
    }

    fn transition_event(
        &self,
        state: &SupervisorStateRow,
        previous: Option<&str>,
        next: Option<&str>,
        reason: Option<StalledReason>,
        now: DateTime<Utc>,
    ) -> Result<HarnessEventInput, DbError> {
        let (event_type, kind) = if next.is_some() {
            ("supervisor.stalled", "stalled")
        } else {
            ("supervisor.recovered", "recovered")
        };
        let payload_json = serde_json::json!({
            "tickSeq": state.tick_seq,
            "state": if next.is_some() { "stalled" } else { "healthy" },
            "reason": reason.map(StalledReason::as_str),
            "previousReason": previous,
            "lastStartedAt": state.last_started_at,
            "lastCompletedAt": state.last_completed_at,
            "lastError": state.last_error,
        });
        let payload = serde_json::to_string(&payload_json)?;
        Ok(HarnessEventInput {
            event_id: format!("supervisor:{}:{}:{}", self.workspace_id, kind, state.tick_seq),
            workspace_id: self.workspace_id.clone(),
            session_id: format!("supervisor:{}", self.workspace_id),
            execution_mode: ExecutionMode::Production,
            run_id: None,
            task_id: None,
            event_type: event_type.to_string(),
            occurred_at: now,
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("{:x}", sha2::Sha256::digest(payload.as_bytes())),
            payload_json,
        })
    }
}

impl From<HarnessDeadWakeRow> for DeadWakeSummary {
    fn from(row: HarnessDeadWakeRow) -> Self {
        Self {
            event_id: row.event_id,
            task_id: row.task_id,
            failure_reason: row.failure_reason,
            created_at: row.created_at,
            retried_by: row.retried_by,
            retried_at: row.retried_at,
        }
    }
}
