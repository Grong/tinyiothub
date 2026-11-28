//! ApprovalBroker — `ask` 模式下 Write/Destructive 工具的逐次审批。
//!
//! 工具注册一个 oneshot，事件 `approval/requested` 把 approvalId 送到前端；
//! 回答端点按 session_key 校验后 resolve。超时由调用方（工具）负责。

use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("approval not found")]
    NotFound,
    #[error("approval belongs to another session")]
    SessionMismatch,
    #[error("approval is no longer waiting for an answer")]
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSummary {
    pub approval_id: String,
    pub tool: String,
    pub summary: String,
}

struct PendingApproval {
    session_key: String,
    tool: String,
    summary: String,
    tx: oneshot::Sender<ApprovalDecision>,
}

#[derive(Default)]
pub struct ApprovalBroker {
    pending: DashMap<String, PendingApproval>,
}

impl ApprovalBroker {
    pub fn register(
        &self,
        session_key: &str,
        tool: &str,
        summary: &str,
    ) -> (String, oneshot::Receiver<ApprovalDecision>) {
        let approval_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(
            approval_id.clone(),
            PendingApproval {
                session_key: session_key.to_string(),
                tool: tool.to_string(),
                summary: summary.to_string(),
                tx,
            },
        );
        (approval_id, rx)
    }

    pub fn answer(
        &self,
        session_key: &str,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), ApprovalError> {
        // Hold the shard lock for the whole check-and-remove: a concurrent
        // wrong-session attempt must not remove the entry out from under the
        // legitimate owner (which would surface as a spurious `NotFound`).
        match self.pending.entry(approval_id.to_string()) {
            Entry::Occupied(entry) => {
                if entry.get().session_key != session_key {
                    return Err(ApprovalError::SessionMismatch);
                }
                let pending = entry.remove();
                pending.tx.send(decision).map_err(|_| ApprovalError::Closed)
            }
            Entry::Vacant(_) => Err(ApprovalError::NotFound),
        }
    }

    pub fn cancel(&self, approval_id: &str) {
        self.pending.remove(approval_id);
    }

    /// HUD 用：当前 workspace 下等待回答的审批数。
    pub fn count_pending_for_workspace(&self, workspace_id: &str) -> usize {
        let prefix = format!("agent:{workspace_id}:");
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key.starts_with(&prefix))
            .count()
    }

    pub fn list_for_session(&self, session_key: &str) -> Vec<ApprovalSummary> {
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key == session_key)
            .map(|entry| ApprovalSummary {
                approval_id: entry.key().clone(),
                tool: entry.value().tool.clone(),
                summary: entry.value().summary.clone(),
            })
            .collect()
    }
}

pub type SharedApprovalBroker = Arc<ApprovalBroker>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answer_resolves_registered_approval() {
        let broker = ApprovalBroker::default();
        let (id, rx) = broker.register("agent:ws:a/s1", "set_device", "把 3 号楼空调设为 24℃");
        broker.answer("agent:ws:a/s1", &id, ApprovalDecision::Approved).unwrap();
        assert_eq!(rx.await.unwrap(), ApprovalDecision::Approved);
    }

    #[test]
    fn wrong_session_cannot_answer() {
        let broker = ApprovalBroker::default();
        let (id, _rx) = broker.register("agent:ws:a/s1", "set_device", "x");
        let error = broker
            .answer("agent:ws:a/s2", &id, ApprovalDecision::Denied)
            .unwrap_err();
        assert!(matches!(error, ApprovalError::SessionMismatch));
        assert_eq!(broker.count_pending_for_workspace("ws"), 1);
    }

    #[test]
    fn lists_pending_for_session() {
        let broker = ApprovalBroker::default();
        let (_id, _rx) = broker.register("agent:ws:a/s1", "set_device", "x");
        let list = broker.list_for_session("agent:ws:a/s1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].tool, "set_device");
    }
}
