mod dry_run;
mod health;
mod runner;
mod supervisor;
mod templates;

use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tinyiothub_core::harness::{WorkflowDefinition, WorkflowRevision, WorkflowTrigger};
use tinyiothub_storage::{Db, DbError};

pub use dry_run::{DryRunInput, DryRunResult, ScriptedDryRunExecutor, WorkflowDryRun};
pub use health::{
    DEFAULT_DUE_GRACE, DEFAULT_TICK_DEADLINE, DeadWakeSummary, StalledReason, SupervisorHealth,
    SupervisorHealthSnapshot, SupervisorStatus,
};
pub use runner::{MainCoordinator, SubagentRunner};
pub use supervisor::WorkflowScheduler;
pub use templates::{
    TemplateInput, WorkflowTemplate, builtin_templates, find_template, next_wake_at, validate_template,
};

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
        let current = self
            .db
            .current_workflow(&self.workspace_id, workflow_id)
            .await?
            .ok_or_else(|| DbError::NotFound {
                id: workflow_id.to_string(),
            })?;
        let revision = WorkflowRevision {
            workflow_id: workflow_id.to_string(),
            version: expected_version + 1,
            // R30: content edits append a revision; the enabled pointer is
            // preserved and only an explicit enable/disable action changes it.
            enabled: current.enabled,
            definition: input,
            created_at: Utc::now(),
        };
        self.db
            .append_workflow_revision(&self.workspace_id, &revision, expected_version)
            .await
    }

    /// Instantiate a validated built-in template as the first revision of a
    /// new Workflow. DR4=B: unlike [`Self::create`], success is persisted
    /// directly enabled. Invalid template references produce no Workflow row
    /// and never enqueue an occurrence or wake.
    pub async fn instantiate_template(
        &self,
        template_id: &str,
        input: TemplateInput,
    ) -> Result<WorkflowRevision, DbError> {
        let template = templates::find_template(template_id).ok_or_else(|| DbError::NotFound {
            id: format!("workflow-template:{template_id}"),
        })?;
        templates::validate_template(&template)?;

        let mut definition = template.definition.clone();
        if !templates::selector_is_empty(&input.device_selector) {
            definition.device_selector = input.device_selector;
        }
        if !templates::selector_is_empty(&input.source_selector) {
            definition.source_selector = input.source_selector;
        }
        definition.approval_policy =
            templates::effective_approval_policy(&template.definition, &input.approval_policy)?;
        Self::validate(&definition)?;
        templates::validate_workspace_references(&self.db, &self.workspace_id, template_id, &definition).await?;

        let workflow_id = input.workflow_id.trim();
        let workflow_id = if workflow_id.is_empty() {
            format!("workflow-{}", uuid::Uuid::new_v4())
        } else {
            if self
                .db
                .current_workflow(&self.workspace_id, workflow_id)
                .await?
                .is_some()
            {
                return Err(DbError::Conflict {
                    message: format!("workflow already exists: {workflow_id}"),
                });
            }
            workflow_id.to_string()
        };
        let revision = WorkflowRevision {
            workflow_id,
            version: 1,
            enabled: true,
            definition,
            created_at: Utc::now(),
        };
        self.db.create_enabled_workflow(&self.workspace_id, &revision).await
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

    pub async fn create_subagent(&self, parent_task_id: &str) -> Result<(String, String), DbError> {
        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        let run_id = format!("run-{}", uuid::Uuid::new_v4());
        self.db
            .create_subagent_task(&self.workspace_id, parent_task_id, &task_id, &run_id, Utc::now())
            .await?;
        Ok((task_id, run_id))
    }
}

pub(crate) fn normalize_cron(expression: &str) -> String {
    if expression.split_whitespace().count() == 5 {
        format!("0 {expression}")
    } else {
        expression.to_string()
    }
}
