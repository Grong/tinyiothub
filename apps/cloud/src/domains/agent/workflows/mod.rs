mod runner;
mod supervisor;

use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tinyiothub_core::harness::{WorkflowDefinition, WorkflowRevision, WorkflowTrigger};
use tinyiothub_storage::{Db, DbError};

pub use runner::{MainCoordinator, SubagentRunner};
pub use supervisor::WorkflowScheduler;

#[derive(Clone)]
pub struct WorkflowService {
    db: Arc<Db>,
    workspace_id: String,
}

impl WorkflowService {
    pub fn new(db: Arc<Db>, workspace_id: impl Into<String>) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
        }
    }

    fn validate(definition: &WorkflowDefinition) -> Result<(), DbError> {
        if definition.objective.trim().is_empty() {
            return Err(DbError::Validation {
                message: "workflow objective must not be blank".to_string(),
            });
        }
        if definition.trigger.trigger_id().trim().is_empty() {
            return Err(DbError::Validation {
                message: "workflow trigger ID must not be blank".to_string(),
            });
        }
        if let WorkflowTrigger::Cron { expression, .. } = &definition.trigger {
            let normalized = normalize_cron(expression);
            Schedule::from_str(&normalized).map_err(|error| DbError::Validation {
                message: format!("invalid workflow cron: {error}"),
            })?;
        }
        Ok(())
    }

    pub async fn create(&self, input: WorkflowDefinition) -> Result<WorkflowRevision, DbError> {
        Self::validate(&input)?;
        let revision = WorkflowRevision {
            workflow_id: format!("workflow-{}", uuid::Uuid::new_v4()),
            version: 1,
            enabled: false,
            definition: input,
            created_at: Utc::now(),
        };
        self.db.create_workflow(&self.workspace_id, &revision).await
    }

    pub async fn edit(
        &self,
        workflow_id: &str,
        expected_version: i64,
        input: WorkflowDefinition,
    ) -> Result<WorkflowRevision, DbError> {
        Self::validate(&input)?;
        let revision = WorkflowRevision {
            workflow_id: workflow_id.to_string(),
            version: expected_version + 1,
            enabled: false,
            definition: input,
            created_at: Utc::now(),
        };
        self.db
            .append_workflow_revision(&self.workspace_id, &revision, expected_version)
            .await
    }

    pub async fn set_enabled(&self, workflow_id: &str, enabled: bool) -> Result<WorkflowRevision, DbError> {
        self.db
            .set_workflow_enabled(&self.workspace_id, workflow_id, enabled, Utc::now())
            .await
    }

    pub async fn revision(&self, workflow_id: &str, version: i64) -> Result<WorkflowRevision, DbError> {
        self.db
            .workflow_revision(&self.workspace_id, workflow_id, version)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: format!("{workflow_id}@{version}"),
            })
    }
}

pub(crate) fn normalize_cron(expression: &str) -> String {
    if expression.split_whitespace().count() == 5 {
        format!("0 {expression}")
    } else {
        expression.to_string()
    }
}
