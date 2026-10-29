use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::agent_task::{TaskStoreError, WakeDispatchState};

use super::approval::{ApprovalExecutor, ExecutionOutcome};
use super::coordinator::{Advance, TaskCoordinator, TaskError};
use crate::port::task::TaskStore;

const MAX_CONSECUTIVE_DISPATCH_FAILURES: u8 = 3;

pub struct WakeRouter {
    store: Arc<dyn TaskStore>,
    coordinator: Arc<TaskCoordinator>,
    approvals: Option<Arc<ApprovalExecutor>>,
    failures: DashMap<(String, String), u8>,
}

impl WakeRouter {
    pub fn new(store: Arc<dyn TaskStore>, coordinator: Arc<TaskCoordinator>) -> Self {
        Self {
            store,
            coordinator,
            approvals: None,
            failures: DashMap::new(),
        }
    }

    pub fn with_approvals(mut self, approvals: Arc<ApprovalExecutor>) -> Self {
        self.approvals = Some(approvals);
        self
    }

    /// Expire due approvals from the live routing loop. Returns the number of
    /// approvals expired. The resulting decision wakes are claimed by `run`.
    pub async fn expire_approvals(&self, now: chrono::DateTime<chrono::Utc>) -> Result<u64, TaskError> {
        let Some(approvals) = &self.approvals else {
            return Ok(0);
        };
        approvals
            .expire_due_at(now)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()).into())
    }

    /// Emit the retained-intent gauge and prune only intents whose wake is no
    /// longer live (or whose Task is terminal) and that are older than the
    /// bounded retention window. Runs from the existing periodic runtime tick.
    pub async fn sweep_finalize_intents(&self, now: chrono::DateTime<chrono::Utc>) -> Result<usize, TaskError> {
        let pruned = self
            .store
            .prune_finalize_intents(now - chrono::Duration::days(7))
            .await?;
        let count = self.store.count_finalize_intents().await?;
        tracing::info!(
            metric = "runtime_task_finalize_intents",
            count,
            pruned,
            "finalize intent gauge"
        );
        Ok(pruned)
    }

    pub async fn run(&self, limit: i64) -> Result<usize, TaskError> {
        let wakes = self.store.claim_pending_wakes(limit).await?;
        let count = wakes.len();
        for wake in wakes {
            let key = (wake.task_id.clone(), wake.event_id.clone());
            let signal = serde_json::from_str::<serde_json::Value>(&wake.signal_json).ok();
            let approval_workspace = signal.as_ref().and_then(|signal| {
                (signal.get("kind").and_then(serde_json::Value::as_str) == Some("approval_decided"))
                    .then(|| signal.get("workspace_id").and_then(serde_json::Value::as_str))
                    .flatten()
                    .map(str::to_owned)
            });
            let result = if let (Some(workspace_id), Some(approvals)) = (approval_workspace.as_deref(), &self.approvals)
            {
                approvals
                    .execute_decision(workspace_id, &wake.event_id)
                    .await
                    .map(|outcome| match outcome {
                        ExecutionOutcome::Deferred => Advance::Deferred,
                        ExecutionOutcome::Job(_) | ExecutionOutcome::NoExecution => Advance::Recorded,
                    })
                    .map_err(|error| TaskStoreError::Unavailable(error.to_string()).into())
            } else {
                self.coordinator.advance(&wake.task_id, &wake.event_id).await
            };
            match result {
                Ok(Advance::Deferred) => {
                    self.failures.remove(&key);
                    self.store
                        .mark_wake(&wake.task_id, &wake.event_id, WakeDispatchState::Pending)
                        .await?;
                }
                Ok(_) => {
                    self.failures.remove(&key);
                    self.store
                        .mark_wake(&wake.task_id, &wake.event_id, WakeDispatchState::Dispatched)
                        .await?;
                }
                Err(error) if self.coordinator.has_pending_staging(&wake.task_id, &wake.event_id) => {
                    // The report/finalize intent is still held in memory, so a
                    // transient store outage must not consume the dead-letter
                    // budget for this wake. Keep it claimable and retry.
                    self.failures.remove(&key);
                    tracing::warn!(
                        task_id = %wake.task_id,
                        event_id = %wake.event_id,
                        %error,
                        "Task wake dispatch deferred with an in-memory finalize intent"
                    );
                    self.store
                        .mark_wake(&wake.task_id, &wake.event_id, WakeDispatchState::Pending)
                        .await?;
                }
                Err(error) => {
                    let attempts = {
                        let mut attempts = self.failures.entry(key.clone()).or_insert(0);
                        *attempts = attempts.saturating_add(1);
                        *attempts
                    };
                    let state = if attempts >= MAX_CONSECUTIVE_DISPATCH_FAILURES {
                        WakeDispatchState::Dead
                    } else {
                        WakeDispatchState::Pending
                    };
                    tracing::warn!(
                        task_id = %wake.task_id,
                        event_id = %wake.event_id,
                        attempts,
                        %error,
                        "Task wake dispatch failed"
                    );
                    self.store.mark_wake(&wake.task_id, &wake.event_id, state).await?;
                }
            }
        }
        Ok(count)
    }
}
