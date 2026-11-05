use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::agent_task::TaskStoreError;

/// Read-only port for the single approval source: the workspace `decisions`
/// table. The legacy platform `task_approvals` store has no writers and is not
/// consulted for Task status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalPromptFact {
    pub approval_id: String,
    pub status: String,
    pub detail: String,
}

#[async_trait]
pub trait PendingToolApproval: Send + Sync {
    async fn has_requested_tool_approval(&self, workspace_id: &str, task_id: &str) -> Result<bool, TaskStoreError>;

    /// Recorded approval outcomes for the Task prompt. Status is one of
    /// `requested`, `approved`, `denied`, `expired`, `withdrawn`.
    async fn tool_approval_facts(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<Vec<ApprovalPromptFact>, TaskStoreError>;

    /// Cancel closes a requested approval without waiting for the dispatcher:
    /// the Decision becomes `withdrawn` and its Prepared Effect `not_applied`.
    async fn withdraw_requested_approvals(&self, workspace_id: &str, task_id: &str) -> Result<u64, TaskStoreError>;
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

    async fn tool_approval_facts(
        &self,
        workspace_id: &str,
        task_id: &str,
    ) -> Result<Vec<ApprovalPromptFact>, TaskStoreError> {
        let decisions = self
            .db
            .harness_task_tool_approvals(workspace_id, task_id)
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))?;
        let redaction = tinyiothub_core::redaction::RedactionPolicy::default();
        Ok(decisions
            .into_iter()
            .map(|decision| {
                let status = match (decision.state, decision.answer.as_deref()) {
                    (tinyiothub_core::harness::DecisionState::Requested, _) => "requested".to_string(),
                    (_, Some("approved")) => "approved".to_string(),
                    (_, Some("denied")) => "denied".to_string(),
                    (tinyiothub_core::harness::DecisionState::Expired, _) => "expired".to_string(),
                    (tinyiothub_core::harness::DecisionState::Withdrawn, _) => "withdrawn".to_string(),
                    _ => "requested".to_string(),
                };
                let subject = redaction.redact(&decision.subject);
                let tool = subject.get("tool").and_then(|value| value.as_str()).unwrap_or("");
                let reason = subject
                    .get("reason")
                    .or_else(|| subject.get("summary"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                ApprovalPromptFact {
                    approval_id: decision.decision_id,
                    status,
                    detail: format!("{tool}: {reason}"),
                }
            })
            .collect())
    }

    async fn withdraw_requested_approvals(&self, workspace_id: &str, task_id: &str) -> Result<u64, TaskStoreError> {
        self.db
            .withdraw_requested_tool_approvals(workspace_id, task_id, chrono::Utc::now())
            .await
            .map_err(|error| TaskStoreError::Unavailable(error.to_string()))
    }
}
