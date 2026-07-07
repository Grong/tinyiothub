//! Heartbeat report parsing — extract structured HeartbeatResult from LLM text output.
//!
//! When the harness pipeline is active, prefer `build_loop_report()` and
//! `build_heartbeat_result_from_report()` over `parse_healing_report()`.
//! The latter is retained for backward compat with non-harness paths.

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tracing::warn;

use super::types::{ExecutedAction, HeartbeatResult, HeartbeatStatus};
use crate::harness::{LoopReport, SignalSource, StepResult, StepStatus, StepVerdict, TickVerdict};
use crate::proposal::{Proposal, ProposalStatus};

static JSON_FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```json\s*\n([\s\S]*?)\n```").expect("JSON fence regex should compile"));

/// Parse an LLM-generated heartbeat report (JSON inside ```json fence or raw JSON).
pub fn parse_healing_report(raw: &str, workspace_id: &str) -> HeartbeatResult {
    let json_str = extract_json(raw);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(value) => HeartbeatResult {
            workspace_id: workspace_id.to_string(),
            status: parse_status(&value),
            summary: value["summary"].as_str().unwrap_or("").to_string(),
            executed_actions: parse_executed_actions(&value),
            proposals: parse_proposals(&value, workspace_id),
            error: value["error"].as_str().map(|s| s.to_string()),
            pipeline_verdict: String::new(),
            lie_detected: false,
            tool_call_count: 0,
            duration_ms: 0,
        },
        Err(e) => {
            warn!(workspace_id, error = %e, "Failed to parse heartbeat report JSON");
            HeartbeatResult {
                workspace_id: workspace_id.to_string(),
                status: HeartbeatStatus::Error,
                summary: String::new(),
                executed_actions: vec![],
                proposals: vec![],
                error: Some(format!("JSON parse error: {}", e)),
                pipeline_verdict: String::new(),
                lie_detected: false,
                tool_call_count: 0,
                duration_ms: 0,
            }
        }
    }
}

fn extract_json(raw: &str) -> String {
    if let Some(captures) = JSON_FENCE_RE.captures(raw) {
        return captures[1].to_string();
    }
    if let Some(start) = raw.find('{')
        && let Some(end) = raw.rfind('}')
    {
        return raw[start..=end].to_string();
    }
    raw.to_string()
}

fn parse_status(value: &serde_json::Value) -> HeartbeatStatus {
    match value["status"].as_str() {
        Some("partial") | Some("Partial") => HeartbeatStatus::Partial,
        Some("error") | Some("Error") => HeartbeatStatus::Error,
        _ => HeartbeatStatus::Complete,
    }
}

fn parse_executed_actions(value: &serde_json::Value) -> Vec<ExecutedAction> {
    value["executed_actions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| ExecutedAction {
                    tool_name: a["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: a["device_id"].as_str().map(|s| s.to_string()),
                    success: a["success"].as_bool().unwrap_or(true),
                    details: a["details"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_proposals(value: &serde_json::Value, workspace_id: &str) -> Vec<Proposal> {
    value["proposals"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| Proposal {
                    id: p["id"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    workspace_id: workspace_id.to_string(),
                    agent_id: String::new(),
                    tool_name: p["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: p["device_id"].as_str().map(|s| s.to_string()),
                    summary: p["summary"].as_str().unwrap_or("").to_string(),
                    reason: p["reason"].as_str().unwrap_or("").to_string(),
                    risk: p["risk"].as_str().unwrap_or("low").to_string(),
                    parameters: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    status: ProposalStatus::Pending,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Build a LoopReport from structured step results (harness path).
///
/// This replaces the old LLM text parsing approach.
/// StepVerdict aggregation determines the TickVerdict.
pub fn build_loop_report(
    workspace_id: &str,
    trigger_source: SignalSource,
    steps: Vec<StepResult>,
    duration_ms: u64,
    stage_durations: HashMap<String, u64>,
) -> LoopReport {
    let mut tool_call_count: u32 = 0;
    let mut executed_actions: Vec<ExecutedAction> = Vec::new();
    let proposals: Vec<Proposal> = Vec::new();

    for step in &steps {
        for tc in &step.tool_calls {
            tool_call_count += 1;
            executed_actions.push(ExecutedAction {
                tool_name: tc.name.clone(),
                device_id: None,
                success: tc.success,
                details: if tc.success {
                    tc.output.clone()
                } else {
                    format!("BLOCKED: {}", tc.output)
                },
            });
        }
    }

    // Collect step verdicts for quality gate
    let step_verdicts: Vec<StepVerdict> = steps
        .iter()
        .map(|s| {
            if s.status == StepStatus::Done
                && !s.tool_calls.is_empty()
                && s.tool_calls.iter().all(|tc| !tc.success)
            {
                StepVerdict::Lying {
                    reason: format!(
                        "Step '{}' reported Done but all {} tool calls failed",
                        s.step_id,
                        s.tool_calls.len()
                    ),
                }
            } else if matches!(s.status, StepStatus::Failed { .. }) {
                StepVerdict::Incomplete {
                    reason: format!("Step '{}' failed", s.step_id),
                }
            } else if s.status == StepStatus::Done && s.tool_calls.is_empty() && s.output.is_empty()
            {
                StepVerdict::Incomplete {
                    reason: format!("Step '{}' completed with no output or tool calls", s.step_id),
                }
            } else {
                StepVerdict::Consistent
            }
        })
        .collect();

    let lie_detected = step_verdicts
        .iter()
        .any(|v| matches!(v, StepVerdict::Lying { .. }));

    let has_lies = lie_detected;
    let has_failures = step_verdicts
        .iter()
        .any(|v| matches!(v, StepVerdict::Incomplete { .. }));

    let verdict = if has_lies {
        TickVerdict::Fail {
            reason: "Lie detected in step verification".into(),
        }
    } else if has_failures {
        let escalated: Vec<String> = steps
            .iter()
            .filter(|s| {
                matches!(s.status, StepStatus::Failed { .. }) || s.status == StepStatus::Skipped
            })
            .map(|s| s.step_id.clone())
            .collect();
        TickVerdict::Partial { escalated }
    } else {
        TickVerdict::Pass
    };

    LoopReport {
        workspace_id: workspace_id.to_string(),
        trigger_source,
        verdict,
        steps,
        executed_actions,
        proposals,
        duration_ms,
        tool_call_count,
        lie_detected,
        stage_durations,
    }
}

/// Convert a LoopReport into the existing HeartbeatResult type for backward compat
/// with repository persistence and event publishing.
pub fn build_heartbeat_result_from_report(report: &LoopReport) -> HeartbeatResult {
    let status = match report.verdict {
        TickVerdict::Pass => HeartbeatStatus::Complete,
        TickVerdict::Partial { .. } => HeartbeatStatus::Partial,
        TickVerdict::Fail { .. } => HeartbeatStatus::Error,
    };

    let summary = report
        .steps
        .iter()
        .map(|s| format!("[{}] {}", s.step_id, s.output))
        .collect::<Vec<_>>()
        .join("\n");

    let pipeline_verdict = match report.verdict {
        TickVerdict::Pass => "Pass".to_string(),
        TickVerdict::Partial { .. } => "Partial".to_string(),
        TickVerdict::Fail { .. } => "Fail".to_string(),
    };

    HeartbeatResult {
        workspace_id: report.workspace_id.clone(),
        status,
        summary,
        executed_actions: report.executed_actions.clone(),
        proposals: report.proposals.clone(),
        error: if matches!(report.verdict, TickVerdict::Fail { .. }) {
            Some("Harness tick verification failed".into())
        } else {
            None
        },
        pipeline_verdict,
        lie_detected: report.lie_detected,
        tool_call_count: report.tool_call_count,
        duration_ms: report.duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_report() {
        let raw = r#"```json
{
  "status": "complete",
  "summary": "All devices healthy",
  "executed_actions": [
    {"tool_name": "check_temp", "device_id": "d1", "success": true, "details": "OK"}
  ],
  "proposals": []
}
```"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.status, HeartbeatStatus::Complete);
        assert_eq!(result.summary, "All devices healthy");
        assert_eq!(result.executed_actions.len(), 1);
        assert_eq!(result.executed_actions[0].tool_name, "check_temp");
    }

    #[test]
    fn test_parse_without_fence() {
        let raw = r#"{"status": "error", "summary": "Timeout", "error": "LLM timeout"}"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.status, HeartbeatStatus::Error);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_parse_partial() {
        let raw = r#"{"status": "partial", "summary": "Some failed", "executed_actions": [], "proposals": []}"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.status, HeartbeatStatus::Partial);
    }

    #[test]
    fn test_parse_malformed_json() {
        let raw = "not json at all";
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.status, HeartbeatStatus::Error);
        assert!(result.error.is_some());
    }
}
