use std::fmt::Write as _;

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::agent_task::AgentTask;
use tinyiothub_core::device_job::DeviceJob;
use tinyiothub_core::watch::WatchRule;

use crate::prompt::snapshot::section;
use crate::runtime::thing_agent::types::WakeSignal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPromptApproval {
    pub approval_id: String,
    pub status: String,
    pub detail: String,
}

pub fn build_task_prompt(
    task: &AgentTask,
    runs: &[RunReport],
    jobs: &[DeviceJob],
    watches: &[WatchRule],
    approvals: &[TaskPromptApproval],
    snapshot: &str,
    wake: Option<&WakeSignal>,
) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "Task objective\n<untrusted_task_context>\ntask_id: {}\nworkspace_id: {}\nobjective: {}\n</untrusted_task_context>",
        task.id, task.workspace_id, task.objective
    );
    if let Some(run) = runs.last() {
        let _ = write!(
            out,
            "\n\nLatest Run result\n<untrusted_task_context>\nrun_id: {}\noutcome: {}\nresult: {}\n</untrusted_task_context>",
            run.run_id,
            run.outcome.as_str(),
            run.summary
        );
    }

    let mut pending_jobs: Vec<&DeviceJob> = jobs.iter().filter(|job| job.state.is_active()).collect();
    pending_jobs.sort_by(|a, b| a.id.cmp(&b.id));
    if !pending_jobs.is_empty() {
        out.push_str("\n\nPending Jobs\n<untrusted_task_context>");
        for job in pending_jobs {
            let result = job
                .result_json
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_else(|| "none".to_string());
            let _ = write!(
                out,
                "\njob_id: {}\ndevice_id: {}\ntool: {}\nstate: {}\nstage_result: {}",
                job.id,
                job.device_id,
                job.tool,
                job.state.as_str(),
                result
            );
        }
        out.push_str("\n</untrusted_task_context>");
    }

    let mut pending_watches: Vec<&WatchRule> = watches.iter().collect();
    pending_watches.sort_by(|a, b| a.id.cmp(&b.id));
    if !pending_watches.is_empty() {
        out.push_str("\n\nPending watches\n<untrusted_task_context>");
        for watch in pending_watches {
            let condition = serde_json::to_string(&watch.condition).unwrap_or_else(|_| "invalid".to_string());
            let _ = write!(out, "\nrule_id: {}\ncondition: {}", watch.id, condition);
        }
        out.push_str("\n</untrusted_task_context>");
    }

    let mut pending_approvals: Vec<&TaskPromptApproval> = approvals.iter().collect();
    pending_approvals.sort_by(|a, b| a.approval_id.cmp(&b.approval_id));
    if !pending_approvals.is_empty() {
        out.push_str("\n\nPending approvals\n<untrusted_task_context>");
        for approval in pending_approvals {
            let _ = write!(
                out,
                "\napproval_id: {}\nstatus: {}\ndetail: {}",
                approval.approval_id, approval.status, approval.detail
            );
        }
        out.push_str("\n</untrusted_task_context>");
    }

    out.push_str(&section(snapshot));
    if let Some(wake) = wake {
        let facts = serde_json::to_string_pretty(wake).unwrap_or_else(|_| "invalid wake".to_string());
        let _ = write!(
            out,
            "\n\nWake facts\n<untrusted_task_context>\n{facts}\n</untrusted_task_context>"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tinyiothub_core::agent_runs::{Outcome, RunReport};
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus};
    use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};
    use tinyiothub_core::watch::{WatchCondition, WatchRule};

    use super::{TaskPromptApproval, build_task_prompt};
    use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

    fn task() -> AgentTask {
        AgentTask {
            id: "task_1".into(),
            workspace_id: "ws_1".into(),
            idempotency_key: "idem_1".into(),
            objective: "restore the line".into(),
            session_key: Some("session_1".into()),
            created_by: "user_1".into(),
            status: TaskStatus::Active,
            current_run_id: None,
            state_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ended_at: None,
        }
    }

    fn report(run_id: &str) -> RunReport {
        RunReport {
            run_id: run_id.into(),
            workspace_id: "ws_1".into(),
            trigger: "task".into(),
            outcome: Outcome::Acted,
            summary: format!("result for {run_id}"),
            actions: vec![],
            verified: true,
            duration_ms: 10,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: None,
            wake_event_id: None,
        }
    }

    fn job(job_id: &str) -> DeviceJob {
        DeviceJob {
            id: job_id.into(),
            workspace_id: "ws_1".into(),
            device_id: "device_1".into(),
            tool: "set_speed".into(),
            args_json: serde_json::json!({"speed": 2}),
            state: DeviceJobState::Running,
            stop_requested_at: None,
            progress_json: None,
            result_json: Some(serde_json::json!({"stage": "warming"})),
            task_id: Some("task_1".into()),
            run_id: Some("run_1".into()),
            tool_call_id: None,
            driver_call_id: None,
            timeout_ms: 1000,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    fn watch(rule_id: &str) -> WatchRule {
        WatchRule {
            id: rule_id.into(),
            workspace_id: "ws_1".into(),
            condition: WatchCondition::Availability {
                thing_id: "device_1".into(),
                online: true,
            },
            persistent: false,
            timeout_secs: 60,
            created_at: Utc::now().to_rfc3339(),
            owner_task_id: Some("task_1".into()),
        }
    }

    #[test]
    fn prompt_is_deterministic_ordered_and_does_not_cap_total_bytes() {
        let snapshot = "x".repeat(7000);
        let approvals = vec![
            TaskPromptApproval {
                approval_id: "approval_z".into(),
                status: "pending".into(),
                detail: "last".into(),
            },
            TaskPromptApproval {
                approval_id: "approval_a".into(),
                status: "pending".into(),
                detail: "first".into(),
            },
        ];
        let wake = WakeSignal {
            workspace_id: "ws_1".into(),
            priority: Priority::Normal,
            dedup_key: Some("wake_1".into()),
            source: TriggerSource::UserDirective {
                user_id: "user_1".into(),
                text: "device says <untrusted>".into(),
                session_key: Some("session_1".into()),
                source: None,
                problem_key: None,
            },
        };
        let prompt = build_task_prompt(
            &task(),
            &[report("run_old"), report("run_new")],
            &[job("job_z"), job("job_a")],
            &[watch("rule_z"), watch("rule_a")],
            &approvals,
            &snapshot,
            Some(&wake),
        );

        let ordered = [
            "Task objective",
            "Latest Run result",
            "Pending Jobs",
            "Pending watches",
            "Pending approvals",
            "工作区实时快照",
            "Wake facts",
        ];
        let mut previous = 0;
        for heading in ordered {
            let position = prompt.find(heading).expect("prompt section");
            assert!(position >= previous, "{heading} is out of order");
            previous = position;
        }
        assert!(prompt.find("job_a").expect("job_a") < prompt.find("job_z").expect("job_z"));
        assert!(prompt.find("rule_a").expect("rule_a") < prompt.find("rule_z").expect("rule_z"));
        assert!(prompt.find("approval_a").expect("approval_a") < prompt.find("approval_z").expect("approval_z"));
        assert!(prompt.contains("run_new"));
        assert!(!prompt.contains("run_old"));
        assert!(prompt.contains("warming"));
        assert!(prompt.contains(&snapshot));
        assert!(prompt.len() > 7000);
        assert!(prompt.contains("<untrusted_task_context>"));
    }
}
