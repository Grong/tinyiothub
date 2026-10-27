use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::agent_task::WakeDispatchState;

use super::coordinator::{Advance, TaskCoordinator, TaskError};
use crate::port::task::TaskStore;

const MAX_CONSECUTIVE_DISPATCH_FAILURES: u8 = 3;

pub struct WakeRouter {
    store: Arc<dyn TaskStore>,
    coordinator: Arc<TaskCoordinator>,
    failures: DashMap<(String, String), u8>,
}

impl WakeRouter {
    pub fn new(store: Arc<dyn TaskStore>, coordinator: Arc<TaskCoordinator>) -> Self {
        Self {
            store,
            coordinator,
            failures: DashMap::new(),
        }
    }

    pub async fn run(&self, limit: i64) -> Result<usize, TaskError> {
        let wakes = self.store.claim_pending_wakes(limit).await?;
        let count = wakes.len();
        for wake in wakes {
            let key = (wake.task_id.clone(), wake.event_id.clone());
            match self.coordinator.advance(&wake.task_id, &wake.event_id).await {
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
