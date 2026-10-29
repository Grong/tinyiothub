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
