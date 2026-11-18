use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tinyiothub_agent::harness::{EffectReconciler, ReconcileOutcome, RecoveryFact, reduce_recovery};
use tinyiothub_core::harness::{DecisionKind, DecisionState, EffectIntent, EffectState, Transition};
use tinyiothub_storage::Db;
use tinyiothub_storage::harness_effect::EffectRecoveryItem;

/// Fill the owning workspace into a wake signal produced by a reducer that has
/// no workspace argument, and default the kind so `TaskCoordinator::advance`
/// can parse it. Idempotent: signals that already carry these fields keep
/// their values.
pub(crate) fn scope_transition_wake(transition: &mut Transition, workspace_id: &str) {
    let Some(wake) = transition.wake.as_mut() else {
        return;
    };
    let Ok(mut signal) = serde_json::from_str::<serde_json::Value>(&wake.signal_json) else {
        return;
    };
    let Some(object) = signal.as_object_mut() else {
        return;
    };
    object.insert(
        "workspace_id".to_string(),
        serde_json::Value::String(workspace_id.to_string()),
    );
    object
        .entry("kind")
        .or_insert_with(|| serde_json::Value::String("effect_terminal".to_string()));
    wake.signal_json = signal.to_string();
}

pub mod admin;
#[cfg(test)]
mod api_cutover_tests;
pub mod approvals;
pub mod controls;
pub mod dispatch;
pub mod harness_store;
pub mod projection;
pub mod session_stream;
pub mod stream;
pub mod tasks;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryMode {
    Restart,
    Restore,
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub dispatch_permits: Vec<EffectIntent>,
    pub blocked: usize,
}

#[async_trait]
pub trait EffectSender: Send + Sync {
    async fn send(&self, intent: &EffectIntent) -> Result<(), String>;
}

#[derive(Clone)]
pub struct EffectDispatcher {
    db: Arc<Db>,
}

impl EffectDispatcher {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn dispatch(
        &self,
        workspace_id: &str,
        intent: &EffectIntent,
        sender: &dyn EffectSender,
    ) -> Result<bool, String> {
        let controls = controls::WorkflowControls::new(self.db.clone(), workspace_id);
        if !controls
            .dispatch_permitted(&intent.effect_id)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        sender.send(intent).await?;
        Ok(true)
    }
}

#[derive(Clone)]
pub struct EffectRecovery {
    db: Arc<Db>,
}

impl EffectRecovery {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    async fn item(&self, workspace_id: &str, effect_id: &str) -> Result<EffectRecoveryItem, String> {
        self.db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.intent.effect_id == effect_id)
            .ok_or_else(|| format!("active effect {effect_id} not found"))
    }

    async fn apply(&self, workspace_id: &str, mut transition: Transition) -> Result<(), String> {
        if let Some(event) = &mut transition.event {
            event.workspace_id = workspace_id.to_string();
        }
        for event in &mut transition.additional_events {
            event.workspace_id = workspace_id.to_string();
        }
        scope_transition_wake(&mut transition, workspace_id);
        let precondition = transition.precondition.clone();
        self.db
            .apply_workspace_transition(workspace_id, &precondition, &transition)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn transition(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        fact: RecoveryFact,
    ) -> Result<(), String> {
        let transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            item.job_id.as_deref(),
            &format!("recovery:{}", uuid::Uuid::new_v4()),
            Utc::now(),
            &item.task_state,
            fact,
        )
        .map_err(|error| error.to_string())?;
        self.apply(workspace_id, transition).await
    }

    /// Consume an answered reconciliation without rewriting a paused Task's
    /// pause relation. The Effect still settles; the Task waits for the
    /// operator's explicit resume.
    async fn consume_answer(&self, workspace_id: &str, item: &EffectRecoveryItem) -> Result<(), String> {
        let mut transition = reduce_recovery(
            &item.intent,
            item.decision.as_ref(),
            item.job_id.as_deref(),
            &format!("recovery:{}", uuid::Uuid::new_v4()),
            Utc::now(),
            &item.task_state,
            RecoveryFact::ConsumeAnswer,
        )
        .map_err(|error| error.to_string())?;
        if item.task_state == "paused" {
            transition.run_task = None;
        }
        self.apply(workspace_id, transition).await
    }

    async fn recover_unknown(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        reconciler: &dyn EffectReconciler,
        report: &mut RecoveryReport,
    ) -> Result<(), String> {
        match item.decision.as_ref() {
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation && decision.state == DecisionState::Answered =>
            {
                if item.task_active {
                    self.transition(workspace_id, item, RecoveryFact::ConsumeAnswer).await?;
                } else {
                    report.blocked += 1;
                }
                return Ok(());
            }
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation
                    && decision.state == DecisionState::Requested => {}
            None => {
                self.transition(workspace_id, item, RecoveryFact::EnsureReconciliation)
                    .await?;
                let item = self.item(workspace_id, &item.intent.effect_id).await?;
                return Box::pin(self.recover_unknown(workspace_id, &item, reconciler, report)).await;
            }
            _ => return Err("unknown effect has an invalid reconciliation decision".to_string()),
        }
        match reconciler.settle(&item.intent).await {
            ReconcileOutcome::Unknown { reason } if item.intent.recovery_from == Some(EffectState::Cancelling) => {
                self.transition(
                    workspace_id,
                    item,
                    RecoveryFact::Reconcile(ReconcileOutcome::Unknown { reason }),
                )
                .await
            }
            ReconcileOutcome::Unknown { reason } => {
                self.db
                    .record_effect_reconcile_attempt(workspace_id, &item.intent, &reason)
                    .await
                    .map_err(|error| error.to_string())?;
                report.blocked += 1;
                Ok(())
            }
            outcome => {
                self.transition(workspace_id, item, RecoveryFact::Reconcile(outcome))
                    .await
            }
        }
    }

    async fn recover_cancelling(
        &self,
        workspace_id: &str,
        item: &EffectRecoveryItem,
        reconciler: &dyn EffectReconciler,
    ) -> Result<(), String> {
        match reconciler.settle(&item.intent).await {
            ReconcileOutcome::Unknown { .. } => {
                self.transition(
                    workspace_id,
                    item,
                    RecoveryFact::Reconcile(ReconcileOutcome::Unknown {
                        reason: "stop result unknown".to_string(),
                    }),
                )
                .await
            }
            outcome => {
                self.transition(workspace_id, item, RecoveryFact::Reconcile(outcome))
                    .await
            }
        }
    }

    pub async fn recover_workspace(
        &self,
        workspace_id: &str,
        mode: RecoveryMode,
        reconciler: &dyn EffectReconciler,
    ) -> Result<RecoveryReport, String> {
        // Fail closed on any persisted row outside the listed recovery
        // matrix before a single dispatch permit or decision consume occurs.
        self.db
            .guard_recovery_matrix(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let items = self
            .db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let mut report = RecoveryReport::default();
        if mode == RecoveryMode::Restore {
            for item in &items {
                let fact = if item.task_state == "cancelling"
                    && matches!(item.intent.state, EffectState::Prepared | EffectState::Dispatching)
                {
                    RecoveryFact::Cancel
                } else {
                    RecoveryFact::Fence
                };
                self.transition(workspace_id, item, fact).await?;
            }
            return Box::pin(self.recover_workspace(workspace_id, RecoveryMode::Restart, reconciler)).await;
        }
        for item in items {
            match item.intent.state {
                EffectState::Prepared => {
                    if item.task_state == "cancelling" {
                        self.transition(workspace_id, &item, RecoveryFact::Cancel).await?;
                    } else if item.task_active
                        && item.decision.as_ref().is_some_and(|decision| {
                            decision.kind == DecisionKind::ToolApproval
                                && matches!(
                                    decision.state,
                                    DecisionState::Answered | DecisionState::Expired | DecisionState::Withdrawn
                                )
                        })
                    {
                        let approved = item
                            .decision
                            .as_ref()
                            .is_some_and(|decision| decision.answer.as_deref() == Some("approved"));
                        self.transition(workspace_id, &item, RecoveryFact::ConsumeApproval)
                            .await?;
                        if approved {
                            let dispatched = self.item(workspace_id, &item.intent.effect_id).await?;
                            report.dispatch_permits.push(dispatched.intent);
                        }
                    }
                }
                EffectState::Dispatching if item.task_state == "cancelling" => {
                    self.transition(workspace_id, &item, RecoveryFact::Cancel).await?;
                    let cancelling = self.item(workspace_id, &item.intent.effect_id).await?;
                    self.recover_cancelling(workspace_id, &cancelling, reconciler).await?;
                }
                EffectState::Dispatching => {
                    self.transition(workspace_id, &item, RecoveryFact::Fence).await?;
                    let fenced = self.item(workspace_id, &item.intent.effect_id).await?;
                    self.recover_unknown(workspace_id, &fenced, reconciler, &mut report)
                        .await?;
                }
                EffectState::Unknown => {
                    self.recover_unknown(workspace_id, &item, reconciler, &mut report)
                        .await?;
                }
                EffectState::Cancelling => {
                    self.recover_cancelling(workspace_id, &item, reconciler).await?;
                }
                EffectState::Completed
                | EffectState::Failed
                | EffectState::NotApplied
                | EffectState::Cancelled
                | EffectState::CancelledUnknown => {}
            }
        }
        Ok(report)
    }

    pub async fn answer_reconciliation(
        &self,
        workspace_id: &str,
        effect_id: &str,
        answer: &str,
        actor: &str,
        evidence: Option<serde_json::Value>,
    ) -> Result<(), String> {
        // Commit the durable answer (and its continuation wake). Applying it to
        // the Effect is the recovery pass's job: `ensure_reconciliation` consumes
        // an answered Decision when the Task can advance, and leaves a paused
        // Task waiting for an explicit resume. This keeps the answer atomic and
        // keeps every consumption on one idempotent, audited path.
        self.db
            .answer_effect_reconciliation(workspace_id, effect_id, answer, actor, evidence)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Production recovery for one persisted `unknown` Effect.
    ///
    /// - no reconciliation Decision: create the idempotent Decision and keep
    ///   the Run blocked (restore fencing semantics live in the reducer);
    /// - answered Decision and the Task can still advance: consume it so the
    ///   Effect reaches `completed`/`not_applied` and the continuation wake can
    ///   run;
    /// - requested Decision, or an answered one on a paused Task: leave it for
    ///   the operator. Nothing here ever dispatches a physical action.
    pub async fn ensure_reconciliation(&self, workspace_id: &str, effect_id: &str) -> Result<(), String> {
        // Surface a workspace read failure to the caller (startup/periodic
        // sweeps log it); a missing/terminal Effect is a normal no-op.
        let items = self
            .db
            .list_effect_recovery(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let Some(item) = items.into_iter().find(|item| item.intent.effect_id == effect_id) else {
            return Ok(());
        };
        if item.intent.state != EffectState::Unknown {
            return Ok(());
        }
        match item.decision.as_ref() {
            None => {
                self.transition(workspace_id, &item, RecoveryFact::EnsureReconciliation)
                    .await
            }
            // The operator answered: apply it even while the Task is paused.
            // The Effect must leave `unknown` so the attention item clears and
            // the resume gate can lift; a paused Task keeps its original pause
            // relation and waits for the explicit resume.
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation && decision.state == DecisionState::Answered =>
            {
                self.consume_answer(workspace_id, &item).await
            }
            Some(decision)
                if decision.kind == DecisionKind::EffectReconciliation
                    && decision.state == DecisionState::Requested =>
            {
                // A non-paused Task with a persisted requested Decision needs
                // the relation-backed pause; re-running EnsureReconciliation on
                // an already paused Task would only churn the version.
                if item.task_state != "paused"
                    && !matches!(item.task_state.as_str(), "completed" | "failed" | "cancelled")
                {
                    self.transition(workspace_id, &item, RecoveryFact::EnsureReconciliation)
                        .await
                } else {
                    Ok(())
                }
            }
            // An unknown Effect with only an answered/consumed approval still
            // needs an actionable reconciliation path; create the deterministic
            // reconciliation Decision (idempotent on retry).
            Some(decision)
                if decision.kind == DecisionKind::ToolApproval
                    && matches!(decision.state, DecisionState::Answered | DecisionState::Consumed) =>
            {
                self.transition(workspace_id, &item, RecoveryFact::EnsureReconciliation)
                    .await
            }
            Some(_) => Err("unknown effect has an invalid reconciliation decision".to_string()),
        }
    }

    pub async fn request_cancellation(&self, workspace_id: &str, effect_id: &str) -> Result<(), String> {
        self.db
            .guard_recovery_matrix(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        if self
            .db
            .effect_intent(workspace_id, effect_id)
            .await
            .map_err(|error| error.to_string())?
            .is_some_and(|intent| intent.state.is_terminal())
        {
            return Ok(());
        }
        let item = self.item(workspace_id, effect_id).await?;
        if matches!(item.intent.state, EffectState::Unknown | EffectState::Cancelling) {
            return Ok(());
        }
        self.transition(workspace_id, &item, RecoveryFact::Cancel).await
    }
}
