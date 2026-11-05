use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::agent_task::TaskStoreError;

/// Read-only port for the single approval source: the workspace `decisions`
/// table. The legacy platform `task_approvals` store has no writers and is not
/// consulted for Task status.
#[async_trait]
pub trait PendingToolApproval: Send + Sync {
    async fn has_requested_tool_approval(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError>;
}

#[derive(Clone)]
pub struct HarnessPendingToolApproval {
    db: Arc<tinyiothub_storage::Db>,
}

impl HarnessPendingToolApproval {
    pub fn new(db: Arc<tinyiothub_storage::Db>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl PendingToolApproval for HarnessPendingToolApproval {
    async fn has_requested_tool_approval(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError> {
        self.db
            .harness_has_requested_tool_approval(workspace_id, task_id)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }
}
