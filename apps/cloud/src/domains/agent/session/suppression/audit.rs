//! Planned-wake attempts, atomic skip audit and immutable replay.
//!
//! One planned wake is identified by `(workspaceId, plannedWakeAt)`, so a
//! crash between the workspace skip commit and the Schedule update replays the
//! same wake rather than inventing a second Turn. Every attempt has a
//! monotonically allocated `attemptNo`; the turnId, wakeEventId and lifecycle
//! event IDs are derived from `(wakeId, attemptNo)` and are therefore stable
//! for exactly one attempt.
//!
//! The skipped lifecycle is written by exactly one call to
//! [`tinyiothub_storage::Db::commit_skip`], which appends every event and the
//! suppression state in one workspace transaction. This module never writes
//! skipped events through a second path.

use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
use tinyiothub_storage::Db;
use tinyiothub_storage::error::{DbError, Result};
use tinyiothub_storage::suppression::{
    CommittedSkip, OUTCOME_FULL_IN_PROGRESS, OUTCOME_SKIP_IN_PROGRESS, OUTCOME_SKIPPED, SkipAttempt, SuppressionState,
    SyncPending,
};
use uuid::Uuid;

use super::error::{Attempt, ErrorCode, Reconcile, RecoveryOutcome, SuppressionAlert, recover};
use super::preflight::{FENCE_SYNC_PENDING, PreflightDecision};
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::events::{SESSION_EVENT_SCHEMA_V, session_started_payload};

/// Stable reason written to `turn.completed(outcome=skipped)`.
pub const SKIP_REASON_NO_MATERIAL_CHANGE: &str = "no_material_change";
/// `turn.started.turnKind` for a scheduled planned wake.
pub const TURN_KIND_SCHEDULED: &str = "scheduled";
/// The exact skipped lifecycle, in commit order.
pub const SKIP_EVENT_TYPES: [&str; 6] = [
    "trigger.received",
    "wake.dispatched",
    "session.started",
    "turn.started",
    "turn.completed",
    "session.ended",
];

const WAKE_ID_DOMAIN: &[u8] = b"tinyiothub:planned-wake:v1\0";
const ATTEMPT_ID_DOMAIN: &[u8] = b"tinyiothub:wake-attempt:v1\0";

/// Stable identifier derived from the workspace and the planned wake time.
pub fn wake_id(workspace_id: &str, planned_wake_at: DateTime<Utc>) -> String {
    let planned = planned_wake_at.to_rfc3339_opts(SecondsFormat::Secs, true);
    format!(
        "wake:{}",
        hex_digest(&[WAKE_ID_DOMAIN, workspace_id.as_bytes(), b"\0", planned.as_bytes()])
    )
}

/// One planned scheduled wake. `planned_wake_at` is `None` for direct/event
/// invocations that have no stable planned identity; those keep a unique wake
/// id and never participate in planned-wake replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedWake {
    pub workspace_id: String,
    pub session_id: String,
    pub planned_wake_at: Option<DateTime<Utc>>,
    pub wake_id: String,
}

impl PlannedWake {
    pub fn planned(
        workspace_id: impl Into<String>,
        session_id: impl Into<String>,
        planned_wake_at: DateTime<Utc>,
    ) -> Self {
        let workspace_id = workspace_id.into();
        let session_id = session_id.into();
        let wake_id = wake_id(&workspace_id, planned_wake_at);
        Self {
            workspace_id,
            session_id,
            planned_wake_at: Some(planned_wake_at),
            wake_id,
        }
    }

    /// Non-planned invocation (direct call or event wake). Keeps the previous
    /// per-invocation identity instead of inventing a stable planned wake.
    pub fn unplanned(workspace_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            session_id: session_id.into(),
            planned_wake_at: None,
            wake_id: format!("wake:{}", Uuid::new_v4()),
        }
    }

    /// Attempt-derived identities for one monotonically allocated attempt.
    pub fn attempt(&self, attempt_no: i64) -> WakeAttempt {
        WakeAttempt {
            wake_id: self.wake_id.clone(),
            attempt_no,
            turn_id: derive_attempt_id("turn", &self.wake_id, attempt_no, None),
            wake_event_id: derive_attempt_id("wake-event", &self.wake_id, attempt_no, None),
        }
    }
}

/// Attempt-derived turn/wake/event identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeAttempt {
    pub wake_id: String,
    pub attempt_no: i64,
    pub turn_id: String,
    pub wake_event_id: String,
}

impl WakeAttempt {
    /// Stable event id for one lifecycle event of this attempt.
    pub fn event_id(&self, event_type: &str) -> String {
        derive_attempt_id("event", &self.wake_id, self.attempt_no, Some(event_type))
    }
}

fn derive_attempt_id(domain: &str, wake_id: &str, attempt_no: i64, event_type: Option<&str>) -> String {
    let attempt = attempt_no.to_string();
    let mut parts: Vec<&[u8]> = vec![
        ATTEMPT_ID_DOMAIN,
        domain.as_bytes(),
        b"\0",
        wake_id.as_bytes(),
        b"\0",
        attempt.as_bytes(),
    ];
    if let Some(event_type) = event_type {
        parts.push(b"\0");
        parts.push(event_type.as_bytes());
    }
    format!("{domain}:{}", hex_digest(&parts))
}

fn hex_digest(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn payload_digest(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Inputs to one atomic skip commit.
pub struct SkipAuditRequest<'a> {
    pub wake: &'a PlannedWake,
    pub attempt: &'a SkipAttempt,
    pub decision: &'a PreflightDecision,
    pub checked_at: DateTime<Utc>,
}

/// The exact skipped lifecycle for one attempt.
///
/// This is public so wake integration and tests can prove the event identity
/// is deterministic; the only writer remains [`Db::commit_skip`].
pub fn skipped_lifecycle_events(
    wake: &PlannedWake,
    attempt_no: i64,
    baseline_state_id: &str,
    checked_at: DateTime<Utc>,
) -> Vec<HarnessEventInput> {
    let attempt = wake.attempt(attempt_no);
    let event = |event_type: &str, payload: Value| HarnessEventInput {
        event_id: attempt.event_id(event_type),
        workspace_id: wake.workspace_id.clone(),
        session_id: wake.session_id.clone(),
        execution_mode: ExecutionMode::Production,
        // The skipped attempt owns a reserved run id so replay can load exactly
        // this lifecycle by `turnId`.
        run_id: Some(attempt.turn_id.clone()),
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: checked_at,
        cause_id: None,
        schema_v: SESSION_EVENT_SCHEMA_V,
        payload_digest: payload_digest(&payload),
        payload_json: payload,
    };
    let checked_at = checked_at.to_rfc3339_opts(SecondsFormat::Secs, true);
    vec![
        event(
            "trigger.received",
            json!({ "trigger": "schedule", "sourceId": &attempt.wake_event_id }),
        ),
        event("wake.dispatched", json!({ "wakeEventId": &attempt.wake_event_id })),
        event(
            "session.started",
            session_started_payload(&wake.session_id, TURN_KIND_SCHEDULED),
        ),
        event(
            "turn.started",
            json!({ "turnId": &attempt.turn_id, "turnKind": TURN_KIND_SCHEDULED }),
        ),
        event(
            "turn.completed",
            json!({
                "turnId": &attempt.turn_id,
                "outcome": "skipped",
                "reason": SKIP_REASON_NO_MATERIAL_CHANGE,
                "baselineStateId": baseline_state_id,
                "checkedAt": checked_at,
            }),
        ),
        event(
            "session.ended",
            json!({ "sessionId": &wake.session_id, "outcome": "skipped" }),
        ),
    ]
}

/// Build one lifecycle event with this attempt's derived identity.
///
/// Used by the wake integration for the events emitted outside
/// `SessionRunner`. Planned-wake lifecycle emissions share the attempt's
/// `turn_id` as their run id, so replay sees one identity convention for the
/// whole attempt. The event id is stable for the attempt.
pub fn attempt_event(
    context: &TurnContext,
    attempt: &WakeAttempt,
    event_type: &str,
    payload: Value,
) -> HarnessEventInput {
    HarnessEventInput {
        event_id: attempt.event_id(event_type),
        workspace_id: context.workspace_id.clone(),
        session_id: context.session_key.clone(),
        execution_mode: ExecutionMode::Production,
        run_id: Some(attempt.turn_id.clone()),
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: Utc::now(),
        cause_id: None,
        schema_v: SESSION_EVENT_SCHEMA_V,
        payload_digest: payload_digest(&payload),
        payload_json: payload,
    }
}

/// Which Turn the replay decision produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayOutcome {
    Skipped,
    Completed,
}

impl ReplayOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Completed => "completed",
        }
    }
}

/// Immutable replay answer for one planned wake.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayResult {
    pub outcome: ReplayOutcome,
    pub wake_id: String,
    pub turn_id: String,
    pub attempt_no: i64,
    /// The committed skip this full attempt supersedes, when the state changed.
    pub previous_attempt_id: Option<String>,
    /// True when a changed replay links this attempt to the same wake's
    /// previous attempt.
    pub supersedes: bool,
    /// The known committed skip for unchanged replay.
    pub committed_skip: Option<CommittedSkip>,
}

impl ReplayResult {
    pub fn outcome(&self) -> &'static str {
        self.outcome.as_str()
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn previous_attempt_id(&self) -> Option<&str> {
        self.previous_attempt_id.as_deref()
    }

    pub fn committed_skip(&self) -> Option<&CommittedSkip> {
        self.committed_skip.as_ref()
    }

    fn committed(wake: &PlannedWake, skip: CommittedSkip, turn_id: String) -> Self {
        Self {
            outcome: ReplayOutcome::Skipped,
            wake_id: wake.wake_id.clone(),
            turn_id,
            attempt_no: skip.attempt_no,
            previous_attempt_id: None,
            supersedes: false,
            committed_skip: Some(skip),
        }
    }

    fn skip_eligible(wake: &PlannedWake, attempt: WakeAttempt) -> Self {
        Self {
            outcome: ReplayOutcome::Skipped,
            wake_id: wake.wake_id.clone(),
            turn_id: attempt.turn_id,
            attempt_no: attempt.attempt_no,
            previous_attempt_id: None,
            supersedes: false,
            committed_skip: None,
        }
    }

    fn full(wake: &PlannedWake, attempt: WakeAttempt, previous_attempt_id: Option<String>) -> Self {
        Self {
            outcome: ReplayOutcome::Completed,
            wake_id: wake.wake_id.clone(),
            turn_id: attempt.turn_id,
            attempt_no: attempt.attempt_no,
            supersedes: previous_attempt_id.is_some(),
            previous_attempt_id,
            committed_skip: None,
        }
    }
}

/// Atomic skip writer and replay decision point.
pub struct AtomicSkipAudit {
    db: Arc<Db>,
}

impl AtomicSkipAudit {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Pre-check a decision before any state is touched.
    ///
    /// Callers must invoke this (or rely on [`Db::begin_skip_attempt`]'s mode
    /// gate) before allocating an attempt: shadow computes `would_skip` but
    /// must never write a durable audit fence.
    pub fn ensure_skip_allowed(decision: &PreflightDecision) -> Result<()> {
        validate_skip_decision(decision)
    }

    /// Commit the complete skipped lifecycle and all pending sync barriers in
    /// exactly one workspace transaction.
    pub async fn commit(&self, request: SkipAuditRequest<'_>) -> Result<CommittedSkip> {
        validate_skip_decision(request.decision)?;
        if request.attempt.workspace_id != request.wake.workspace_id
            || request.attempt.session_id != request.wake.session_id
        {
            return Err(DbError::Validation {
                message: "skip attempt does not belong to this planned wake".to_string(),
            });
        }
        if request.attempt.attempt_no <= 0 {
            return Err(DbError::Validation {
                message: "skip attempt requires a positive attemptNo".to_string(),
            });
        }
        let baseline_state_id = request
            .decision
            .baseline_state_id
            .as_deref()
            .ok_or_else(|| DbError::Validation {
                message: "a skip requires a baseline state id".to_string(),
            })?;
        let events = skipped_lifecycle_events(
            request.wake,
            request.attempt.attempt_no,
            baseline_state_id,
            request.checked_at,
        );
        // Exactly one writer: events and suppression state share this call's
        // single workspace transaction.
        self.db.commit_skip(request.attempt, &events).await
    }

    /// Decide what a replay of the same planned wake means.
    ///
    /// - A committed skip for this wake plus unchanged Sources returns the
    ///   committed skip without writing anything. Pending sync barriers are
    ///   left for reconciliation instead of creating a new Turn.
    /// - Unchanged Sources without a committed skip are eligible for the
    ///   normal atomic skip path.
    /// - Changed Sources (or any other veto) require a new full attempt linked
    ///   to the previous attempt via `previousAttemptId`/`supersedes`.
    pub async fn replay(&self, wake: &PlannedWake, decision: &PreflightDecision) -> Result<ReplayResult> {
        let state = self
            .db
            .load_suppression_state(&wake.workspace_id, &wake.session_id)
            .await?;
        if let Some(skip) = self.known_skip(wake, state.as_ref()).await? {
            if unchanged_against_committed_skip(decision) {
                let turn_id = committed_turn_id(&skip).unwrap_or_else(|| wake.attempt(skip.attempt_no).turn_id);
                return Ok(ReplayResult::committed(wake, skip, turn_id));
            }
            let next = next_attempt_no(state.as_ref());
            return Ok(ReplayResult::full(wake, wake.attempt(next), Some(skip.attempt_id)));
        }

        let next = next_attempt_no(state.as_ref());
        let attempt = wake.attempt(next);
        // Shadow records wouldSkip but never skips; only canary/enforce expose
        // the skip-eligible path.
        if decision.would_skip && decision.mode.allows_skip() {
            return Ok(ReplayResult::skip_eligible(wake, attempt));
        }
        Ok(ReplayResult::full(wake, attempt, None))
    }

    /// Load the committed skip owned by this planned wake, if any.
    async fn known_skip(&self, wake: &PlannedWake, state: Option<&SuppressionState>) -> Result<Option<CommittedSkip>> {
        let Some(state) = state else {
            return Ok(None);
        };
        if state.latest_outcome != OUTCOME_SKIPPED || state.attempt_no <= 0 {
            return Ok(None);
        }
        let Some(attempt_id) = state.latest_attempt_id.as_deref() else {
            return Ok(None);
        };
        let attempt = wake.attempt(state.attempt_no);
        let event_ids: Vec<String> = SKIP_EVENT_TYPES
            .iter()
            .map(|event_type| attempt.event_id(event_type))
            .collect();
        self.db
            .load_committed_skip(&wake.workspace_id, &wake.session_id, attempt_id, &event_ids)
            .await
    }
}

fn validate_skip_decision(decision: &PreflightDecision) -> Result<()> {
    if !decision.mode.allows_skip() {
        return Err(DbError::Validation {
            message: format!("suppression mode {} does not allow a skip", decision.mode.as_str()),
        });
    }
    if !decision.would_skip || !decision.vetoes.is_empty() || !decision.fence.is_empty() {
        return Err(DbError::Validation {
            message: "preflight decision does not allow a skip".to_string(),
        });
    }
    Ok(())
}

/// A committed skip stays unchanged across Schedule/projection/notification
/// reconciliation. Only real Source/input/registry changes or other fences
/// force a new full attempt.
fn unchanged_against_committed_skip(decision: &PreflightDecision) -> bool {
    decision.vetoes.is_empty() && (decision.fence.is_empty() || decision.fence == FENCE_SYNC_PENDING)
}

fn next_attempt_no(state: Option<&SuppressionState>) -> i64 {
    state.map(|state| state.attempt_no + 1).unwrap_or(1)
}

fn committed_turn_id(skip: &CommittedSkip) -> Option<String> {
    skip.events
        .iter()
        .find(|event| event.event_type == "turn.completed")
        .and_then(|event| event.payload_json.get("turnId"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Post-commit notification consumer.
///
/// Production notifies the SSE hub. Tests inject failures to prove a committed
/// skip is never rolled back: the barrier stays set and clients resume from
/// the high watermark.
#[async_trait::async_trait]
pub trait SuppressionNotifier: Send + Sync {
    async fn notify(&self, workspace_id: &str, session_id: &str) -> std::result::Result<(), String>;
}

/// Notification consumer that always succeeds (read-only tests/recovery units).
pub struct NoopSuppressionNotifier;

#[async_trait::async_trait]
impl SuppressionNotifier for NoopSuppressionNotifier {
    async fn notify(&self, _workspace_id: &str, _session_id: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

/// What startup/restart recovery settled. Safe identifiers only.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryReport {
    pub workspace_id: String,
    pub session_key: String,
    /// A committed skip was found and its sync barriers were reconciled.
    pub replayed_committed_skip: bool,
    /// A stale full attempt was settled to `full_failed`.
    pub settled_full_attempt: Option<String>,
    /// A stale audit fence was settled to `audit_failed`.
    pub settled_audit_attempt: Option<String>,
    /// Barriers whose consumer succeeded and were cleared.
    pub cleared: Vec<String>,
    /// Alerts to emit through Session Events + tracing + report.
    pub alerts: Vec<SuppressionAlert>,
}

impl RecoveryReport {
    fn new(workspace_id: &str, session_key: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            session_key: session_key.to_string(),
            replayed_committed_skip: false,
            settled_full_attempt: None,
            settled_audit_attempt: None,
            cleared: Vec::new(),
            alerts: Vec::new(),
        }
    }
}

/// Startup/restart recovery for durable suppression fences (ruling R16).
///
/// Every settlement is idempotent and bounded by the global three-total-attempt
/// recovery: an unknown commit result is reconciled by attempt/event identity
/// before any retry. A sync barrier is cleared only after its consumer
/// succeeded, so a crash between commit and reconciliation never hides work.
pub struct SuppressionRecovery {
    db: Arc<Db>,
    notifier: Arc<dyn SuppressionNotifier>,
}

impl SuppressionRecovery {
    pub fn new(db: Arc<Db>, notifier: Arc<dyn SuppressionNotifier>) -> Self {
        Self { db, notifier }
    }

    pub fn with_noop_notifier(db: Arc<Db>) -> Self {
        Self::new(db, Arc::new(NoopSuppressionNotifier))
    }

    /// Settle the fences of one Assistant session.
    pub async fn recover_session(&self, workspace_id: &str, session_key: &str) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::new(workspace_id, session_key);
        let Some(mut state) = self.db.load_suppression_state(workspace_id, session_key).await? else {
            return Ok(report);
        };

        // A full attempt left in progress by a previous process has no live
        // worker. Inspect its terminal evidence, then settle it so the next
        // wake is allowed to run a full Turn.
        if state.full_attempt_in_progress || state.latest_outcome == OUTCOME_FULL_IN_PROGRESS {
            if let Some(attempt_id) = state.latest_attempt_id.clone() {
                let reason = self
                    .inspect_stale_full_attempt(workspace_id, session_key, &state)
                    .await?;
                if self
                    .settle_full_attempt(workspace_id, session_key, &attempt_id, &reason)
                    .await
                {
                    report.settled_full_attempt = Some(attempt_id);
                } else {
                    report.alerts.push(self.alert(
                        &state,
                        ErrorCode::AuditWriteFailed,
                        json!({ "fence": "full_attempt_in_progress" }),
                    ));
                }
            }
            state = self
                .db
                .load_suppression_state(workspace_id, session_key)
                .await?
                .unwrap_or(state);
        }

        // A stale audit fence never owns a committed skip: the skip state and
        // its six events commit together. Settle it to `audit_failed`, which
        // keeps the old baseline ineligible and vetoes the next skip.
        if state.audit_in_progress || state.latest_outcome == OUTCOME_SKIP_IN_PROGRESS {
            if let Some(attempt_id) = state.latest_attempt_id.clone() {
                if self
                    .settle_skip_attempt(workspace_id, session_key, &attempt_id, "audit_write_failed")
                    .await
                {
                    report.settled_audit_attempt = Some(attempt_id);
                } else {
                    report.alerts.push(self.alert(
                        &state,
                        ErrorCode::AuditWriteFailed,
                        json!({ "fence": "audit_in_progress" }),
                    ));
                }
            }
            state = self
                .db
                .load_suppression_state(workspace_id, session_key)
                .await?
                .unwrap_or(state);
        }

        // A committed skip is replayed from its immutable events; only the
        // non-Schedule side effects are reconciled here. Schedule stays the
        // TriggerRouter's exclusive write.
        if state.latest_outcome == OUTCOME_SKIPPED {
            let committed = self.committed_skip_present(workspace_id, session_key).await?;
            if committed {
                report.replayed_committed_skip = true;
                if state.projection_sync_pending {
                    match self.reconcile_projection(workspace_id, session_key, &state).await {
                        None => report.cleared.push("projection".to_string()),
                        Some(alert) => report.alerts.push(alert),
                    }
                }
                if state.notification_pending {
                    match self.reconcile_notification(workspace_id, session_key, &state).await {
                        None => report.cleared.push("notification".to_string()),
                        Some(alert) => report.alerts.push(alert),
                    }
                }
                if state.schedule_sync_pending {
                    match self.reconcile_schedule(workspace_id, session_key, &state).await {
                        None => report.cleared.push("schedule".to_string()),
                        Some(alert) => report.alerts.push(alert),
                    }
                }
            } else if state.schedule_sync_pending || state.projection_sync_pending || state.notification_pending {
                report.alerts.push(self.alert(
                    &state,
                    ErrorCode::AuditWriteFailed,
                    json!({ "cause": "committed_skip_events_missing" }),
                ));
            }
        }
        Ok(report)
    }

    /// Reconcile the non-Schedule side effects of an already committed skip.
    ///
    /// Used by the live wake immediately after `commit_skip` and by recovery.
    /// Schedule stays the TriggerRouter's exclusive responsibility, so this
    /// never writes it and never raises `schedule_barrier_failed`.
    pub async fn reconcile_committed_consumers(&self, workspace_id: &str, session_key: &str) -> Vec<SuppressionAlert> {
        let mut alerts = Vec::new();
        let state = match self.db.load_suppression_state(workspace_id, session_key).await {
            Ok(Some(state)) if state.latest_outcome == OUTCOME_SKIPPED => state,
            Ok(_) => return alerts,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression recovery: state read failed");
                return alerts;
            }
        };
        if state.projection_sync_pending
            && let Some(alert) = self.reconcile_projection(workspace_id, session_key, &state).await
        {
            alerts.push(alert);
        }
        if state.notification_pending
            && let Some(alert) = self.reconcile_notification(workspace_id, session_key, &state).await
        {
            alerts.push(alert);
        }
        alerts
    }

    /// Settle a failed atomic skip audit within the bounded recovery.
    pub async fn settle_skip_failure(
        &self,
        workspace_id: &str,
        session_key: &str,
        attempt_id: &str,
        reason: &str,
    ) -> bool {
        self.settle_skip_attempt(workspace_id, session_key, attempt_id, reason)
            .await
    }

    /// Settle a failed full attempt within the bounded recovery.
    pub async fn settle_full_failure(
        &self,
        workspace_id: &str,
        session_key: &str,
        attempt_id: &str,
        reason: &str,
    ) -> bool {
        self.settle_full_attempt(workspace_id, session_key, attempt_id, reason)
            .await
    }

    /// Only a terminal event newer than the fence tells us what the stale
    /// attempt was doing; otherwise it simply died in progress.
    async fn inspect_stale_full_attempt(
        &self,
        workspace_id: &str,
        session_key: &str,
        state: &SuppressionState,
    ) -> Result<String> {
        let latest = self
            .db
            .recent_suppression_outcomes(workspace_id, session_key, 1)
            .await?;
        Ok(match latest.first() {
            Some(outcome) if outcome.occurred_at >= state.updated_at && outcome.outcome == "completed" => {
                "stale_completed_attempt_recovered"
            }
            Some(outcome) if outcome.occurred_at >= state.updated_at && outcome.outcome == "cancelled" => {
                "stale_cancelled_attempt_recovered"
            }
            _ => "stale_in_progress_attempt_recovered",
        }
        .to_string())
    }

    /// The committed skip is proven by its latest immutable `turn.completed`.
    async fn committed_skip_present(&self, workspace_id: &str, session_key: &str) -> Result<bool> {
        let latest = self
            .db
            .recent_suppression_outcomes(workspace_id, session_key, 1)
            .await?;
        Ok(latest.first().is_some_and(|outcome| outcome.outcome == OUTCOME_SKIPPED))
    }

    async fn settle_full_attempt(&self, workspace_id: &str, session_key: &str, attempt_id: &str, reason: &str) -> bool {
        let db = self.db.clone();
        let workspace = workspace_id.to_string();
        let session = session_key.to_string();
        let attempt = attempt_id.to_string();
        let reason = reason.to_string();
        let reconcile_session = session.clone();
        let reconcile_db = db.clone();
        let reconcile_attempt = attempt.clone();
        matches!(
            recover(
                move |_attempt| {
                    let db = db.clone();
                    let workspace = workspace.clone();
                    let attempt = attempt.clone();
                    let reason = reason.clone();
                    async move {
                        match db.fail_full_attempt(&workspace, &attempt, &reason).await {
                            Ok(_) => Attempt::Succeeded(()),
                            Err(error) => db_attempt(error),
                        }
                    }
                },
                move |_attempt| {
                    let db = reconcile_db.clone();
                    let workspace = workspace_id.to_string();
                    let session = reconcile_session.clone();
                    let attempt = reconcile_attempt.clone();
                    async move {
                        match db.load_suppression_state(&workspace, &session).await {
                            Ok(Some(current))
                                if !current.full_attempt_in_progress
                                    || current.latest_attempt_id.as_deref() != Some(attempt.as_str()) =>
                            {
                                // Settled elsewhere or superseded by a newer attempt.
                                Reconcile::Settled(())
                            }
                            _ => Reconcile::RetryAllowed,
                        }
                    }
                },
            )
            .await,
            RecoveryOutcome::Succeeded { .. }
        )
    }

    async fn settle_skip_attempt(&self, workspace_id: &str, session_key: &str, attempt_id: &str, reason: &str) -> bool {
        let db = self.db.clone();
        let workspace = workspace_id.to_string();
        let session = session_key.to_string();
        let attempt = attempt_id.to_string();
        let reason = reason.to_string();
        let reconcile_session = session.clone();
        let reconcile_db = db.clone();
        let reconcile_attempt = attempt.clone();
        matches!(
            recover(
                move |_attempt| {
                    let db = db.clone();
                    let workspace = workspace.clone();
                    let session = session.clone();
                    let attempt = attempt.clone();
                    let reason = reason.clone();
                    async move {
                        match db.fail_skip_attempt(&workspace, &session, &attempt, &reason).await {
                            Ok(_) => Attempt::Succeeded(()),
                            Err(error) => db_attempt(error),
                        }
                    }
                },
                move |_attempt| {
                    let db = reconcile_db.clone();
                    let workspace = workspace_id.to_string();
                    let session = reconcile_session.clone();
                    let attempt = reconcile_attempt.clone();
                    async move {
                        match db.load_suppression_state(&workspace, &session).await {
                            Ok(Some(current))
                                if !current.audit_in_progress
                                    || current.latest_attempt_id.as_deref() != Some(attempt.as_str()) =>
                            {
                                Reconcile::Settled(())
                            }
                            _ => Reconcile::RetryAllowed,
                        }
                    }
                },
            )
            .await,
            RecoveryOutcome::Succeeded { .. }
        )
    }

    /// Projection is an independent, non-atomic projection of committed
    /// events. Update or confirm it, then clear the barrier.
    async fn reconcile_projection(
        &self,
        workspace_id: &str,
        session_key: &str,
        state: &SuppressionState,
    ) -> Option<SuppressionAlert> {
        let consumer_db = self.db.clone();
        let consumer_session = session_key.to_string();
        let outcome = recover(
            move |_attempt| {
                let db = consumer_db.clone();
                let session = consumer_session.clone();
                async move {
                    match db.get_session(&session).await {
                        Ok(Some(row)) if row.status == "ended" => Attempt::Succeeded(()),
                        Ok(Some(mut row)) => {
                            row.status = "ended".to_string();
                            row.touch();
                            match db.update_session(&row).await {
                                Ok(()) => Attempt::Succeeded(()),
                                // Projection is an independent recovery surface:
                                // a write failure is bounded-retryable.
                                Err(_) => Attempt::Transient("projection_write_failed".to_string()),
                            }
                        }
                        // No row to update: committed events remain the truth.
                        Ok(None) => Attempt::Succeeded(()),
                        Err(_) => Attempt::Transient("projection_read_failed".to_string()),
                    }
                }
            },
            |_attempt| async { Reconcile::RetryAllowed },
        )
        .await;
        if !matches!(outcome, RecoveryOutcome::Succeeded { .. }) {
            return Some(self.alert(
                state,
                ErrorCode::ProjectionReconcileFailed,
                json!({ "consumer": "projection" }),
            ));
        }
        if self
            .clear_barrier(workspace_id, session_key, SyncPending::Projection)
            .await
        {
            None
        } else {
            Some(self.alert(
                state,
                ErrorCode::ProjectionReconcileFailed,
                json!({ "barrier": "projection" }),
            ))
        }
    }

    /// Post-commit SSE notification never rolls back committed events. It is
    /// retried three total times; persistent failure keeps the barrier and
    /// alerts so clients resume from the high watermark.
    async fn reconcile_notification(
        &self,
        workspace_id: &str,
        session_key: &str,
        state: &SuppressionState,
    ) -> Option<SuppressionAlert> {
        let notifier = self.notifier.clone();
        let workspace = workspace_id.to_string();
        let session = session_key.to_string();
        let outcome = recover(
            move |_attempt| {
                let notifier = notifier.clone();
                let workspace = workspace.clone();
                let session = session.clone();
                async move {
                    match notifier.notify(&workspace, &session).await {
                        Ok(()) => Attempt::Succeeded(()),
                        Err(error) => Attempt::Transient(error),
                    }
                }
            },
            |_attempt| async { Reconcile::RetryAllowed },
        )
        .await;
        if !matches!(outcome, RecoveryOutcome::Succeeded { .. }) {
            return Some(self.alert(
                state,
                ErrorCode::NotificationFailed,
                json!({ "notificationFailed": true }),
            ));
        }
        if self
            .clear_barrier(workspace_id, session_key, SyncPending::Notification)
            .await
        {
            None
        } else {
            Some(self.alert(
                state,
                ErrorCode::NotificationFailed,
                json!({ "barrier": "notification" }),
            ))
        }
    }

    /// Schedule is written exclusively by TriggerRouter. Recovery clears the
    /// barrier only after the schedule already records the skip; otherwise it
    /// stays a visible `committed_partial` and alerts.
    async fn reconcile_schedule(
        &self,
        workspace_id: &str,
        session_key: &str,
        state: &SuppressionState,
    ) -> Option<SuppressionAlert> {
        // The Schedule write must be the one for this committed skip: its
        // `lastWakeAt` cannot predate the skip's terminal event.
        let skip_occurred_at = self
            .db
            .recent_suppression_outcomes(workspace_id, session_key, 1)
            .await
            .ok()
            .and_then(|outcomes| {
                outcomes
                    .first()
                    .filter(|outcome| outcome.outcome == OUTCOME_SKIPPED)
                    .map(|outcome| outcome.occurred_at)
            });
        let confirmed = match self.db.get_assistant_schedule(workspace_id).await {
            Ok(Some(schedule)) => {
                schedule.last_outcome.as_deref() == Some(OUTCOME_SKIPPED)
                    && matches!(
                        (schedule.last_wake_at, skip_occurred_at),
                        (Some(written_at), Some(skip_at)) if written_at >= skip_at
                    )
            }
            Ok(None) => false,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression recovery: schedule read failed");
                false
            }
        };
        if confirmed
            && self
                .clear_barrier(workspace_id, session_key, SyncPending::Schedule)
                .await
        {
            return None;
        }
        Some(self.alert(
            state,
            ErrorCode::ScheduleBarrierFailed,
            json!({ "committedPartial": true, "scheduleConfirmed": confirmed }),
        ))
    }

    async fn clear_barrier(&self, workspace_id: &str, session_key: &str, pending: SyncPending) -> bool {
        let db = self.db.clone();
        let workspace = workspace_id.to_string();
        let session = session_key.to_string();
        matches!(
            recover(
                move |_attempt| {
                    let db = db.clone();
                    let workspace = workspace.clone();
                    let session = session.clone();
                    async move {
                        match db.clear_sync_pending(&workspace, &session, pending).await {
                            Ok(_) => Attempt::Succeeded(()),
                            Err(error) => db_attempt(error),
                        }
                    }
                },
                |_attempt| async { Reconcile::RetryAllowed },
            )
            .await,
            RecoveryOutcome::Succeeded { .. }
        )
    }

    fn alert(&self, state: &SuppressionState, code: ErrorCode, metrics: Value) -> SuppressionAlert {
        SuppressionAlert {
            code: code.as_str().to_string(),
            workspace_id: state.workspace_id.clone(),
            turn_id: state
                .baseline_turn_state
                .as_ref()
                .and_then(|value| value.get("turnId"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            baseline_state_id: state.baseline_state_id.clone(),
            current_registry_version: state.registry_version.clone(),
            baseline_registry_version: state.registry_version.clone(),
            vetoes: Vec::new(),
            consecutive_failures: 1,
            metrics,
            trace_id: state
                .latest_attempt_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        }
    }
}

/// Classify a storage failure for the bounded recovery loop.
fn db_attempt<T>(error: DbError) -> Attempt<T, DbError> {
    let class = match &error {
        // Unknown or superseded outcomes reconcile by attempt/state identity
        // before any retry.
        DbError::CommitUnknown { .. }
        | DbError::StorageLocked { .. }
        | DbError::CasConflict { .. }
        | DbError::NotFound { .. } => 2,
        DbError::Sqlx(_) => 1,
        _ => 0,
    };
    match class {
        2 => Attempt::Unknown(error),
        1 => Attempt::Transient(error),
        _ => Attempt::Permanent(error),
    }
}

#[cfg(test)]
#[allow(clippy::module_inception)] // the test path must contain `suppression::audit`
mod audit {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use chrono::{DateTime, Duration, TimeZone, Utc};
    use serde_json::{Value, json};
    use tinyiothub_core::harness::{ExecutionMode, HarnessEvent, HarnessEventInput};
    use tinyiothub_storage::Db;
    use tinyiothub_storage::assistant_schedule::SuppressionConfig;
    use tinyiothub_storage::harness_event::EventFilter;
    use tinyiothub_storage::suppression::{
        FullAttemptCompletion, OUTCOME_SKIPPED, SuppressionMode, TURN_STATE_VERSION,
    };

    use super::{
        AtomicSkipAudit, PlannedWake, ReplayOutcome, SKIP_EVENT_TYPES, SkipAuditRequest, WakeAttempt, wake_id,
    };
    use crate::domains::agent::session::suppression::contract::{
        CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;
    use crate::domains::agent::session::suppression::preflight::SuppressionService;
    use crate::domains::agent::session::suppression::registry::{
        AssistantSource, SOURCE_VERSION, SourceContext, SourceMode, SourceRegistry,
    };

    const WS: &str = "ws-audit";
    const ASSISTANT: &str = "__assistant__ws-audit";
    const SESSION: &str = "agent:ws-audit:__assistant__ws-audit/main";
    const SOURCE_ID: &str = "test.changing";

    fn ts() -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).single().expect("timestamp")
    }

    fn later(seconds: i64) -> DateTime<Utc> {
        ts() + Duration::seconds(seconds)
    }

    fn hash_hex(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    struct ChangingSource {
        revision: Mutex<(String, String)>,
    }

    impl ChangingSource {
        fn new() -> Self {
            Self {
                revision: Mutex::new(("rev-1".to_string(), hash_hex('a'))),
            }
        }

        fn change(&self) {
            let mut revision = self.revision.lock().expect("source revision");
            revision.0 = "rev-2".to_string();
            revision.1 = hash_hex('b');
        }

        fn snapshot(&self) -> SourceSnapshot {
            let revision = self.revision.lock().expect("source revision").clone();
            SourceSnapshot {
                source_id: SOURCE_ID.to_string(),
                source_version: SOURCE_VERSION,
                state_ref: Some(StateRef {
                    id: format!("state:{}", revision.0),
                    revision: revision.0.clone(),
                    state_hash: revision.1.clone(),
                }),
                revision: revision.0,
                state_hash: revision.1,
                available: true,
                empty: true,
                work_items: vec![],
                error: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl AssistantSource for ChangingSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                source_id: SOURCE_ID.to_string(),
                source_version: SOURCE_VERSION,
            }
        }

        async fn capture(&self, _cx: &SourceContext) -> Result<SourceSnapshot, SuppressionError> {
            Ok(self.snapshot())
        }

        async fn resolve_state(&self, _reference: &StateRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("source_ref_mismatch", "unused", "unused"))
        }

        async fn resolve_payload(&self, _reference: &PayloadRef) -> Result<CanonicalBytes, SuppressionError> {
            Err(SuppressionError::new("work_item_payload_missing", "unused", "unused"))
        }
    }

    struct AuditFixture {
        _root: tempfile::TempDir,
        db: Arc<Db>,
        workspace: sqlx::SqlitePool,
        registry: Arc<SourceRegistry>,
        source: Arc<ChangingSource>,
        wake: PlannedWake,
        baseline_observed_at: DateTime<Utc>,
    }

    impl AuditFixture {
        async fn create() -> Self {
            Self::create_with_mode(SuppressionMode::Enforce).await
        }

        async fn create_with_mode(mode: SuppressionMode) -> Self {
            let root = tempfile::tempdir().expect("fixture root");
            let platform = tinyiothub_storage::test_helpers::test_pool().await;
            let db = Arc::new(Db::new(platform).with_workspace_store_root(root.path().join("data/agents")));
            tinyiothub_storage::seed::seed_system(&db).await.expect("seed system");
            sqlx::query(
                "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
                 VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
            )
            .bind(WS)
            .execute(db.pool())
            .await
            .expect("insert workspace");
            db.create_workspace_store(WS).await.expect("workspace store");
            let workspace = open_workspace_db(&workspace_file(root.path(), WS)).await;
            db.ensure_assistant_schedule(WS, ASSISTANT, ts())
                .await
                .expect("assistant schedule");
            db.update_suppression_config(WS, Some(mode), None)
                .await
                .expect("configure suppression mode")
                .expect("schedule row");

            let source = Arc::new(ChangingSource::new());
            let mut sources = SourceRegistry::new();
            sources.register(source.clone()).expect("register source");
            let registry = Arc::new(sources);

            let baseline_observed_at = ts();
            let snapshots = registry
                .capture_all(&SourceContext::new(WS, SESSION))
                .await
                .expect("capture baseline");
            let config = db
                .get_suppression_config(WS)
                .await
                .expect("config")
                .expect("schedule row");
            let baseline = db
                .load_suppression_state(WS, SESSION)
                .await
                .expect("state")
                .unwrap_or_else(|| tinyiothub_storage::suppression::SuppressionState::initial(WS, SESSION));
            let attempt = db.begin_full_attempt(SESSION, &baseline).await.expect("begin baseline");
            let completion = FullAttemptCompletion {
                state_id: "state-1".to_string(),
                state_version: TURN_STATE_VERSION,
                registry_version: registry.registry_version(),
                config_version: config.config_version,
                control_cursor: 0,
                turn_state: turn_state("state-1", &snapshots, &registry.registry_version()),
                observed_at: baseline_observed_at,
            };
            db.complete_full_attempt(WS, &attempt.attempt_id, &completion)
                .await
                .expect("install baseline");
            // Reconciliation is measured from the durable installation time
            // (which is the real clock), so anchor the fixture's observation
            // window to it instead of a synthetic fixed timestamp.
            let baseline_observed_at = db
                .load_suppression_state(WS, SESSION)
                .await
                .expect("reload state")
                .and_then(|state| state.baseline_installed_at)
                .expect("installed baseline timestamp");

            Self {
                _root: root,
                db,
                workspace,
                registry,
                source,
                wake: PlannedWake::planned(WS, SESSION, baseline_observed_at),
                baseline_observed_at,
            }
        }

        async fn config(&self) -> SuppressionConfig {
            self.db
                .get_suppression_config(WS)
                .await
                .expect("config")
                .expect("schedule row")
        }

        fn checked_at(&self) -> DateTime<Utc> {
            self.baseline_observed_at + Duration::seconds(60)
        }

        async fn state(&self) -> tinyiothub_storage::suppression::SuppressionState {
            self.db
                .load_suppression_state(WS, SESSION)
                .await
                .expect("state")
                .expect("suppression row")
        }

        async fn decision(
            &self,
            seconds: i64,
        ) -> crate::domains::agent::session::suppression::preflight::PreflightDecision {
            let state = self.state().await;
            let config = self.config().await;
            SuppressionService::new(self.registry.clone())
                .evaluate(
                    SourceContext {
                        workspace_id: WS.to_string(),
                        session_id: SESSION.to_string(),
                        observed_at: self.baseline_observed_at + Duration::seconds(seconds),
                        mode: SourceMode::Enforce,
                    },
                    Some(&state),
                    &config,
                    0,
                )
                .await
                .expect("preflight")
        }

        async fn commit_skip(&self) -> tinyiothub_storage::suppression::CommittedSkip {
            let decision = self.decision(60).await;
            assert!(decision.would_skip, "fixture must allow a skip: {:?}", decision.vetoes);
            let state = self.state().await;
            let attempt = self
                .db
                .begin_skip_attempt(SESSION, &state)
                .await
                .expect("begin skip attempt");
            AtomicSkipAudit::new(self.db.clone())
                .commit(SkipAuditRequest {
                    wake: &self.wake,
                    attempt: &attempt,
                    decision: &decision,
                    checked_at: self.checked_at(),
                })
                .await
                .expect("atomic skip commit")
        }

        async fn replay(&self) -> super::ReplayResult {
            let decision = self.decision(60).await;
            let result = AtomicSkipAudit::new(self.db.clone())
                .replay(&self.wake, &decision)
                .await
                .expect("replay decision");
            if result.outcome == ReplayOutcome::Completed {
                // Task 6 owns the real full-Turn lifecycle. Simulate its
                // durable attempt/completion so replay linkage is observable.
                let state = self.state().await;
                let attempt = self
                    .db
                    .begin_full_attempt(SESSION, &state)
                    .await
                    .expect("begin full attempt");
                assert_eq!(
                    attempt.attempt_no, result.attempt_no,
                    "replay plan and durable attempt allocation must agree"
                );
                let snapshots = self
                    .registry
                    .capture_all(&SourceContext::new(WS, SESSION))
                    .await
                    .expect("capture full attempt");
                let config = self.config().await;
                let state_id = format!("state-full-{}", attempt.attempt_no);
                let completion = FullAttemptCompletion {
                    state_id: state_id.clone(),
                    state_version: TURN_STATE_VERSION,
                    registry_version: self.registry.registry_version(),
                    config_version: config.config_version,
                    control_cursor: 0,
                    turn_state: turn_state(&state_id, &snapshots, &self.registry.registry_version()),
                    observed_at: self.checked_at(),
                };
                self.db
                    .complete_full_attempt(WS, &attempt.attempt_id, &completion)
                    .await
                    .expect("complete simulated full attempt");
            }
            result
        }

        async fn events(&self) -> Vec<HarnessEvent> {
            let filter = EventFilter {
                session_id: Some(SESSION.to_string()),
                run_id: None,
                task_id: None,
                execution_mode: ExecutionMode::Production,
            };
            self.db
                .session_event_page(WS, &filter, 0, 256)
                .await
                .expect("session events")
                .events
        }

        async fn event_count(&self) -> usize {
            self.events().await.len()
        }

        async fn skipped_event_count(&self) -> usize {
            self.events()
                .await
                .iter()
                .filter(|event| event.event_type == "turn.completed" && event.payload_json["outcome"] == "skipped")
                .count()
        }

        async fn skip_turn_id(&self, skip: &tinyiothub_storage::suppression::CommittedSkip) -> String {
            super::committed_turn_id(skip).expect("committed turn id")
        }

        async fn append_user_message(&self, message_id: &str) {
            let payload = json!({ "messageId": message_id, "role": "user", "text": "new input" });
            let event = HarnessEventInput {
                event_id: format!("user:{message_id}"),
                workspace_id: WS.to_string(),
                session_id: SESSION.to_string(),
                execution_mode: ExecutionMode::Production,
                run_id: None,
                task_id: None,
                event_type: "message.created".to_string(),
                occurred_at: ts(),
                cause_id: None,
                schema_v: 1,
                payload_digest: super::payload_digest(&payload),
                payload_json: payload,
            };
            self.db.append_session_event(&event).await.expect("user message");
        }

        fn change_source(&self) {
            self.source.change();
        }
    }

    fn workspace_file(root: &std::path::Path, workspace_id: &str) -> PathBuf {
        root.join("data/agents").join(workspace_id).join("harness/harness.db")
    }

    async fn open_workspace_db(path: &std::path::Path) -> sqlx::SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("workspace database")
    }

    fn turn_state(state_id: &str, snapshots: &[SourceSnapshot], registry_version: &str) -> Value {
        json!({
            "stateId": state_id,
            "stateVersion": TURN_STATE_VERSION,
            "turnId": "turn-fixture",
            "sources": snapshots
                .iter()
                .map(|snapshot| {
                    json!({
                        "sourceId": snapshot.source_id,
                        "sourceVersion": snapshot.source_version,
                        "revision": snapshot.revision,
                        "stateHash": snapshot.state_hash,
                        "available": snapshot.available,
                    })
                })
                .collect::<Vec<_>>(),
            "controlCursor": 0,
            "registryVersion": registry_version,
            "mode": "enforce",
        })
    }

    #[test]
    fn wake_id_is_stable_for_workspace_and_planned_time() {
        let first = wake_id(WS, ts());
        assert_eq!(first, wake_id(WS, ts()));
        assert_ne!(first, wake_id("ws-other", ts()));
        assert_ne!(first, wake_id(WS, later(1)));
    }

    #[test]
    fn attempt_ids_include_the_monotonic_attempt_number() {
        let wake = PlannedWake::planned(WS, SESSION, ts());
        let first = wake.attempt(1);
        let second = wake.attempt(2);
        assert_ne!(first.turn_id, second.turn_id);
        assert_ne!(first.wake_event_id, second.wake_event_id);
        assert_ne!(
            first.event_id("turn.completed"),
            second.event_id("turn.completed"),
            "event ids must change with attemptNo"
        );
        assert_ne!(
            first.event_id("turn.completed"),
            first.event_id("session.ended"),
            "event ids must distinguish lifecycle events"
        );
        // A replay of the same attempt derives the same ids.
        assert_eq!(first, wake.attempt(1));
    }

    #[tokio::test]
    async fn skipped_lifecycle_is_committed_atomically_in_order() {
        let fixture = AuditFixture::create().await;
        let skip = fixture.commit_skip().await;

        assert_eq!(skip.events.len(), SKIP_EVENT_TYPES.len());
        let types: Vec<&str> = skip.events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(types, SKIP_EVENT_TYPES);
        assert_eq!(skip.state.latest_outcome, OUTCOME_SKIPPED);
        assert!(skip.state.schedule_sync_pending);
        assert!(skip.state.projection_sync_pending);
        assert!(skip.state.notification_pending);

        let terminal = &skip.events[4].payload_json;
        assert_eq!(terminal["outcome"], "skipped");
        assert_eq!(terminal["reason"], "no_material_change");
        assert_eq!(terminal["baselineStateId"], "state-1");
        assert!(terminal["checkedAt"].as_str().is_some());
        assert_eq!(skip.events[3].payload_json["turnKind"], "scheduled");
        assert_eq!(skip.events[5].payload_json["outcome"], "skipped");
        assert_eq!(
            skip.events[0].payload_json["sourceId"],
            skip.events[1].payload_json["wakeEventId"]
        );
        assert_eq!(fixture.event_count().await, 6);

        // One attempt, one reserved run id: every row belongs to the same
        // skipped lifecycle and no unrelated event was written.
        let turn_ids: std::collections::HashSet<&str> =
            skip.events.iter().filter_map(|event| event.run_id.as_deref()).collect();
        assert_eq!(turn_ids.len(), 1);
    }

    #[tokio::test]
    async fn unchanged_replay_returns_the_committed_skip_without_new_events() {
        let fixture = AuditFixture::create().await;
        let skip = fixture.commit_skip().await;
        let turn_id = fixture.skip_turn_id(&skip).await;
        let before = fixture.event_count().await;

        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "skipped");
        assert_eq!(replay.turn_id(), turn_id);
        assert_eq!(replay.previous_attempt_id(), None);
        assert_eq!(
            replay.committed_skip().map(|skip| skip.attempt_id.as_str()),
            Some(skip.attempt_id.as_str())
        );
        assert_eq!(fixture.event_count().await, before, "no new Turn or events");
        assert_eq!(fixture.skipped_event_count().await, 1);
    }

    #[tokio::test]
    async fn changed_replay_creates_linked_full_attempt_without_overwriting_skip() {
        let fixture = AuditFixture::create().await;
        let skip = fixture.commit_skip().await;
        fixture.change_source();
        let full = fixture.replay().await;
        assert_eq!(full.outcome(), "completed");
        assert_ne!(full.turn_id(), fixture.skip_turn_id(&skip).await);
        assert_eq!(full.previous_attempt_id(), Some(skip.attempt_id.as_str()));
        assert!(full.supersedes);
        assert_eq!(fixture.skipped_event_count().await, 1);
    }

    #[tokio::test]
    async fn crash_before_schedule_update_replays_skip_and_resumes_pending() {
        let fixture = AuditFixture::create().await;
        let skip = fixture.commit_skip().await;
        let before = fixture.event_count().await;
        let crashed = fixture.state().await;
        assert!(crashed.schedule_sync_pending, "commit crashed before Schedule");

        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "skipped");
        assert_eq!(
            replay.committed_skip().map(|skip| skip.attempt_id.as_str()),
            Some(skip.attempt_id.as_str())
        );
        assert_eq!(fixture.event_count().await, before);

        for pending in [
            tinyiothub_storage::suppression::SyncPending::Schedule,
            tinyiothub_storage::suppression::SyncPending::Projection,
            tinyiothub_storage::suppression::SyncPending::Notification,
        ] {
            fixture
                .db
                .clear_sync_pending(WS, SESSION, pending)
                .await
                .expect("reconcile pending barrier");
        }
        let settled = fixture.replay().await;
        assert_eq!(settled.outcome(), "skipped");
        assert_eq!(
            fixture.event_count().await,
            before,
            "reconciliation did not create a Turn"
        );
    }

    #[tokio::test]
    async fn duplicate_attempt_event_ids_do_not_create_duplicate_events() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("begin skip attempt");
        let events =
            super::skipped_lifecycle_events(&fixture.wake, attempt.attempt_no, "state-1", fixture.checked_at());
        let committed = fixture.db.commit_skip(&attempt, &events).await.expect("first commit");
        assert_eq!(committed.events.len(), 6);

        // Replaying the same attempt identity must not append a second skipped
        // lifecycle; the event ids are already owned by this attempt.
        let again = fixture.db.commit_skip(&attempt, &events).await;
        assert!(matches!(again, Err(tinyiothub_storage::DbError::CasConflict { .. })));
        assert_eq!(fixture.event_count().await, 6);
        assert_eq!(fixture.skipped_event_count().await, 1);
        assert!(decision.would_skip);
    }

    #[tokio::test]
    async fn transaction_rollback_leaves_no_partial_skip_lifecycle() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("begin skip attempt");

        sqlx::query(
            "UPDATE suppression_state SET state_version = state_version + 1 \
             WHERE workspace_id = ? AND session_id = ?",
        )
        .bind(WS)
        .bind(SESSION)
        .execute(&fixture.workspace)
        .await
        .expect("advance state version");

        let error = AtomicSkipAudit::new(fixture.db.clone())
            .commit(SkipAuditRequest {
                wake: &fixture.wake,
                attempt: &attempt,
                decision: &decision,
                checked_at: fixture.checked_at(),
            })
            .await
            .expect_err("stale audit state must roll back");
        assert!(matches!(error, tinyiothub_storage::DbError::CasConflict { .. }));
        assert_eq!(fixture.event_count().await, 0, "no partial skipped lifecycle");
        let fenced = fixture.state().await;
        assert_eq!(fenced.latest_outcome, "skip_in_progress");
        assert!(fenced.audit_in_progress);
        assert!(!fenced.baseline_is_eligible());
    }

    #[tokio::test]
    async fn audit_failure_fence_blocks_the_next_skip() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("begin skip attempt");
        sqlx::query(
            "UPDATE suppression_state SET full_failed = 1 \
             WHERE workspace_id = ? AND session_id = ?",
        )
        .bind(WS)
        .bind(SESSION)
        .execute(&fixture.workspace)
        .await
        .expect("set another fence");

        AtomicSkipAudit::new(fixture.db.clone())
            .commit(SkipAuditRequest {
                wake: &fixture.wake,
                attempt: &attempt,
                decision: &decision,
                checked_at: fixture.checked_at(),
            })
            .await
            .expect_err("another fence rejects the commit");

        let settled = fixture
            .db
            .fail_skip_attempt(WS, SESSION, &attempt.attempt_id, "audit_write_failed")
            .await
            .expect("settle the failed audit");
        assert_eq!(settled.latest_outcome, "audit_failed");
        assert!(!settled.audit_in_progress);
        assert!(!settled.baseline_is_eligible());

        let next = fixture.decision(60).await;
        assert!(!next.would_skip);
        assert!(
            next.vetoes
                .contains(&crate::domains::agent::session::suppression::preflight::SkipVeto::AuditFailed)
        );
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn off_before_commit_rejects_the_stale_skip() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("begin skip attempt");
        fixture
            .db
            .update_suppression_config(WS, Some(SuppressionMode::Off), None)
            .await
            .expect("off")
            .expect("schedule row");

        let error = AtomicSkipAudit::new(fixture.db.clone())
            .commit(SkipAuditRequest {
                wake: &fixture.wake,
                attempt: &attempt,
                decision: &decision,
                checked_at: fixture.checked_at(),
            })
            .await
            .expect_err("off must reject a stale skip");
        assert!(matches!(error, tinyiothub_storage::DbError::CasConflict { .. }));
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn user_message_before_commit_rejects_the_stale_skip() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("begin skip attempt");
        fixture.append_user_message("m-1").await;

        let error = AtomicSkipAudit::new(fixture.db.clone())
            .commit(SkipAuditRequest {
                wake: &fixture.wake,
                attempt: &attempt,
                decision: &decision,
                checked_at: fixture.checked_at(),
            })
            .await
            .expect_err("new user input must reject the skip");
        assert!(matches!(error, tinyiothub_storage::DbError::CasConflict { .. }));
        assert_eq!(fixture.event_count().await, 1, "only the user message remains");
    }

    #[tokio::test]
    async fn registry_or_config_change_before_commit_rejects_the_stale_skip() {
        for change in ["registry", "config"] {
            let fixture = AuditFixture::create().await;
            let decision = fixture.decision(60).await;
            let state = fixture.state().await;
            let attempt = fixture
                .db
                .begin_skip_attempt(SESSION, &state)
                .await
                .expect("begin skip attempt");
            match change {
                "registry" => {
                    sqlx::query(
                        "UPDATE suppression_state SET registry_version = ? \
                         WHERE workspace_id = ? AND session_id = ?",
                    )
                    .bind("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
                    .bind(WS)
                    .bind(SESSION)
                    .execute(&fixture.workspace)
                    .await
                    .expect("change registry");
                }
                _ => {
                    fixture
                        .db
                        .update_suppression_config(WS, None, Some(600))
                        .await
                        .expect("change config")
                        .expect("schedule row");
                }
            }

            let error = AtomicSkipAudit::new(fixture.db.clone())
                .commit(SkipAuditRequest {
                    wake: &fixture.wake,
                    attempt: &attempt,
                    decision: &decision,
                    checked_at: fixture.checked_at(),
                })
                .await
                .expect_err("a version change must reject the skip");
            assert!(
                matches!(error, tinyiothub_storage::DbError::CasConflict { .. }),
                "case={change}"
            );
            assert_eq!(fixture.event_count().await, 0, "case={change}");
        }
    }

    #[tokio::test]
    async fn concurrent_audit_attempts_have_one_winner() {
        let fixture = AuditFixture::create().await;
        let state = fixture.state().await;
        let (left, right) = tokio::join!(
            fixture.db.begin_skip_attempt(SESSION, &state),
            fixture.db.begin_skip_attempt(SESSION, &state)
        );
        let results = [left, right];
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "only one audit attempt may own the CAS"
        );
        let loser = results.into_iter().find(|result| result.is_err()).expect("loser");
        assert!(matches!(loser, Err(tinyiothub_storage::DbError::CasConflict { .. })));
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn first_replay_without_a_prior_skip_is_skip_eligible() {
        let fixture = AuditFixture::create().await;
        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "skipped");
        assert!(replay.committed_skip().is_none());
        assert_eq!(replay.attempt_no, fixture.state().await.attempt_no + 1);
        assert!(replay.turn_id().starts_with("turn:"));
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn shadow_would_skip_cannot_write_an_audit_fence() {
        let fixture = AuditFixture::create_with_mode(SuppressionMode::Shadow).await;
        let decision = fixture.decision(60).await;
        assert!(decision.would_skip, "shadow computes wouldSkip");
        assert_eq!(decision.mode, SuppressionMode::Shadow);
        assert!(!decision.mode.allows_skip());

        let error = AtomicSkipAudit::ensure_skip_allowed(&decision).expect_err("shadow may not skip");
        assert!(matches!(error, tinyiothub_storage::DbError::Validation { .. }));

        let before = fixture.state().await;
        let error = fixture
            .db
            .begin_skip_attempt(SESSION, &before)
            .await
            .expect_err("shadow may not write the audit fence");
        assert!(matches!(error, tinyiothub_storage::DbError::CasConflict { .. }));

        let after = fixture.state().await;
        assert!(!after.audit_in_progress, "no durable audit fence in shadow");
        assert_eq!(after.attempt_no, before.attempt_no);
        assert_eq!(after.latest_outcome, "full_completed");
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn canary_and_enforce_commit_atomic_skips() {
        for mode in [SuppressionMode::Canary, SuppressionMode::Enforce] {
            let fixture = AuditFixture::create_with_mode(mode).await;
            let decision = fixture.decision(60).await;
            assert!(decision.would_skip, "mode={}", mode.as_str());
            assert_eq!(decision.mode, mode);

            let skip = fixture.commit_skip().await;
            assert_eq!(skip.state.latest_outcome, OUTCOME_SKIPPED);
            assert_eq!(fixture.skipped_event_count().await, 1, "mode={}", mode.as_str());
        }
    }

    #[tokio::test]
    async fn shadow_replay_is_a_full_turn_not_skip_eligible() {
        let fixture = AuditFixture::create_with_mode(SuppressionMode::Shadow).await;
        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "completed");
        assert!(replay.committed_skip().is_none());
        assert_eq!(fixture.event_count().await, 0);
    }

    #[tokio::test]
    async fn commit_rejects_a_shadow_decision_before_touching_the_transaction() {
        let fixture = AuditFixture::create().await;
        let decision = fixture.decision(60).await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("enforce attempt");

        let mut shadow = decision.clone();
        shadow.mode = SuppressionMode::Shadow;
        let error = AtomicSkipAudit::new(fixture.db.clone())
            .commit(SkipAuditRequest {
                wake: &fixture.wake,
                attempt: &attempt,
                decision: &shadow,
                checked_at: fixture.checked_at(),
            })
            .await
            .expect_err("commit must reject shadow before any write");
        assert!(matches!(error, tinyiothub_storage::DbError::Validation { .. }));
        assert_eq!(fixture.event_count().await, 0);
        assert!(fixture.state().await.audit_in_progress);
    }

    #[tokio::test]
    async fn committed_skip_replays_when_lifecycle_events_have_a_foreign_run_id() {
        let fixture = AuditFixture::create().await;
        let state = fixture.state().await;
        let predicted_attempt = state.attempt_no + 1;
        let lifecycle =
            super::skipped_lifecycle_events(&fixture.wake, predicted_attempt, "state-1", fixture.checked_at());
        // Simulate pre-fix wake emissions: same deterministic event ids and
        // payloads, but the previous per-invocation run id.
        for event in lifecycle.iter().take(3) {
            let mut legacy = event.clone();
            legacy.run_id = Some("legacy-run-id".to_string());
            fixture
                .db
                .append_session_event(&legacy)
                .await
                .expect("legacy lifecycle event");
        }

        let skip = fixture.commit_skip().await;
        assert_eq!(
            skip.events[0].run_id.as_deref(),
            Some("legacy-run-id"),
            "duplicate-id append keeps the pre-existing row"
        );
        assert_ne!(
            skip.events[3].run_id.as_deref(),
            Some("legacy-run-id"),
            "newly appended events use the attempt run id"
        );

        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "skipped");
        assert_eq!(
            replay.committed_skip().map(|skip| skip.attempt_id.as_str()),
            Some(skip.attempt_id.as_str())
        );
        assert_eq!(fixture.event_count().await, 6);
        assert_eq!(fixture.skipped_event_count().await, 1);
    }

    #[test]
    fn replay_outcome_wire_values_are_stable() {
        assert_eq!(ReplayOutcome::Skipped.as_str(), "skipped");
        assert_eq!(ReplayOutcome::Completed.as_str(), "completed");
        let _: WakeAttempt = PlannedWake::planned(WS, SESSION, ts()).attempt(7);
    }

    struct FailingNotifier {
        calls: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl super::SuppressionNotifier for FailingNotifier {
        async fn notify(&self, _workspace_id: &str, _session_id: &str) -> std::result::Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err("sse notify failed".to_string())
        }
    }

    fn notifier_calls() -> (Arc<AtomicU32>, Arc<FailingNotifier>) {
        let calls = Arc::new(AtomicU32::new(0));
        (calls.clone(), Arc::new(FailingNotifier { calls }))
    }

    #[tokio::test]
    async fn startup_recovery_settles_a_stale_full_attempt_and_permits_the_next_full_turn() {
        let fixture = AuditFixture::create().await;
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_full_attempt(SESSION, &state)
            .await
            .expect("stale full attempt");

        let report = super::SuppressionRecovery::with_noop_notifier(fixture.db.clone())
            .recover_session(WS, SESSION)
            .await
            .expect("recovery");

        assert_eq!(
            report.settled_full_attempt.as_deref(),
            Some(attempt.attempt_id.as_str())
        );
        assert!(report.alerts.is_empty(), "alerts={:?}", report.alerts);
        let recovered = fixture.state().await;
        assert!(!recovered.full_attempt_in_progress);
        assert!(recovered.full_failed);
        assert_eq!(recovered.latest_outcome, "full_failed");
        assert_eq!(
            recovered.attempt_no, attempt.attempt_no,
            "recovery settles the stale attempt instead of inventing a new one"
        );
        assert!(!recovered.baseline_is_eligible());
        // A stale attempt must not block the next full Turn.
        let next = fixture
            .db
            .begin_full_attempt(SESSION, &recovered)
            .await
            .expect("next full attempt");
        assert_eq!(next.attempt_no, attempt.attempt_no + 1);
    }

    #[tokio::test]
    async fn startup_recovery_settles_a_stale_audit_fence_to_audit_failed() {
        let fixture = AuditFixture::create().await;
        let state = fixture.state().await;
        let skip = fixture
            .db
            .begin_skip_attempt(SESSION, &state)
            .await
            .expect("stale audit fence");

        let report = super::SuppressionRecovery::with_noop_notifier(fixture.db.clone())
            .recover_session(WS, SESSION)
            .await
            .expect("recovery");

        assert_eq!(report.settled_audit_attempt.as_deref(), Some(skip.attempt_id.as_str()));
        let recovered = fixture.state().await;
        assert!(!recovered.audit_in_progress);
        assert_eq!(recovered.latest_outcome, "audit_failed");
        assert_eq!(recovered.last_error.as_deref(), Some("audit_write_failed"));
        assert!(!recovered.baseline_is_eligible());
        assert_eq!(fixture.event_count().await, 0, "recovery never writes a partial skip");

        let decision = fixture.decision(60).await;
        assert!(!decision.would_skip);
        assert!(
            decision
                .vetoes
                .contains(&crate::domains::agent::session::suppression::preflight::SkipVeto::AuditFailed)
        );
    }

    #[tokio::test]
    async fn startup_recovery_replays_a_committed_skip_and_clears_consumers() {
        let fixture = AuditFixture::create().await;
        let skip = fixture.commit_skip().await;
        let before = fixture.event_count().await;
        fixture
            .db
            .mark_assistant_wake(WS, fixture.checked_at(), fixture.checked_at(), "skipped")
            .await
            .expect("schedule already acknowledged the skip");

        let report = super::SuppressionRecovery::with_noop_notifier(fixture.db.clone())
            .recover_session(WS, SESSION)
            .await
            .expect("recovery");

        assert!(report.replayed_committed_skip);
        assert!(report.cleared.contains(&"schedule".to_string()), "{:?}", report.cleared);
        assert!(
            report.cleared.contains(&"projection".to_string()),
            "{:?}",
            report.cleared
        );
        assert!(
            report.cleared.contains(&"notification".to_string()),
            "{:?}",
            report.cleared
        );
        assert!(report.alerts.is_empty(), "alerts={:?}", report.alerts);
        let recovered = fixture.state().await;
        assert!(!recovered.schedule_sync_pending);
        assert!(!recovered.projection_sync_pending);
        assert!(!recovered.notification_pending);
        assert_eq!(fixture.event_count().await, before, "replay writes no new events");

        // The committed skip still replays unchanged and never creates a Turn.
        let replay = fixture.replay().await;
        assert_eq!(replay.outcome(), "skipped");
        assert_eq!(
            replay.committed_skip().map(|skip| skip.attempt_id.as_str()),
            Some(skip.attempt_id.as_str())
        );
        assert_eq!(fixture.event_count().await, before);
    }

    #[tokio::test]
    async fn startup_recovery_keeps_schedule_committed_partial_until_the_writer_succeeds() {
        let fixture = AuditFixture::create().await;
        fixture.commit_skip().await;

        let report = super::SuppressionRecovery::with_noop_notifier(fixture.db.clone())
            .recover_session(WS, SESSION)
            .await
            .expect("recovery");
        assert!(report.replayed_committed_skip);
        assert!(
            report
                .alerts
                .iter()
                .any(|alert| alert.code == "schedule_barrier_failed"),
            "alerts={:?}",
            report.alerts
        );
        let partial = fixture.state().await;
        assert!(
            partial.schedule_sync_pending,
            "Schedule stays committed_partial until its exclusive writer succeeds"
        );
        assert!(!partial.projection_sync_pending);
        assert!(!partial.notification_pending);

        // The TriggerRouter writer confirms the skip; the next recovery clears
        // the remaining barrier without creating any new Turn.
        fixture
            .db
            .mark_assistant_wake(WS, fixture.checked_at(), fixture.checked_at(), "skipped")
            .await
            .expect("schedule writer");
        let settled = super::SuppressionRecovery::with_noop_notifier(fixture.db.clone())
            .recover_session(WS, SESSION)
            .await
            .expect("second recovery");
        assert!(settled.cleared.contains(&"schedule".to_string()));
        assert!(
            !settled
                .alerts
                .iter()
                .any(|alert| alert.code == "schedule_barrier_failed")
        );
        assert!(!fixture.state().await.schedule_sync_pending);
    }

    // Real time is intentional: SQLx pool acquisition uses timers, and a
    // paused clock makes an in-memory connection acquisition time out before
    // it can complete. The exact 1s/2s bound is proven without a DB in
    // `suppression::error`.
    #[tokio::test]
    async fn post_commit_notification_failure_never_rolls_back_events_and_alerts() {
        let fixture = AuditFixture::create().await;
        fixture.commit_skip().await;
        let before = fixture.event_count().await;
        let (calls, notifier) = notifier_calls();

        let report = super::SuppressionRecovery::new(fixture.db.clone(), notifier)
            .recover_session(WS, SESSION)
            .await
            .expect("recovery");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "notification recovery uses the three-total-attempt bound"
        );
        assert_eq!(
            fixture.event_count().await,
            before,
            "committed events are never rolled back by an SSE notification failure"
        );
        assert!(
            fixture.state().await.notification_pending,
            "client resumes from high watermark"
        );
        assert!(
            report.alerts.iter().any(|alert| alert.code == "notification_failed"),
            "alerts={:?}",
            report.alerts
        );
    }

    #[tokio::test]
    async fn off_mode_reconcile_does_not_leave_an_active_alert() {
        let fixture = AuditFixture::create().await;
        fixture.commit_skip().await;
        // Normal consumers have caught up; only the deliberate off-mode
        // reconcile follows.
        for pending in [
            tinyiothub_storage::suppression::SyncPending::Schedule,
            tinyiothub_storage::suppression::SyncPending::Projection,
            tinyiothub_storage::suppression::SyncPending::Notification,
        ] {
            fixture
                .db
                .clear_sync_pending(WS, SESSION, pending)
                .await
                .expect("consumer settled");
        }
        fixture
            .db
            .update_suppression_config(WS, Some(SuppressionMode::Off), None)
            .await
            .expect("off before reconcile")
            .expect("schedule row");

        // A deliberate off-mode reconcile allocates a durable attempt and
        // settles it without installing a baseline.
        let state = fixture.state().await;
        let attempt = fixture
            .db
            .begin_full_attempt(SESSION, &state)
            .await
            .expect("off-mode attempt");
        let settled = fixture
            .db
            .settle_full_attempt_without_baseline(WS, &attempt.attempt_id, "full_completed")
            .await
            .expect("settlement");

        assert!(settled.full_failed, "the old baseline stays ineligible");
        assert!(settled.last_error.is_none());
        let activity = crate::domains::agent::session::suppression::error::alert_activity(&settled);
        assert!(
            !activity.active,
            "a deliberate off-mode reconcile must not leave alerts.active true"
        );
        assert!(activity.code.is_none());

        let decision = fixture.decision(60).await;
        assert!(!decision.would_skip, "off keeps the kill switch immediate");
        assert!(
            decision
                .vetoes
                .contains(&crate::domains::agent::session::suppression::preflight::SkipVeto::Disabled)
        );
    }
}
