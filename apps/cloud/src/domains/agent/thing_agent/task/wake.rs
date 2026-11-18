use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::agent_task::WakeDispatchState;

use super::coordinator::{Advance, TaskCoordinator, TaskError};
use tinyiothub_agent::port::task::TaskStore;

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

    /// Emit the retained-intent gauge and prune only intents that are older
    /// than the bounded retention window **and** whose wake is no longer live
    /// **and** whose Task is terminal or missing. Also drops in-memory staged
    /// intents whose wake was resolved out-of-band. Runs from the existing
    /// periodic runtime tick.
    pub async fn sweep_finalize_intents(&self, now: chrono::DateTime<chrono::Utc>) -> Result<usize, TaskError> {
        let (staging_dropped, memory_count) = self.coordinator.sweep_pending_staging().await;
        let pruned = self
            .store
            .prune_finalize_intents(now - chrono::Duration::days(7))
            .await?;
        let count = self.store.count_finalize_intents().await?;
        tracing::info!(
            metric = "runtime_task_finalize_intents",
            count,
            memory_count,
            pruned,
            staging_dropped,
            "finalize intent gauge"
        );
        Ok(pruned)
    }

    pub async fn run(&self, limit: i64) -> Result<usize, TaskError> {
        let wakes = self.store.claim_pending_wakes(limit).await?;
        let count = wakes.len();
        for wake in wakes {
            let key = (wake.task_id.clone(), wake.event_id.clone());
            // The HTTP approval answer already consumed the Decision and ran
            // the Effect dispatch pass. Routing the wake only continues the
            // Task conversation, exactly like every other wake kind.
            let result = self.coordinator.advance(&wake.task_id, &wake.event_id).await;
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
                    if state == WakeDispatchState::Dead {
                        self.store
                            .mark_wake_dead(&wake.task_id, &wake.event_id, "dispatch_failed")
                            .await?;
                    } else {
                        self.store.mark_wake(&wake.task_id, &wake.event_id, state).await?;
                    }
                }
            }
        }
        Ok(count)
    }
}
