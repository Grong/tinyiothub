use std::sync::Arc;

use chrono::{DateTime, Utc};
use tinyiothub_agent::harness::{MainAgent, MainInput, SubagentExecutor, SubagentInput, TaskOutcome};
use tinyiothub_core::harness::HarnessFact;
use tinyiothub_storage::{Db, DbError};

#[derive(Clone)]
pub struct MainCoordinator {
    db: Arc<Db>,
    workspace_id: String,
    agent: Arc<dyn MainAgent>,
}

impl MainCoordinator {
    pub fn new(db: Arc<Db>, workspace_id: impl Into<String>, agent: Arc<dyn MainAgent>) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
            agent,
        }
    }

    pub async fn on_fact(&self, fact: HarnessFact) -> anyhow::Result<()> {
        let projection = self.db.workflow_main_projection(&self.workspace_id).await?;
        self.agent
            .coordinate(MainInput {
                session_id: format!("supervisor:{}", self.workspace_id),
                fact,
                projection,
                transcript: Vec::new(),
            })
            .await
    }
}

#[derive(Clone)]
pub struct SubagentRunner {
    agent: Arc<dyn SubagentExecutor>,
}

impl SubagentRunner {
    pub fn new(agent: Arc<dyn SubagentExecutor>) -> Self {
        Self { agent }
    }

    pub async fn run(&self, mut input: SubagentInput) -> anyhow::Result<TaskOutcome> {
        input.session_id = format!("task:{}/run:{}", input.task_id, input.run_id);
        input.transcript.clear();
        self.agent.execute(input).await
    }

    pub async fn commit_outcome(
        &self,
        db: &Db,
        workspace_id: &str,
        task_id: &str,
        outcome: &TaskOutcome,
        now: DateTime<Utc>,
    ) -> Result<bool, DbError> {
        db.complete_subagent_task(
            workspace_id,
            task_id,
            &outcome.status,
            serde_json::to_value(outcome)?,
            now,
        )
        .await
    }
}
