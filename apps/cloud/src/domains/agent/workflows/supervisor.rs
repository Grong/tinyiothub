use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use tinyiothub_core::harness::{WorkflowOccurrence, WorkflowTrigger};
use tinyiothub_storage::harness_workflow::CronWindow;
use tinyiothub_storage::{Db, DbError};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct WorkflowScheduler {
    db: Arc<Db>,
    workspace_id: String,
    health: super::SupervisorHealth,
}

impl WorkflowScheduler {
    pub fn new(db: Arc<Db>, workspace_id: impl Into<String>) -> Self {
        let workspace_id = workspace_id.into();
        let health = super::SupervisorHealth::new((*db).clone(), workspace_id.clone());
        Self {
            db,
            workspace_id,
            health,
        }
    }

    pub async fn run(self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = interval.tick() => {
                    // Monotonic-derived wall time: an in-process clock jump
                    // cannot fake a tick deadline or due boundary.
                    let now = self.health.monotonic_now();
                    if let Err(error) = self.health.record_tick_started(now).await {
                        tracing::error!(
                            workspace_id = %self.workspace_id,
                            %error,
                            "workflow scheduler health start failed"
                        );
                        continue;
                    }
                    match self.tick(now).await {
                        Ok(_) => {
                            if let Err(error) = self.health.record_tick_completed(now).await {
                                tracing::error!(
                                    workspace_id = %self.workspace_id,
                                    %error,
                                    "workflow scheduler health completion failed"
                                );
                            }
                        }
                        Err(error) => {
                            tracing::error!(
                                workspace_id = %self.workspace_id,
                                %error,
                                "workflow scheduler tick failed"
                            );
                            if let Err(record_error) = self.health.record_tick_error(now, &error.to_string()).await {
                                tracing::error!(
                                    workspace_id = %self.workspace_id,
                                    %record_error,
                                    "workflow scheduler health error record failed"
                                );
                            }
                        }
                    }
                    match self.health.observe().await {
                        Ok(snapshot) if matches!(snapshot.status, super::SupervisorStatus::Stalled) => {
                            tracing::warn!(
                                metric = "harness_supervisor_stalled",
                                workspace_id = %self.workspace_id,
                                reason = snapshot.stalled_reason.map(super::StalledReason::as_str),
                                "workflow scheduler stalled"
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::error!(
                                workspace_id = %self.workspace_id,
                                %error,
                                "workflow scheduler health observation failed"
                            );
                        }
                    }
                }
            }
        }
    }

    pub async fn tick(&self, now: DateTime<Utc>) -> Result<Vec<WorkflowOccurrence>, DbError> {
        let workflows = self.db.list_enabled_workflows(&self.workspace_id).await?;
        let mut occurrences = Vec::new();
        for workflow in workflows {
            let WorkflowTrigger::Cron { trigger_id, expression } = &workflow.definition.trigger else {
                continue;
            };
            let schedule =
                Schedule::from_str(&super::normalize_cron(expression)).map_err(|error| DbError::Validation {
                    message: format!("invalid persisted workflow cron: {error}"),
                })?;
            let Some(cursor) = self
                .db
                .cron_cursor(&self.workspace_id, &workflow.workflow_id, trigger_id)
                .await?
            else {
                if let Some(next) = schedule.after(&now).next() {
                    self.db
                        .initialize_cron_cursor(&self.workspace_id, &workflow.workflow_id, trigger_id, next)
                        .await?;
                }
                continue;
            };
            if cursor > now {
                continue;
            }
            let mut current = cursor;
            let mut next = schedule.after(&current).next().ok_or_else(|| DbError::Validation {
                message: "workflow cron has no subsequent slot".to_string(),
            })?;
            let mut missed = Vec::new();
            while next <= now {
                missed.push(current);
                current = next;
                next = schedule.after(&current).next().ok_or_else(|| DbError::Validation {
                    message: "workflow cron has no subsequent slot".to_string(),
                })?;
            }
            let due = Some(current);
            occurrences.extend(
                self.db
                    .record_cron_window(
                        &self.workspace_id,
                        &CronWindow {
                            workflow_id: &workflow.workflow_id,
                            trigger_id,
                            expected_slot: cursor,
                            missed_slots: &missed,
                            due_slot: due,
                            next_slot: next,
                            now,
                        },
                    )
                    .await?,
            );
        }
        Ok(occurrences)
    }

    pub async fn schedule_slot(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        slot: DateTime<Utc>,
    ) -> Result<Option<WorkflowOccurrence>, DbError> {
        self.db
            .schedule_workflow_slot(&self.workspace_id, workflow_id, trigger_id, slot)
            .await
    }

    pub async fn on_event(
        &self,
        trigger_id: &str,
        source_seq: i64,
        event_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<WorkflowOccurrence>, DbError> {
        let workflows = self.db.list_enabled_workflows(&self.workspace_id).await?;
        let mut occurrences = Vec::new();
        for workflow in workflows {
            if !matches!(
                &workflow.definition.trigger,
                WorkflowTrigger::Event { trigger_id: id } | WorkflowTrigger::Watch { trigger_id: id }
                    if id == trigger_id
            ) {
                continue;
            }
            if let Some(occurrence) = self
                .db
                .schedule_workflow_event(
                    &self.workspace_id,
                    &workflow.workflow_id,
                    trigger_id,
                    source_seq,
                    event_id,
                    now,
                )
                .await?
            {
                occurrences.push(occurrence);
            }
        }
        Ok(occurrences)
    }

    pub async fn on_source_unavailable(
        &self,
        trigger_id: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let workflows = self.db.list_enabled_workflows(&self.workspace_id).await?;
        for workflow in workflows {
            if matches!(
                &workflow.definition.trigger,
                WorkflowTrigger::Event { trigger_id: id } | WorkflowTrigger::Watch { trigger_id: id }
                    if id == trigger_id
            ) {
                self.db
                    .record_workflow_source_unavailable(
                        &self.workspace_id,
                        &workflow.workflow_id,
                        trigger_id,
                        reason,
                        now,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
