//! Production bridge from a Prepared Harness Effect to a physical Device Job.
//!
//! The Harness effect pipeline owns the state machine; the legacy
//! `JobService` still owns the physical worker call until Task 19C3 moves the
//! Job store. This module is the only place that joins them:
//!
//! ```text
//! Prepared Effect (no approval / approved Decision)
//!          │  reducer: prepared -> dispatching (dispatch permit, external_ref)
//!          ▼
//! WorkflowControls::dispatch_permitted  ──false──► not_applied
//!          │ true
//!          ▼
//! JobService::submit  ──validation/conflict──► not_applied
//!          │ accepted
//!          ▼
//! physical device action
//!          │  JobResultSink command_finished / command_cancelled
//!          ▼
//! reducer: dispatching -> completed | failed | cancelling -> cancelled(_unknown)
//! ```
//!
//! A `dispatching` commit is the only physical send permit and it always
//! happens before `JobService::submit`. That is what makes pause/cancel gates
//! effective and what keeps a crash between permit and send from blindly
//! re-sending: the recovery scanner fences the effect to `unknown` instead.
//!
//! `request_json` is internal dispatch input only. Event/SSE payloads emitted
//! here carry the effect id, state, and Job result, never the raw request.
//! Approval-consumption transitions also persist the Decision mutation (whose
//! `subject_json` pre-dates this task); that is internal harness state, not an
//! event payload.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use tinyiothub_agent::harness::{RecoveryFact, reduce, reduce_recovery};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
use tinyiothub_core::harness::{
    Decision, DecisionKind, DecisionState, EffectIntent, EffectState, ExecutionMode, HarnessFact, HarnessState,
    JobProjection, Transition, WakeProjection,
};
use tinyiothub_runtime::job::service::{JobRequest, JobService};
use tinyiothub_runtime::job::worker_adapter::JobResultSink;
use tinyiothub_storage::Db;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;

use super::EffectSender;
use super::controls::WorkflowControls;
use super::harness_stream::HarnessStreamHub;

/// Dispatch cadence. Same family as the wake router (25 ms claim loop); a
/// 250 ms per-workspace sweep is a bounded recovery pass, not a hot loop.
pub const DISPATCH_INTERVAL: Duration = Duration::from_millis(250);

/// Physical Job id derived from the Effect id. A dispatch attempt therefore
/// reuses one physical identity instead of minting a new one.
pub fn job_id_for(effect_id: &str) -> String {
    format!("effect:{effect_id}")
}

fn effect_id_from_job(job_id: &str) -> Option<&str> {
    job_id.strip_prefix("effect:")
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DispatchReport {
    /// Effects whose Job was submitted during this pass.
    pub dispatched: usize,
    /// Prepared Effects deliberately left alone (approval pending, paused or
    /// cancelling Task).
    pub blocked: usize,
    /// Effects settled to `not_applied` without a physical send.
    pub not_applied: usize,
}

/// `EffectIntent.request_json` is the frozen execution request. Both fields
/// are required; disagreeing with them is a validation failure, never a guess.
#[derive(Debug, Clone)]
struct EffectRequest {
    device_id: String,
    args_json: Value,
}

impl EffectRequest {
    fn parse(intent: &EffectIntent) -> Result<Self, String> {
        let device_id = intent
            .request_json
            .get("deviceId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "effect request is missing deviceId".to_string())?;
        let args_json = intent
            .request_json
            .get("argsJson")
            .filter(|value| !value.is_null())
            .cloned()
            .ok_or_else(|| "effect request is missing argsJson".to_string())?;
        Ok(Self {
            device_id: device_id.to_string(),
            args_json,
        })
    }
}

/// `EffectSender` implementation over the legacy `JobService`.
///
/// The high-level dispatcher uses [`JobEffectSender::submit`] so it can keep
/// the `DeviceJobError` variant; the trait keeps the existing test seam and
/// future callers working.
#[derive(Clone)]
pub struct JobEffectSender {
    db: Arc<Db>,
    jobs: JobService,
}

impl JobEffectSender {
    pub fn new(db: Arc<Db>, jobs: JobService) -> Self {
        Self { db, jobs }
    }

    pub async fn submit(&self, workspace_id: &str, intent: &EffectIntent) -> Result<DeviceJob, DeviceJobError> {
        let request = EffectRequest::parse(intent).map_err(DeviceJobError::Validation)?;
        self.jobs
            .submit(JobRequest {
                job_id: Some(job_id_for(&intent.effect_id)),
                workspace_id: workspace_id.to_string(),
                device_id: request.device_id,
                tool: intent.tool_name.clone(),
                args_json: request.args_json,
                task_id: Some(intent.task_id.clone()),
                run_id: Some(intent.run_id.clone()),
                tool_call_id: Some(intent.tool_call_id.clone()),
            })
            .await
    }
}

#[async_trait]
impl EffectSender for JobEffectSender {
    async fn send(&self, intent: &EffectIntent) -> Result<(), String> {
        // The trait carries no workspace argument; the Effect id is unique per
        // workspace, so resolve the owning workspace without guessing.
        let workspaces = self
            .db
            .find_all_workspace_ids()
            .await
            .map_err(|error| error.to_string())?;
        for workspace_id in workspaces {
            match self.db.effect_intent(&workspace_id, &intent.effect_id).await {
                Ok(Some(_)) => {
                    return self
                        .submit(&workspace_id, intent)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string());
                }
                Ok(None) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Err(format!("effect {} has no owning workspace", intent.effect_id))
    }
}

/// Per-workspace dispatch pass plus the shared Job-completion settler.
#[derive(Clone)]
pub struct EffectDispatch {
    db: Arc<Db>,
    jobs: JobService,
    stream: Option<Arc<HarnessStreamHub>>,
    notify: Arc<tokio::sync::Notify>,
}

impl EffectDispatch {
    pub fn new(db: Arc<Db>, jobs: JobService) -> Self {
        Self {
            db,
            jobs,
            stream: None,
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn with_stream(mut self, stream: Arc<HarnessStreamHub>) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Wake the dispatcher loop. Writers call this after a committed fact that
    /// can unblock a Prepared Effect (the approval HTTP answer does).
    pub fn notify(&self) {
        self.notify.notify_one();
    }

    pub fn sender(&self) -> JobEffectSender {
        JobEffectSender::new(self.db.clone(), self.jobs.clone())
    }

    fn publish(&self, workspace_id: &str) {
        if let Some(stream) = &self.stream {
            stream.notify(workspace_id);
        }
    }

    async fn apply(&self, workspace_id: &str, mut transition: Transition) -> Result<(), String> {
        if let Some(event) = &mut transition.event {
            event.workspace_id = workspace_id.to_string();
        }
        for event in &mut transition.additional_events {
            event.workspace_id = workspace_id.to_string();
        }
        let precondition = transition.precondition.clone();
        self.db
            .apply_workspace_transition(workspace_id, &precondition, &transition)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn harness_state(workspace_id: &str, intent: &EffectIntent, status: EffectState, version: i64) -> HarnessState {
        HarnessState {
            workspace_id: workspace_id.to_string(),
            effect_id: intent.effect_id.clone(),
            session_id: intent.session_id.clone(),
            task_id: intent.task_id.clone(),
            run_id: intent.run_id.clone(),
            execution_mode: ExecutionMode::Production,
            status,
            state_version: version,
        }
    }

    async fn transition_from_prepared_not_applied(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        decision: Option<&Decision>,
        transition_id: &str,
        reason: Value,
    ) -> Result<(), String> {
        let mut transition = if let Some(decision) = decision {
            reduce_recovery(
                intent,
                Some(decision),
                None,
                transition_id,
                Utc::now(),
                "",
                RecoveryFact::ConsumeApproval,
            )
            .map_err(|error| error.to_string())?
        } else {
            reduce(
                &Self::harness_state(workspace_id, intent, EffectState::Prepared, intent.state_version),
                &HarnessFact {
                    transition_id: transition_id.to_string(),
                    effect_id: intent.effect_id.clone(),
                    next: EffectState::NotApplied,
                    occurred_at: Utc::now(),
                    external_ref: None,
                    result: Some(reason.clone()),
                    job: None,
                    wake: None,
                },
            )
            .map_err(|error| error.to_string())?
        };
        if let Some(effect) = &mut transition.effect {
            effect.result = Some(reason);
        }
        self.apply(workspace_id, transition).await?;
        self.publish(workspace_id);
        Ok(())
    }

    /// Settle a Prepared Effect whose owning Task already reached a terminal
    /// shape. It must never dispatch (carried from Task 19B N6).
    async fn settle_terminal_task(&self, workspace_id: &str, item: &EffectRecoveryItem) -> Result<(), String> {
        let transition_id = format!("dispatch:{}:terminal_task", item.intent.effect_id);
        let mut transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            None,
            &transition_id,
            Utc::now(),
            &item.task_state,
            RecoveryFact::Cancel,
        )
        .map_err(|error| error.to_string())?;
        // The Task is already terminal; do not rewrite its Run or enqueue a
        // wake for it. Only the Effect needs to settle.
        transition.run_task = None;
        transition.wake = None;
        let reason = json!({"reason": "owning_task_terminal", "taskState": item.task_state});
        if let Some(effect) = &mut transition.effect {
            effect.result = Some(reason);
        }
        self.apply(workspace_id, transition).await?;
        self.publish(workspace_id);
        Ok(())
    }

    async fn transition_dispatching_not_applied(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        reason: Value,
    ) -> Result<(), String> {
        let Some(current) = self
            .db
            .effect_intent(workspace_id, &intent.effect_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        if current.state != EffectState::Dispatching {
            return Ok(());
        }
        let transition = reduce(
            &Self::harness_state(workspace_id, &current, EffectState::Dispatching, current.state_version),
            &HarnessFact {
                transition_id: format!("dispatch:{}:not_applied", current.effect_id),
                effect_id: current.effect_id.clone(),
                next: EffectState::NotApplied,
                occurred_at: Utc::now(),
                external_ref: None,
                result: Some(reason),
                job: None,
                wake: Some(WakeProjection {
                    task_id: current.task_id.clone(),
                    wake_id: format!("dispatch:{}:not_applied:wake", current.effect_id),
                    signal_json: json!({
                        "effectId": current.effect_id,
                        "state": EffectState::NotApplied.as_str(),
                    })
                    .to_string(),
                    created_at: Utc::now(),
                }),
            },
        )
        .map_err(|error| error.to_string())?;
        self.apply(workspace_id, transition).await?;
        self.publish(workspace_id);
        Ok(())
    }

    async fn dispatch_prepared(&self, workspace_id: &str, item: &EffectRecoveryItem) -> Result<DispatchReport, String> {
        let intent = &item.intent;
        if matches!(item.task_state.as_str(), "completed" | "failed" | "cancelled") {
            self.settle_terminal_task(workspace_id, item).await?;
            return Ok(DispatchReport {
                not_applied: 1,
                ..DispatchReport::default()
            });
        }
        if matches!(item.task_state.as_str(), "paused" | "cancelling") {
            return Ok(DispatchReport {
                blocked: 1,
                ..DispatchReport::default()
            });
        }

        // Approval gate: an unanswered tool_approval always blocks. A denied
        // or closed approval settles the Effect without a physical send.
        match item.decision.as_ref() {
            None => {}
            Some(decision) if decision.kind == DecisionKind::ToolApproval => {
                match (decision.state, decision.answer.as_deref()) {
                    (DecisionState::Requested, _) => {
                        return Ok(DispatchReport {
                            blocked: 1,
                            ..DispatchReport::default()
                        });
                    }
                    (DecisionState::Answered, Some("approved")) => {}
                    (DecisionState::Answered, Some("denied")) => {
                        self.transition_from_prepared_not_applied(
                            workspace_id,
                            intent,
                            Some(decision),
                            &format!("dispatch:{}:denied", intent.effect_id),
                            json!({"reason": "denied"}),
                        )
                        .await?;
                        return Ok(DispatchReport {
                            not_applied: 1,
                            ..DispatchReport::default()
                        });
                    }
                    (DecisionState::Expired | DecisionState::Withdrawn, _) => {
                        self.transition_from_prepared_not_applied(
                            workspace_id,
                            intent,
                            Some(decision),
                            &format!("dispatch:{}:approval_closed", intent.effect_id),
                            json!({"reason": "approval_closed"}),
                        )
                        .await?;
                        return Ok(DispatchReport {
                            not_applied: 1,
                            ..DispatchReport::default()
                        });
                    }
                    _ => {
                        return Ok(DispatchReport {
                            blocked: 1,
                            ..DispatchReport::default()
                        });
                    }
                }
            }
            // Any other Decision kind on a Prepared Effect is outside the
            // recovery matrix (the guard already fails closed); never guess.
            Some(_) => {
                return Ok(DispatchReport {
                    blocked: 1,
                    ..DispatchReport::default()
                });
            }
        }

        if EffectRequest::parse(intent).is_err() {
            let reason =
                json!({"reason": "invalid_request", "error": "effect request is missing deviceId or argsJson"});
            if let Some(decision) = item.decision.as_ref() {
                // Consume the approval first (the matrix forbids leaving an
                // Answered approval on a terminal Effect); an approved
                // decision reaches Dispatching, so settle that permit too.
                let permit = reduce_recovery(
                    intent,
                    Some(decision),
                    None,
                    &format!("dispatch:{}:invalid_request", intent.effect_id),
                    Utc::now(),
                    &item.task_state,
                    RecoveryFact::ConsumeApproval,
                )
                .map_err(|error| error.to_string())?;
                let dispatching = permit
                    .effect
                    .as_ref()
                    .is_some_and(|effect| effect.state == EffectState::Dispatching);
                self.apply(workspace_id, permit).await?;
                if dispatching {
                    self.transition_dispatching_not_applied(workspace_id, intent, reason.clone())
                        .await?;
                }
            } else {
                self.transition_from_prepared_not_applied(
                    workspace_id,
                    intent,
                    None,
                    &format!("dispatch:{}:invalid_request", intent.effect_id),
                    reason,
                )
                .await?;
            }
            return Ok(DispatchReport {
                not_applied: 1,
                ..DispatchReport::default()
            });
        }

        let job_id = job_id_for(&intent.effect_id);
        let now = Utc::now();
        let transition_id = format!("dispatch:{}:{}", intent.effect_id, uuid::Uuid::new_v4());
        let permit = match item.decision.as_ref() {
            Some(decision) => {
                let mut transition = reduce_recovery(
                    intent,
                    Some(decision),
                    None,
                    &transition_id,
                    now,
                    &item.task_state,
                    RecoveryFact::ConsumeApproval,
                )
                .map_err(|error| error.to_string())?;
                // No Job exists yet; the Job store writes the workspace row on
                // submit. Carrying a JobProjection here would require a
                // pre-submit upsert, which can clobber a concurrent pass's row.
                transition.job = None;
                if let Some(effect) = &mut transition.effect {
                    effect.external_ref = Some(job_id.clone());
                    effect.result = None;
                }
                transition
            }
            None => reduce(
                &Self::harness_state(workspace_id, intent, EffectState::Prepared, intent.state_version),
                &HarnessFact {
                    transition_id: transition_id.clone(),
                    effect_id: intent.effect_id.clone(),
                    next: EffectState::Dispatching,
                    occurred_at: now,
                    external_ref: Some(job_id.clone()),
                    result: None,
                    job: None,
                    wake: None,
                },
            )
            .map_err(|error| error.to_string())?,
        };

        // A concurrent pass may win the CAS (or the store is unavailable); do
        // not send. No workspace Job row exists yet: the Job store only writes
        // it after a successful submit.
        self.apply(workspace_id, permit).await?;

        let controls = WorkflowControls::new(self.db.clone(), workspace_id);
        let permitted = match controls.dispatch_permitted(&intent.effect_id).await {
            Ok(permitted) => permitted,
            Err(error) => {
                // The permit is committed but the gate cannot be evaluated; no
                // send occurred, so settle deterministically.
                self.transition_dispatching_not_applied(
                    workspace_id,
                    intent,
                    json!({"reason": "dispatch_gate_error", "error": error.to_string()}),
                )
                .await?;
                return Err(error.to_string());
            }
        };
        if !permitted {
            self.transition_dispatching_not_applied(workspace_id, intent, json!({"reason": "dispatch_not_permitted"}))
                .await?;
            return Ok(DispatchReport {
                blocked: 1,
                ..DispatchReport::default()
            });
        }

        let sender = self.sender();
        match sender.submit(workspace_id, intent).await {
            Ok(job) => {
                // A cancel may have been requested after the permit gate but
                // before the submit. Re-check the Effect and stop the Job we
                // just created when it moved out of `dispatching`.
                if let Ok(Some(current)) = self.db.effect_intent(workspace_id, &intent.effect_id).await
                    && current.state != EffectState::Dispatching
                    && let Err(error) = self.jobs.stop(workspace_id, &intent.task_id, &job.id).await
                {
                    tracing::warn!(
                        %workspace_id,
                        job_id = %job.id,
                        %error,
                        "post-submit cancel stop failed"
                    );
                }
                self.publish(workspace_id);
                Ok(DispatchReport {
                    dispatched: 1,
                    ..DispatchReport::default()
                })
            }
            Err(DeviceJobError::Validation(message)) => {
                self.transition_dispatching_not_applied(
                    workspace_id,
                    intent,
                    json!({"reason": "dispatch_validation", "error": message}),
                )
                .await?;
                Ok(DispatchReport {
                    not_applied: 1,
                    ..DispatchReport::default()
                })
            }
            Err(DeviceJobError::ExecutionConflict) => {
                self.transition_dispatching_not_applied(workspace_id, intent, json!({"reason": "execution_conflict"}))
                    .await?;
                Ok(DispatchReport {
                    not_applied: 1,
                    ..DispatchReport::default()
                })
            }
            Err(DeviceJobError::Conflict(message)) => {
                // Store-level dedupe: the deterministic Job id already exists.
                // Reuse it only when the frozen identity matches; never
                // re-invoke the worker and never delete a live Job row.
                match self.jobs.job(workspace_id, &job_id).await {
                    Ok(Some(existing)) if same_job_identity(&existing, workspace_id, intent) => {
                        self.publish(workspace_id);
                        Ok(DispatchReport {
                            dispatched: 1,
                            ..DispatchReport::default()
                        })
                    }
                    Ok(_) => {
                        tracing::warn!(%workspace_id, %job_id, %message, "dispatch job identity conflict");
                        self.transition_dispatching_not_applied(
                            workspace_id,
                            intent,
                            json!({"reason": "dispatch_identity_conflict", "error": message}),
                        )
                        .await?;
                        Ok(DispatchReport {
                            not_applied: 1,
                            ..DispatchReport::default()
                        })
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
            Err(error) => {
                // Storage failure: the deterministic Job row could not be
                // created, so the worker was never called. Settle the permit
                // deterministically instead of stranding `dispatching` with no
                // Job for a recovery scanner that does not exist yet.
                tracing::warn!(%workspace_id, %job_id, %error, "device job submit failed");
                self.transition_dispatching_not_applied(
                    workspace_id,
                    intent,
                    json!({"reason": "dispatch_storage", "error": error.to_string()}),
                )
                .await?;
                Ok(DispatchReport {
                    not_applied: 1,
                    ..DispatchReport::default()
                })
            }
        }
    }

    /// One bounded pass over one workspace's active Effects.
    pub async fn dispatch_workspace(&self, workspace_id: &str) -> Result<DispatchReport, String> {
        self.dispatch_scope(workspace_id, None).await
    }

    /// One bounded pass scoped to a single Task. Approval answers use this so
    /// an admin request cannot block on unrelated Effects' arbitration
    /// timeouts.
    pub async fn dispatch_task(&self, workspace_id: &str, task_id: &str) -> Result<DispatchReport, String> {
        self.dispatch_scope(workspace_id, Some(task_id)).await
    }

    async fn dispatch_scope(&self, workspace_id: &str, task_id: Option<&str>) -> Result<DispatchReport, String> {
        self.db
            .guard_recovery_matrix(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let items = self
            .db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let mut report = DispatchReport::default();
        for item in items {
            if item.intent.state != EffectState::Prepared {
                // Only Prepared Effects can acquire a new dispatch permit.
                continue;
            }
            if task_id.is_some_and(|task_id| item.intent.task_id != task_id) {
                continue;
            }
            match self.dispatch_prepared(workspace_id, &item).await {
                Ok(part) => {
                    report.dispatched += part.dispatched;
                    report.blocked += part.blocked;
                    report.not_applied += part.not_applied;
                }
                Err(error) => {
                    tracing::warn!(
                        %workspace_id,
                        effect_id = %item.intent.effect_id,
                        %error,
                        "effect dispatch failed"
                    );
                }
            }
        }
        Ok(report)
    }

    /// One-time startup reconciliation for Effects stranded by a crash between
    /// the dispatch permit and the Job row. It must run once, not on the
    /// periodic pass: `EffectRecovery::recover_workspace` fences every
    /// `dispatching` Effect it sees, which would fence live in-flight Jobs.
    ///
    /// - `dispatching` with no Job row: the send never happened -> `not_applied`.
    /// - `dispatching` with a terminal Job: settle from the Job fact.
    /// - `dispatching` with an active Job: leave it to the Job/worker.
    /// - `cancelling` with no Job row: cancel settled before any send.
    pub async fn reconcile_startup(&self) {
        let workspaces = match self.db.find_all_workspace_ids().await {
            Ok(workspaces) => workspaces,
            Err(error) => {
                tracing::warn!(%error, "startup effect reconciliation workspace listing failed");
                return;
            }
        };
        for workspace_id in workspaces {
            if let Err(error) = self.reconcile_workspace(&workspace_id).await {
                tracing::warn!(%workspace_id, %error, "startup effect reconciliation failed");
            }
        }
    }

    async fn reconcile_workspace(&self, workspace_id: &str) -> Result<(), String> {
        let items = self
            .db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        for item in items {
            match item.intent.state {
                EffectState::Dispatching => {
                    let job_id = job_id_for(&item.intent.effect_id);
                    match self.jobs.job(workspace_id, &job_id).await {
                        Ok(Some(job)) if job.state.is_terminal() => {
                            self.settle_job(workspace_id, &job_id).await?;
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            self.transition_dispatching_not_applied(
                                workspace_id,
                                &item.intent,
                                json!({"reason": "dispatch_crash_no_job"}),
                            )
                            .await?;
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                }
                EffectState::Cancelling => {
                    let job_id = job_id_for(&item.intent.effect_id);
                    match self.jobs.job(workspace_id, &job_id).await {
                        Ok(Some(job)) if job.state.is_terminal() => {
                            self.settle_job(workspace_id, &job_id).await?;
                        }
                        Ok(None) => self.settle_cancelled_before_send(workspace_id, &item.intent).await?,
                        Ok(Some(_)) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn settle_cancelled_before_send(&self, workspace_id: &str, intent: &EffectIntent) -> Result<(), String> {
        let now = Utc::now();
        let transition = reduce(
            &Self::harness_state(workspace_id, intent, EffectState::Cancelling, intent.state_version),
            &HarnessFact {
                transition_id: format!("reconcile:{}:cancelled", intent.effect_id),
                effect_id: intent.effect_id.clone(),
                next: EffectState::Cancelled,
                occurred_at: now,
                external_ref: None,
                result: Some(json!({"reason": "cancelled_before_send"})),
                job: None,
                wake: Some(WakeProjection {
                    task_id: intent.task_id.clone(),
                    wake_id: format!("reconcile:{}:cancelled:wake", intent.effect_id),
                    signal_json: json!({
                        "effectId": intent.effect_id,
                        "state": EffectState::Cancelled.as_str(),
                    })
                    .to_string(),
                    created_at: now,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
        self.apply(workspace_id, transition).await?;
        self.publish(workspace_id);
        Ok(())
    }

    /// Settle one completed physical Job into its Effect. Reads the Job from
    /// the shared `JobService` store (workspace generation for Effect Jobs)
    /// and never re-sends.
    pub async fn settle_job(&self, workspace_id: &str, job_id: &str) -> Result<(), String> {
        let Some(job) = self
            .jobs
            .job(workspace_id, job_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        if !job.state.is_terminal() {
            return Ok(());
        }
        let Some(effect_id) = effect_id_from_job(job_id) else {
            // Legacy Job ids do not carry an Effect identity.
            return Ok(());
        };
        let Some(intent) = self
            .db
            .effect_intent(workspace_id, effect_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        if intent.state.is_terminal() {
            return Ok(());
        }
        if matches!(job.state, DeviceJobState::Cancelled | DeviceJobState::CancelledUnknown)
            && matches!(intent.state, EffectState::Prepared | EffectState::Unknown)
        {
            // Nothing was physically sent, or the outcome is already owned by
            // reconciliation. Never invent a cancellation fact.
            return Ok(());
        }
        if job.state == DeviceJobState::Done && intent.state == EffectState::Cancelling {
            // A definitive device result after a stop request cannot complete
            // a cancelling Effect; record it as cancelled.
            return self
                .settle_terminal(workspace_id, &intent, EffectState::Cancelled, &job)
                .await;
        }
        let next = match job.state {
            DeviceJobState::Done => EffectState::Completed,
            DeviceJobState::Failed | DeviceJobState::Rejected | DeviceJobState::Interrupted => EffectState::Failed,
            DeviceJobState::Cancelled => EffectState::Cancelled,
            DeviceJobState::CancelledUnknown => EffectState::CancelledUnknown,
            _ => return Ok(()),
        };
        self.settle_terminal(workspace_id, &intent, next, &job).await
    }

    async fn settle_terminal(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        next: EffectState,
        job: &DeviceJob,
    ) -> Result<(), String> {
        let now = Utc::now();
        // Ensure the workspace projection exists before the transition updates
        // it (a failed dispatch may have removed it).
        self.db
            .upsert_workspace_job_projection(workspace_id, job)
            .await
            .map_err(|error| error.to_string())?;

        let mut current_state = intent.state;
        let mut current_version = intent.state_version;
        if current_state == EffectState::Dispatching
            && matches!(next, EffectState::Cancelled | EffectState::CancelledUnknown)
        {
            let cancel = reduce(
                &Self::harness_state(workspace_id, intent, EffectState::Dispatching, current_version),
                &HarnessFact {
                    transition_id: format!("settle:{}:cancelling", intent.effect_id),
                    effect_id: intent.effect_id.clone(),
                    next: EffectState::Cancelling,
                    occurred_at: now,
                    external_ref: Some(job.id.clone()),
                    result: None,
                    job: None,
                    wake: None,
                },
            )
            .map_err(|error| error.to_string())?;
            self.apply(workspace_id, cancel).await?;
            current_state = EffectState::Cancelling;
            current_version += 1;
        }

        let result = job.result_json.clone().unwrap_or_else(|| {
            json!({
                "jobId": job.id,
                "state": job.state.as_str(),
            })
        });
        // A definitive device completion after a stop request is recorded as
        // `cancelled`; keep the Job projection state aligned with the Effect
        // instead of leaving a `done` Job under a `cancelled` Effect.
        let projected_job_state = if next == EffectState::Cancelled && job.state == DeviceJobState::Done {
            DeviceJobState::Cancelled.as_str()
        } else {
            job.state.as_str()
        };
        let wake = WakeProjection {
            task_id: intent.task_id.clone(),
            wake_id: format!("settle:{}:wake", intent.effect_id),
            signal_json: json!({
                "effectId": intent.effect_id,
                "state": next.as_str(),
            })
            .to_string(),
            created_at: now,
        };
        let transition = reduce(
            &Self::harness_state(workspace_id, intent, current_state, current_version),
            &HarnessFact {
                transition_id: format!("settle:{}:{}", intent.effect_id, next.as_str()),
                effect_id: intent.effect_id.clone(),
                next,
                occurred_at: now,
                external_ref: Some(job.id.clone()),
                result: Some(result.clone()),
                job: Some(JobProjection {
                    job_id: job.id.clone(),
                    task_id: intent.task_id.clone(),
                    run_id: intent.run_id.clone(),
                    state: projected_job_state.to_string(),
                    result_json: Some(result),
                    external_ref: Some(job.id.clone()),
                    ended_at: job.ended_at.or(Some(now)),
                }),
                wake: Some(wake),
            },
        )
        .map_err(|error| error.to_string())?;
        self.apply(workspace_id, transition).await?;
        self.publish(workspace_id);
        Ok(())
    }

    /// Resolve the owning workspace from the deterministic Job id and settle.
    pub async fn settle_job_by_id(&self, job_id: &str) -> Result<(), String> {
        let Some(effect_id) = effect_id_from_job(job_id) else {
            return Ok(());
        };
        let workspaces = self
            .db
            .find_all_workspace_ids()
            .await
            .map_err(|error| error.to_string())?;
        for workspace_id in workspaces {
            match self.db.effect_intent(&workspace_id, effect_id).await {
                Ok(Some(_)) => return self.settle_job(&workspace_id, job_id).await,
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%workspace_id, %effect_id, %error, "effect lookup during job settle failed");
                }
            }
        }
        Ok(())
    }

    /// Bounded periodic recovery pass. The handle is aborted by the
    /// `ThingAgentManager` on drop, like the wake router.
    pub async fn run(self: Arc<Self>) {
        // Startup-only crash reconciliation. The periodic pass must not fence
        // live `dispatching` Effects.
        self.reconcile_startup().await;
        let mut interval = tokio::time::interval(DISPATCH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = self.notify.notified() => {}
            }
            let workspaces = match self.db.find_all_workspace_ids().await {
                Ok(workspaces) => workspaces,
                Err(error) => {
                    tracing::warn!(%error, "effect dispatch workspace listing failed");
                    continue;
                }
            };
            for workspace_id in workspaces {
                if let Err(error) = self.dispatch_workspace(&workspace_id).await {
                    tracing::warn!(%workspace_id, %error, "effect dispatch pass failed");
                }
            }
        }
    }
}

fn same_job_identity(job: &DeviceJob, workspace_id: &str, intent: &EffectIntent) -> bool {
    job.workspace_id == workspace_id
        && job.task_id.as_deref() == Some(intent.task_id.as_str())
        && job.run_id.as_deref() == Some(intent.run_id.as_str())
        && job.tool_call_id.as_deref() == Some(intent.tool_call_id.as_str())
        && job.tool == intent.tool_name
}

/// `JobResultSink` wrapper. It first lets the existing `JobService` own the
/// terminal Job fact, then settles the matching Effect through the Harness
/// transition applier.
#[derive(Clone)]
pub struct HarnessJobResultSink {
    jobs: JobService,
    dispatch: Arc<EffectDispatch>,
}

impl HarnessJobResultSink {
    pub fn new(jobs: JobService, dispatch: Arc<EffectDispatch>) -> Self {
        Self { jobs, dispatch }
    }
}

#[async_trait]
impl JobResultSink for HarnessJobResultSink {
    async fn command_finished(&self, driver_call_id: &str, success: bool, error: Option<&str>) {
        JobResultSink::command_finished(&self.jobs, driver_call_id, success, error).await;
        if let Err(error) = self.dispatch.settle_job_by_id(driver_call_id).await {
            tracing::warn!(job_id = driver_call_id, %error, "effect settle after job completion failed");
        }
    }

    async fn command_cancelled(&self, job_id: &str, within_grace: bool) {
        JobResultSink::command_cancelled(&self.jobs, job_id, within_grace).await;
        if let Err(error) = self.dispatch.settle_job_by_id(job_id).await {
            tracing::warn!(job_id, %error, "effect settle after job cancellation failed");
        }
    }
}

const _: fn() = || {
    fn assert_sender<T: EffectSender>() {}
    assert_sender::<JobEffectSender>();
};

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::{Value, json};
    use tinyiothub_core::agent_task::TaskWake;
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
    use tinyiothub_core::error::Error;
    use tinyiothub_core::models::thing_command::ThingCommand;
    use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
    use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
    use tinyiothub_runtime::arbitration::ArbitrationRegistry;
    use tinyiothub_runtime::job::service::{JobFinal, JobService};
    use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobResultSink, JobWorkerLocator};
    use tinyiothub_runtime::ports::DeviceJobStore;
    use tinyiothub_storage::Db;
    use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};

    use super::{EffectDispatch, HarnessJobResultSink, job_id_for};

    const WS: &str = "ws-dispatch";

    #[derive(Default)]
    struct RecordingWorker {
        submitted: Mutex<Vec<String>>,
    }

    impl JobDeviceWorker for RecordingWorker {
        fn submit(
            &self,
            _command: ThingCommand,
            job_id: &str,
            _lease: Option<tinyiothub_runtime::arbitration::ResourceLease>,
        ) -> Result<(), Error> {
            self.submitted.lock().expect("submissions").push(job_id.into());
            Ok(())
        }

        fn cancel_job(&self, _job_id: &str) -> bool {
            true
        }

        fn has_recoverable_call(&self, _job_id: &str) -> bool {
            false
        }

        fn complete_job(&self, _job_id: &str) {}
    }

    struct RecordingWorkers {
        worker: Arc<RecordingWorker>,
    }

    impl JobWorkerLocator for RecordingWorkers {
        fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
            Some(self.worker.clone())
        }
    }

    /// Store wrapper that fails the Job insert, exercising the submit
    /// `Storage` path after the dispatching permit is committed.
    struct FailCreateStore(Arc<dyn DeviceJobStore>);

    #[async_trait]
    impl DeviceJobStore for FailCreateStore {
        async fn create_job(&self, _job: &DeviceJob) -> Result<(), DeviceJobError> {
            Err(DeviceJobError::Storage("injected create failure".into()))
        }

        async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            self.0.get_job(workspace_id, job_id).await
        }

        async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
            self.0.get_job_by_id(job_id).await
        }

        async fn get_task_job(
            &self,
            workspace_id: &str,
            task_id: &str,
            job_id: &str,
        ) -> Result<Option<DeviceJob>, DeviceJobError> {
            self.0.get_task_job(workspace_id, task_id, job_id).await
        }

        async fn get_job_by_tool_call(
            &self,
            workspace_id: &str,
            tool_call_id: &str,
        ) -> Result<Option<DeviceJob>, DeviceJobError> {
            self.0.get_job_by_tool_call(workspace_id, tool_call_id).await
        }

        async fn submit_approved_job(&self, job: &DeviceJob) -> Result<(Option<DeviceJob>, bool), DeviceJobError> {
            self.0.submit_approved_job(job).await
        }

        async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
            self.0.mark_running(job_id, driver_call_id).await
        }

        async fn finish_job(
            &self,
            job_id: &str,
            state: DeviceJobState,
            result: &Value,
            wake: Option<&TaskWake>,
        ) -> Result<bool, DeviceJobError> {
            self.0.finish_job(job_id, state, result, wake).await
        }

        async fn update_progress(&self, job_id: &str, progress: &Value) -> Result<bool, DeviceJobError> {
            self.0.update_progress(job_id, progress).await
        }

        async fn request_stop(
            &self,
            workspace_id: &str,
            task_id: &str,
            job_id: &str,
        ) -> Result<DeviceJob, DeviceJobError> {
            self.0.request_stop(workspace_id, task_id, job_id).await
        }

        async fn active_jobs(
            &self,
            workspace_id: Option<&str>,
            unknown_only: bool,
        ) -> Result<Vec<DeviceJob>, DeviceJobError> {
            self.0.active_jobs(workspace_id, unknown_only).await
        }
    }

    async fn ad_hoc_dispatch(
        workspace: &WorkspaceFixture,
        store: Arc<dyn DeviceJobStore>,
    ) -> (Arc<EffectDispatch>, Arc<RecordingWorker>) {
        let db = Arc::new(workspace.db.clone());
        let worker = Arc::new(RecordingWorker::default());
        let jobs = JobService::new(
            store,
            Arc::new(RecordingWorkers { worker: worker.clone() }),
            Arc::new(Validator::default()),
            Arc::new(Resources),
            Arc::new(ArbitrationRegistry::default()),
        );
        (Arc::new(EffectDispatch::new(db, jobs)), worker)
    }

    #[derive(Default)]
    struct Validator {
        /// Test hook: request cancellation while submit is between the permit
        /// gate and the worker call.
        cancel_on_validate: Option<(Arc<Mutex<Option<Arc<Db>>>>, String)>,
    }

    #[async_trait]
    impl AccessCallValidator for Validator {
        async fn validate(
            &self,
            _workspace_id: &str,
            thing_id: &str,
            tool: &str,
            _params: Option<&Value>,
        ) -> Result<ValidatedCall, CallError> {
            if thing_id == "missing" {
                return Err(CallError::NoDeclaration(thing_id.to_string()));
            }
            if let Some((slot, effect_id)) = &self.cancel_on_validate {
                let db = { slot.lock().expect("validator db").clone() };
                if let Some(db) = db {
                    super::super::EffectRecovery::new(db)
                        .request_cancellation(WS, effect_id)
                        .await
                        .expect("cancel during validate");
                }
            }
            Ok(ValidatedCall {
                tool: tinyiothub_core::driver::manifest::ManifestTool {
                    name: tool.to_string(),
                    kind: tinyiothub_core::driver::manifest::ToolKindDecl::Action,
                    target_field: None,
                    parameters: json!({"type":"object"}),
                    timeout: tinyiothub_core::driver::manifest::ToolTimeout {
                        default_ms: 5_000,
                        max_ms: 5_000,
                    },
                    safety: tinyiothub_core::driver::manifest::ToolSafetyDecl::Control,
                    resources: vec![],
                    cancellable: true,
                },
                timeout_ms: 5_000,
                clamped: false,
            })
        }
    }

    struct Resources;

    #[async_trait]
    impl ResourceResolver for Resources {
        async fn resolve_action(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _tool_name: &str,
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }

        async fn resolve_properties(
            &self,
            _workspace_id: &str,
            _thing_id: &str,
            _properties: &[String],
        ) -> Result<Vec<ResourceClaim>, String> {
            Ok(vec![device_claim()])
        }
    }

    struct Fixture {
        workspace: WorkspaceFixture,
        db: Arc<Db>,
        jobs: JobService,
        worker: Arc<RecordingWorker>,
        dispatch: Arc<EffectDispatch>,
    }

    async fn seed_effect_in(workspace: &WorkspaceFixture, effect_id: &str, task_state: &str, request_json: Value) {
        let pool = workspace.workspace_pool().await;
        sqlx::query(
            "INSERT INTO agent_tasks (id, run_id, state, state_version, execution_mode)
             VALUES (?, ?, ?, 1, 'production')",
        )
        .bind(effect_id)
        .bind(format!("run-{effect_id}"))
        .bind(task_state)
        .execute(&pool)
        .await
        .expect("task");
        sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
            .bind(format!("run-{effect_id}"))
            .bind(effect_id)
            .execute(&pool)
            .await
            .expect("run");
        sqlx::query(
            "INSERT INTO effect_intents
                (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
                 request_digest, idempotency_key, external_ref, state, state_version, result_json,
                 created_at, updated_at)
             VALUES (?, 'session-1', ?, ?, ?, 'set_level', ?, ?, ?, NULL, 'prepared', 1, NULL, ?, ?)",
        )
        .bind(effect_id)
        .bind(effect_id)
        .bind(format!("run-{effect_id}"))
        .bind(format!("call-{effect_id}"))
        .bind(request_json.to_string())
        .bind(format!("digest-{effect_id}"))
        .bind(effect_id)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("effect");
    }

    impl Fixture {
        async fn new() -> Self {
            Self::new_with(Arc::new(Validator::default())).await
        }

        async fn new_with(validator: Arc<dyn AccessCallValidator>) -> Self {
            let workspace = fixture_workspace(WS).await;
            let db = Arc::new(workspace.db.clone());
            let worker = Arc::new(RecordingWorker::default());
            let jobs = JobService::new(
                Arc::new(crate::shared::runtime_ports::HarnessAwareDeviceJobStore::new(
                    db.clone(),
                    crate::shared::runtime_ports::DbDeviceJobStore::new(workspace.db.clone()),
                )),
                Arc::new(RecordingWorkers { worker: worker.clone() }),
                validator,
                Arc::new(Resources),
                Arc::new(ArbitrationRegistry::default()),
            );
            let dispatch = Arc::new(EffectDispatch::new(db.clone(), jobs.clone()));
            Self {
                workspace,
                db,
                jobs,
                worker,
                dispatch,
            }
        }

        fn sink(&self) -> HarnessJobResultSink {
            HarnessJobResultSink::new(self.jobs.clone(), self.dispatch.clone())
        }

        async fn seed(&self, effect_id: &str, task_state: &str, request_json: Value) {
            seed_effect_in(&self.workspace, effect_id, task_state, request_json).await;
        }

        async fn seed_decision(&self, effect_id: &str, state: &str, answer: Option<&str>) {
            let pool = self.workspace.workspace_pool().await;
            sqlx::query(
                "INSERT INTO decisions
                    (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
                     options_json, created_at, updated_at, expires_at, answer, answer_actor,
                     answer_event_id, consumed_at, consumed_by)
                 VALUES (?, 'tool_approval', ?, 'session-1', ?, ?, ?, '{}', '[\"approved\",\"denied\"]',
                         ?, ?, NULL, ?, NULL, NULL, NULL, NULL)",
            )
            .bind(format!("approval-{effect_id}"))
            .bind(state)
            .bind(format!("run-{effect_id}"))
            .bind(effect_id)
            .bind(effect_id)
            .bind(Utc::now().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(answer)
            .execute(&pool)
            .await
            .expect("decision");
        }

        async fn effect(&self, effect_id: &str) -> (String, i64, Option<String>, Option<String>) {
            sqlx::query_as(
                "SELECT state, state_version, external_ref, result_json FROM effect_intents WHERE effect_id = ?",
            )
            .bind(effect_id)
            .fetch_one(&self.workspace.workspace_pool().await)
            .await
            .expect("effect state")
        }

        async fn workspace_jobs(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs")
                .fetch_one(&self.workspace.workspace_pool().await)
                .await
                .expect("workspace jobs")
        }

        async fn platform_jobs(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs")
                .fetch_one(self.db.pool())
                .await
                .expect("platform jobs")
        }

        async fn platform_approvals(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
                .fetch_one(self.db.pool())
                .await
                .expect("platform approvals")
        }

        async fn platform_runtime_events(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM hub_ledger WHERE kind = 'runtime.v1'")
                .fetch_one(self.db.pool())
                .await
                .expect("platform runtime events")
        }
    }

    #[tokio::test]
    async fn prepared_effect_dispatches_once_and_second_pass_is_a_no_op() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-1",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;

        let first = fixture.dispatch.dispatch_workspace(WS).await.expect("first pass");
        assert_eq!(first.dispatched, 1);
        let expected_job = job_id_for("effect-1");
        assert_eq!(
            fixture.worker.submitted.lock().expect("submissions").as_slice(),
            std::slice::from_ref(&expected_job)
        );
        let (state, _, external_ref, _) = fixture.effect("effect-1").await;
        assert_eq!(state, "dispatching");
        assert_eq!(external_ref.as_deref(), Some(expected_job.as_str()));
        assert_eq!(fixture.workspace_jobs().await, 1);

        let second = fixture.dispatch.dispatch_workspace(WS).await.expect("second pass");
        assert_eq!(second.dispatched, 0);
        assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
        assert_eq!(fixture.platform_approvals().await, 0);
        assert_eq!(fixture.platform_runtime_events().await, 0);
        assert_eq!(
            fixture.platform_jobs().await,
            0,
            "Effect Jobs live in the workspace store"
        );
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn requested_approval_blocks_until_answered_then_dispatches_or_denies() {
        for answer in [Some("approved"), Some("denied")] {
            let fixture = Fixture::new().await;
            fixture
                .seed(
                    "effect-approval",
                    "running",
                    json!({"deviceId":"device-1","argsJson":{"level":7}}),
                )
                .await;
            fixture.seed_decision("effect-approval", "requested", None).await;
            let blocked = fixture.dispatch.dispatch_workspace(WS).await.expect("blocked pass");
            assert_eq!(blocked.blocked, 1);
            assert_eq!(blocked.dispatched, 0);
            assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());

            let pool = fixture.workspace.workspace_pool().await;
            sqlx::query(
                "UPDATE decisions SET state = 'answered', answer = ?, updated_at = ?
                 WHERE effect_id = 'effect-approval' AND state = 'requested'",
            )
            .bind(answer.expect("answer"))
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .expect("answer decision");

            let pass = fixture.dispatch.dispatch_workspace(WS).await.expect("answered pass");
            let (state, _, _, _) = fixture.effect("effect-approval").await;
            if answer == Some("approved") {
                assert_eq!(pass.dispatched, 1);
                assert_eq!(state, "dispatching");
                assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
            } else {
                assert_eq!(pass.not_applied, 1);
                assert_eq!(state, "not_applied");
                assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
                assert_eq!(fixture.workspace_jobs().await, 0);
            }
            let decision: (String, Option<String>) =
                sqlx::query_as("SELECT state, answer FROM decisions WHERE effect_id = 'effect-approval'")
                    .fetch_one(&fixture.workspace.workspace_pool().await)
                    .await
                    .expect("decision");
            assert_eq!(decision.0, "consumed", "{answer:?}");
            assert_eq!(decision.1.as_deref(), answer);
            fixture.workspace.close().await;
        }
    }

    #[tokio::test]
    async fn job_completion_and_failure_settle_the_effect_with_the_job_result() {
        for (success, expected, expected_result) in [
            (true, "completed", json!({"ok": true})),
            (false, "failed", json!({"error": "device failed"})),
        ] {
            let fixture = Fixture::new().await;
            fixture
                .seed(
                    "effect-outcome",
                    "running",
                    json!({"deviceId":"device-1","argsJson":{"level":7}}),
                )
                .await;
            fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
            let job_id = job_id_for("effect-outcome");
            let sink = fixture.sink();
            JobResultSink::command_finished(
                &sink,
                &job_id,
                success,
                if success { None } else { Some("device failed") },
            )
            .await;

            let (state, _, _, result) = fixture.effect("effect-outcome").await;
            assert_eq!(state, expected, "{result:?}");
            let result: Value = serde_json::from_str(result.as_deref().expect("result")).expect("result json");
            assert_eq!(result, expected_result);
            assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
            fixture.workspace.close().await;
        }
    }

    #[tokio::test]
    async fn cancel_during_dispatching_requests_stop_and_settles_cancelled_unknown() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-cancel",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        let job_id = job_id_for("effect-cancel");

        let recovery = super::super::EffectRecovery::new(fixture.db.clone());
        recovery
            .request_cancellation(WS, "effect-cancel")
            .await
            .expect("request cancellation");
        fixture.jobs.stop(WS, "effect-cancel", &job_id).await.expect("stop job");
        let stopped = fixture.jobs.job(WS, &job_id).await.expect("job read").expect("job");
        assert!(stopped.stop_requested_at.is_some());

        let pass = fixture.dispatch.dispatch_workspace(WS).await.expect("post-cancel pass");
        assert_eq!(pass.dispatched, 0);
        assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);

        let sink = fixture.sink();
        JobResultSink::command_cancelled(&sink, &job_id, false).await;
        assert_eq!(fixture.effect("effect-cancel").await.0, "cancelled_unknown");
        assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn dispatch_validation_failure_is_not_applied_without_a_job_row() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-invalid",
                "running",
                json!({"deviceId":"missing","argsJson":{"level":7}}),
            )
            .await;
        let report = fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        assert_eq!(report.not_applied, 1);
        assert_eq!(fixture.effect("effect-invalid").await.0, "not_applied");
        assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
        assert_eq!(fixture.workspace_jobs().await, 0);
        assert_eq!(fixture.platform_jobs().await, 0);
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn prepared_effect_of_a_terminal_task_is_settled_without_dispatch() {
        for task_state in ["completed", "failed", "cancelled"] {
            let fixture = Fixture::new().await;
            fixture
                .seed(
                    "effect-stale",
                    task_state,
                    json!({"deviceId":"device-1","argsJson":{"level":7}}),
                )
                .await;
            fixture.seed_decision("effect-stale", "requested", None).await;
            let report = fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
            assert_eq!(report.not_applied, 1, "{task_state}");
            let (state, _, external_ref, result) = fixture.effect("effect-stale").await;
            assert_eq!(state, "not_applied", "{task_state}");
            assert!(external_ref.is_none());
            assert!(
                result
                    .as_deref()
                    .is_some_and(|value| value.contains("owning_task_terminal")),
                "{result:?}"
            );
            assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
            assert_eq!(fixture.workspace_jobs().await, 0);
            let decision: String = sqlx::query_scalar("SELECT state FROM decisions WHERE effect_id = 'effect-stale'")
                .fetch_one(&fixture.workspace.workspace_pool().await)
                .await
                .expect("decision");
            assert_eq!(decision, "withdrawn", "{task_state}");
            fixture.workspace.close().await;
        }
    }

    #[tokio::test]
    async fn paused_task_blocks_dispatch() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-paused",
                "paused",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        let report = fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        assert_eq!(report.blocked, 1);
        assert_eq!(fixture.effect("effect-paused").await.0, "prepared");
        assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
        assert_eq!(fixture.workspace_jobs().await, 0);
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn missing_device_or_args_fails_closed_to_not_applied() {
        for request in [json!({"argsJson": {"level": 7}}), json!({"deviceId": "device-1"})] {
            let fixture = Fixture::new().await;
            fixture.seed("effect-shape", "running", request).await;
            let report = fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
            assert_eq!(report.not_applied, 1);
            assert_eq!(fixture.effect("effect-shape").await.0, "not_applied");
            assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
            fixture.workspace.close().await;
        }
    }

    #[tokio::test]
    async fn sink_settles_with_a_custom_job_result() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-result",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        let job_id = job_id_for("effect-result");
        fixture
            .jobs
            .finish(&job_id, JobFinal::Done(json!({"level": 7, "confirmed": true})))
            .await
            .expect("finish");
        fixture.dispatch.settle_job(WS, &job_id).await.expect("settle");
        let (state, _, _, result) = fixture.effect("effect-result").await;
        assert_eq!(state, "completed");
        let result: Value = serde_json::from_str(result.as_deref().expect("result")).expect("result json");
        assert_eq!(result, json!({"level": 7, "confirmed": true}));
        let job_state: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&fixture.workspace.workspace_pool().await)
            .await
            .expect("workspace job");
        assert_eq!(job_state, "done");
        let _ = Duration::from_millis(1);
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn concurrent_dispatch_passes_send_exactly_once() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-race-pass",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        let dispatch = fixture.dispatch.clone();
        let (left, right) = tokio::join!(dispatch.dispatch_workspace(WS), dispatch.dispatch_workspace(WS));
        let dispatched = left.expect("left pass").dispatched + right.expect("right pass").dispatched;
        assert_eq!(dispatched, 1, "the permit CAS must admit one send");
        assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
        assert_eq!(fixture.workspace_jobs().await, 1);
        assert_eq!(fixture.effect("effect-race-pass").await.0, "dispatching");
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn retried_submit_reuses_the_job_and_does_not_recall_the_worker() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-retry",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        let intent = fixture
            .db
            .effect_intent(WS, "effect-retry")
            .await
            .expect("effect read")
            .expect("effect");
        let sender = fixture.dispatch.sender();
        let first = sender.submit(WS, &intent).await.expect("first submit");
        assert_eq!(first.id, job_id_for("effect-retry"));
        let second = sender
            .submit(WS, &intent)
            .await
            .expect_err("repeat submit must not send");
        assert!(
            matches!(second, DeviceJobError::Conflict(_)),
            "store-level dedupe must return Conflict: {second:?}"
        );
        assert_eq!(fixture.worker.submitted.lock().expect("submissions").len(), 1);
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn cancel_between_gate_and_submit_stops_the_created_job() {
        let slot: Arc<Mutex<Option<Arc<Db>>>> = Arc::new(Mutex::new(None));
        let fixture = Fixture::new_with(Arc::new(Validator {
            cancel_on_validate: Some((slot.clone(), "effect-race".into())),
        }))
        .await;
        *slot.lock().expect("slot") = Some(fixture.db.clone());
        fixture
            .seed(
                "effect-race",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;

        let report = fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        assert_eq!(report.dispatched, 1);
        let job_id = job_id_for("effect-race");
        let job = fixture
            .jobs
            .job(WS, &job_id)
            .await
            .expect("job read")
            .expect("created job");
        assert!(
            job.stop_requested_at.is_some(),
            "a cancel that wins the gate-vs-submit race must stop the new Job"
        );
        assert_eq!(fixture.effect("effect-race").await.0, "cancelling");
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn submit_storage_failure_settles_not_applied_instead_of_stranding_dispatching() {
        let workspace = fixture_workspace(WS).await;
        seed_effect_in(
            &workspace,
            "effect-storage",
            "running",
            json!({"deviceId":"device-1","argsJson":{"level":7}}),
        )
        .await;
        let db = Arc::new(workspace.db.clone());
        let inner = Arc::new(crate::shared::runtime_ports::HarnessAwareDeviceJobStore::new(
            db.clone(),
            crate::shared::runtime_ports::DbDeviceJobStore::new(workspace.db.clone()),
        ));
        let (dispatch, worker) = ad_hoc_dispatch(&workspace, Arc::new(FailCreateStore(inner))).await;

        let report = dispatch.dispatch_workspace(WS).await.expect("dispatch");
        assert_eq!(report.not_applied, 1);
        let (state, _, external_ref, result): (String, i64, Option<String>, Option<String>) =
            sqlx::query_as("SELECT state, state_version, external_ref, result_json FROM effect_intents WHERE effect_id = 'effect-storage'")
                .fetch_one(&workspace.workspace_pool().await)
                .await
                .expect("effect");
        assert_eq!(state, "not_applied");
        assert!(external_ref.is_none());
        assert!(
            result
                .as_deref()
                .is_some_and(|value| value.contains("dispatch_storage")),
            "{result:?}"
        );
        assert!(worker.submitted.lock().expect("submissions").is_empty());
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs")
            .fetch_one(&workspace.workspace_pool().await)
            .await
            .expect("job count");
        assert_eq!(jobs, 0);
        workspace.close().await;
    }

    #[tokio::test]
    async fn startup_reconciliation_settles_dispatching_without_a_job() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-crash",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        let pool = fixture.workspace.workspace_pool().await;
        sqlx::query(
            "UPDATE effect_intents SET state = 'dispatching', state_version = 2, external_ref = ?
             WHERE effect_id = 'effect-crash'",
        )
        .bind(job_id_for("effect-crash"))
        .execute(&pool)
        .await
        .expect("crash state");

        fixture.dispatch.reconcile_startup().await;
        let (state, _, external_ref, result) = fixture.effect("effect-crash").await;
        assert_eq!(state, "not_applied");
        assert!(external_ref.is_none());
        assert!(
            result
                .as_deref()
                .is_some_and(|value| value.contains("dispatch_crash_no_job")),
            "{result:?}"
        );
        assert!(fixture.worker.submitted.lock().expect("submissions").is_empty());
        fixture.workspace.close().await;
    }

    #[tokio::test]
    async fn done_after_stop_projects_the_job_as_cancelled() {
        let fixture = Fixture::new().await;
        fixture
            .seed(
                "effect-done-stop",
                "running",
                json!({"deviceId":"device-1","argsJson":{"level":7}}),
            )
            .await;
        fixture.dispatch.dispatch_workspace(WS).await.expect("dispatch");
        let job_id = job_id_for("effect-done-stop");
        super::super::EffectRecovery::new(fixture.db.clone())
            .request_cancellation(WS, "effect-done-stop")
            .await
            .expect("request cancellation");
        fixture
            .jobs
            .finish(&job_id, JobFinal::Done(json!({"level": 7})))
            .await
            .expect("done after stop");
        fixture.dispatch.settle_job(WS, &job_id).await.expect("settle");

        assert_eq!(fixture.effect("effect-done-stop").await.0, "cancelled");
        let job_state: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&fixture.workspace.workspace_pool().await)
            .await
            .expect("workspace job");
        assert_eq!(
            job_state, "cancelled",
            "the Job projection must agree with the cancelled Effect"
        );
        fixture.workspace.close().await;
    }
}
