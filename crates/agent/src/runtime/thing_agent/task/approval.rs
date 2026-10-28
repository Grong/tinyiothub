use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError};
use tinyiothub_core::task_approval::{ApprovalState, ApprovedToolCall};
use tinyiothub_runtime::job::service::JobService;

use super::coordinator::{Advance, TaskCoordinator, TaskError};
use crate::port::approval::{ApprovalError, TaskApprovalStore};

#[allow(clippy::large_enum_variant)] // Public outcome keeps the accepted Job inline.
#[derive(Debug)]
pub enum ExecutionOutcome {
    Job(DeviceJob),
    NoExecution,
    Deferred,
}

#[async_trait]
pub trait ApprovedJobs: Send + Sync {
    async fn submit_approved(&self, call: ApprovedToolCall) -> Result<Option<DeviceJob>, DeviceJobError>;
}

#[async_trait]
impl ApprovedJobs for JobService {
    async fn submit_approved(&self, call: ApprovedToolCall) -> Result<Option<DeviceJob>, DeviceJobError> {
        JobService::submit_approved(self, call).await
    }
}

#[async_trait]
pub trait ApprovalTasks: Send + Sync {
    async fn advance(&self, task_id: &str, event_id: &str) -> Result<Advance, TaskError>;
}

#[async_trait]
impl ApprovalTasks for TaskCoordinator {
    async fn advance(&self, task_id: &str, event_id: &str) -> Result<Advance, TaskError> {
        TaskCoordinator::advance(self, task_id, event_id).await
    }
}

pub struct ApprovalExecutor {
    store: Arc<dyn TaskApprovalStore>,
    jobs: Arc<dyn ApprovedJobs>,
    coordinator: Arc<dyn ApprovalTasks>,
}

impl ApprovalExecutor {
    pub fn new(
        store: Arc<dyn TaskApprovalStore>,
        jobs: Arc<dyn ApprovedJobs>,
        coordinator: Arc<dyn ApprovalTasks>,
    ) -> Self {
        Self {
            store,
            jobs,
            coordinator,
        }
    }

    pub async fn execute_decision(&self, event_id: &str) -> Result<ExecutionOutcome, ApprovalError> {
        let approval = self
            .store
            .get_by_decision_event(event_id)
            .await?
            .ok_or(ApprovalError::NotFound)?;
        match approval.state {
            ApprovalState::Approved => match self.jobs.submit_approved(ApprovedToolCall::from(&approval)).await? {
                Some(job) => Ok(ExecutionOutcome::Job(job)),
                None => Ok(ExecutionOutcome::Deferred),
            },
            ApprovalState::Rejected | ApprovalState::Expired | ApprovalState::Unavailable => {
                match self.coordinator.advance(&approval.task_id, event_id).await? {
                    Advance::Deferred => Ok(ExecutionOutcome::Deferred),
                    _ => Ok(ExecutionOutcome::NoExecution),
                }
            }
            ApprovalState::Pending => Ok(ExecutionOutcome::Deferred),
        }
    }

    pub async fn recover(&self, workspace_id: Option<&str>) -> Result<usize, ApprovalError> {
        self.store.expire_due(Utc::now()).await?;
        Ok(self.store.list_recoverable(workspace_id).await?.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use tinyiothub_core::agent_task::AgentTask;
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
    use tinyiothub_core::task_approval::{
        ApprovalDecision, ApprovalState, ApprovedToolCall, DecisionOutcome, TaskApproval,
    };

    use super::{ApprovalExecutor, ApprovedJobs, ExecutionOutcome};
    use crate::port::approval::{ApprovalError, TaskApprovalStore};
    use crate::runtime::thing_agent::task::coordinator::Advance;

    struct Store {
        approval: TaskApproval,
    }

    #[async_trait]
    impl TaskApprovalStore for Store {
        async fn create_pending(&self, _: &TaskApproval, _: i64) -> Result<AgentTask, ApprovalError> {
            Err(ApprovalError::Storage("not used".into()))
        }
        async fn get_scoped(&self, _: &str, _: &str, _: &str) -> Result<Option<TaskApproval>, ApprovalError> {
            Ok(Some(self.approval.clone()))
        }
        async fn get_by_decision_event(&self, event_id: &str) -> Result<Option<TaskApproval>, ApprovalError> {
            Ok((event_id == self.approval.decision_event_id).then(|| self.approval.clone()))
        }
        async fn list_for_task(&self, _: &str, _: &str) -> Result<Vec<TaskApproval>, ApprovalError> {
            Ok(vec![self.approval.clone()])
        }
        async fn decide(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: ApprovalDecision,
            _: &str,
            _: chrono::DateTime<Utc>,
            _: &tinyiothub_core::agent_task::TaskWake,
        ) -> Result<DecisionOutcome, ApprovalError> {
            Err(ApprovalError::Storage("not used".into()))
        }
        async fn list_recoverable(&self, _: Option<&str>) -> Result<Vec<TaskApproval>, ApprovalError> {
            Ok(vec![self.approval.clone()])
        }
        async fn expire_due(&self, _: chrono::DateTime<Utc>) -> Result<u64, ApprovalError> {
            Ok(0)
        }
        async fn cancel(&self, _: &str, _: &str, _: i64, _: chrono::DateTime<Utc>) -> Result<AgentTask, ApprovalError> {
            Err(ApprovalError::Storage("not used".into()))
        }
    }

    #[derive(Default)]
    struct Jobs {
        calls: Mutex<Vec<ApprovedToolCall>>,
    }

    #[async_trait]
    impl ApprovedJobs for Jobs {
        async fn submit_approved(&self, call: ApprovedToolCall) -> Result<Option<DeviceJob>, DeviceJobError> {
            self.calls.lock().expect("calls").push(call.clone());
            Ok(Some(DeviceJob {
                id: "job_1".into(),
                workspace_id: call.workspace_id,
                device_id: call.device_id,
                tool: call.tool,
                args_json: call.args_json,
                state: DeviceJobState::Accepted,
                stop_requested_at: None,
                progress_json: None,
                result_json: None,
                task_id: Some(call.task_id),
                run_id: Some(call.run_id),
                tool_call_id: Some(call.tool_call_id),
                driver_call_id: None,
                timeout_ms: 1_000,
                started_at: Utc::now(),
                ended_at: None,
            }))
        }
    }

    #[derive(Default)]
    struct Tasks {
        advances: Mutex<Vec<(String, String)>>,
        deferred: bool,
    }

    #[async_trait]
    impl super::ApprovalTasks for Tasks {
        async fn advance(
            &self,
            task_id: &str,
            event_id: &str,
        ) -> Result<Advance, crate::runtime::thing_agent::task::coordinator::TaskError> {
            self.advances
                .lock()
                .expect("advances")
                .push((task_id.into(), event_id.into()));
            Ok(if self.deferred {
                Advance::Deferred
            } else {
                Advance::Recorded
            })
        }
    }

    fn approval(state: ApprovalState) -> TaskApproval {
        let now = Utc::now();
        TaskApproval {
            id: "approval_1".into(),
            workspace_id: "ws_1".into(),
            task_id: "task_1".into(),
            run_id: "run_1".into(),
            tool_call_id: "call_1".into(),
            device_id: "device_1".into(),
            tool: "set_speed".into(),
            args_json: json!({"speed": 7, "frozen": true}),
            reason: "confirm".into(),
            state,
            decision: (state != ApprovalState::Pending).then_some(ApprovalDecision::Approve),
            decider_id: None,
            requested_at: now,
            decided_at: None,
            expires_at: now + chrono::Duration::hours(1),
            decision_event_id: "decision_1".into(),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn approved_execution_uses_only_frozen_persisted_fields() {
        let store = Arc::new(Store {
            approval: approval(ApprovalState::Approved),
        });
        let jobs = Arc::new(Jobs::default());
        let executor = ApprovalExecutor::new(store, jobs.clone(), Arc::new(Tasks::default()));
        let outcome = executor.execute_decision("decision_1").await.expect("execute");
        assert!(matches!(outcome, ExecutionOutcome::Job(ref job) if job.id == "job_1"));
        let calls = jobs.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args_json, json!({"speed": 7, "frozen": true}));
        assert_eq!(calls[0].tool_call_id, "call_1");
    }

    #[tokio::test]
    async fn recovery_preserves_decision_identity_for_router_dispatch() {
        let store = Arc::new(Store {
            approval: approval(ApprovalState::Approved),
        });
        let jobs = Arc::new(Jobs::default());
        let executor = ApprovalExecutor::new(store.clone(), jobs.clone(), Arc::new(Tasks::default()));
        assert_eq!(executor.recover(None).await.expect("recover"), 1);
        assert!(jobs.calls.lock().expect("calls").is_empty());
        executor
            .execute_decision(&store.approval.decision_event_id)
            .await
            .expect("router dispatch");
        let calls = jobs.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_call_id, "call_1");
        assert_eq!(store.approval.decision_event_id, "decision_1");
    }

    #[tokio::test]
    async fn recovery_leaves_decision_dispatch_to_the_wake_router() {
        let store = Arc::new(Store {
            approval: approval(ApprovalState::Approved),
        });
        let jobs = Arc::new(Jobs::default());
        let executor = ApprovalExecutor::new(store, jobs.clone(), Arc::new(Tasks::default()));
        assert_eq!(executor.recover(None).await.expect("recover"), 1);
        assert_eq!(executor.recover(None).await.expect("repeat recovery"), 1);
        assert!(jobs.calls.lock().expect("calls").is_empty());
        assert!(matches!(
            executor.execute_decision("decision_1").await.expect("router dispatch"),
            ExecutionOutcome::Job(_)
        ));
        assert_eq!(jobs.calls.lock().expect("calls").len(), 1);
    }

    #[tokio::test]
    async fn non_execution_decisions_advance_and_pause_defers() {
        for state in [
            ApprovalState::Rejected,
            ApprovalState::Expired,
            ApprovalState::Unavailable,
        ] {
            let tasks = Arc::new(Tasks {
                deferred: true,
                ..Tasks::default()
            });
            let executor = ApprovalExecutor::new(
                Arc::new(Store {
                    approval: approval(state),
                }),
                Arc::new(Jobs::default()),
                tasks.clone(),
            );
            assert!(matches!(
                executor.execute_decision("decision_1").await.expect("deferred"),
                ExecutionOutcome::Deferred
            ));
            assert_eq!(tasks.advances.lock().expect("advances").len(), 1);
        }
    }
}
