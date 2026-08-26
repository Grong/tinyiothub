//! Heartbeat report parsing — extract structured HeartbeatResult from LLM text output.

use regex::Regex;
use std::sync::LazyLock;
use tracing::warn;

use tinyiothub_core::heartbeat::{ExecutedAction, HeartbeatResult, HeartbeatStatus};
use tinyiothub_policy::proposal::{Proposal, ProposalStatus};

static JSON_FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```json\s*\n([\s\S]*?)\n```").expect("JSON fence regex should compile"));

/// Parse an LLM-generated heartbeat report (JSON inside ```json fence or raw JSON).
pub fn parse_healing_report(raw: &str, workspace_id: &str) -> HeartbeatResult {
    let json_str = extract_json(raw);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(value) => HeartbeatResult {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            status: parse_status(&value),
            summary: value["summary"].as_str().unwrap_or("").to_string(),
            task_count: 0,
            executed_actions: parse_executed_actions(&value),
            proposals: parse_proposals(&value, workspace_id),
            error: value["error"].as_str().map(|s| s.to_string()),
        },
        Err(e) => {
            warn!(workspace_id, error = %e, "Failed to parse heartbeat report JSON");
            HeartbeatResult {
                id: uuid::Uuid::new_v4().to_string(),
                workspace_id: workspace_id.to_string(),
                status: HeartbeatStatus::Error,
                summary: String::new(),
                task_count: 0,
                executed_actions: vec![],
                proposals: vec![],
                error: Some(format!("JSON parse error: {}", e)),
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
        && end >= start
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
                    thing_id: a["device_id"].as_str().map(|s| s.to_string()),
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
                .map(|p| {
                    let tool_name = p["tool_name"].as_str().unwrap_or("").to_string();
                    Proposal {
                        // Always server-generated: LLM ids ("prop-1") collide across ticks.
                        id: uuid::Uuid::new_v4().to_string(),
                        workspace_id: workspace_id.to_string(),
                        agent_id: String::new(),
                        risk: tinyiothub_skills::trust::risk_for_tool(&tool_name).to_string(),
                        tool_name,
                        thing_id: p["device_id"].as_str().map(|s| s.to_string()),
                        summary: p["summary"].as_str().unwrap_or("").to_string(),
                        reason: p["reason"].as_str().unwrap_or("").to_string(),
                        parameters: p.get("parameters").cloned(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        status: ProposalStatus::Pending,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
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

    #[test]
    fn extract_json_does_not_panic_when_brace_precedes_bracket_open() {
        // "} ... {" — rfind('}') lands before find('{'); slicing [start..=end]
        // with end < start panics. Garbage LLM output must yield Error, not a crash.
        let result = parse_healing_report("} trailing junk {", "ws1");
        assert_eq!(result.status, HeartbeatStatus::Error);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_proposal_id_is_server_generated_not_llm_supplied() {
        // LLM ids like "prop-1" collide across ticks; the parser must always
        // mint a fresh uuid and ignore any id the LLM provides.
        let raw = r#"{"status": "complete", "summary": "s", "executed_actions": [],
          "proposals": [
            {"id": "prop-1", "tool_name": "reboot_device", "device_id": "d1", "summary": "reboot", "reason": "stuck"}
          ]}"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.proposals.len(), 1);
        let id = &result.proposals[0].id;
        assert_ne!(id, "prop-1");
        assert!(
            uuid::Uuid::parse_str(id).is_ok(),
            "proposal id must be a server-generated uuid, got: {}",
            id
        );
    }

    #[test]
    fn test_proposal_parameters_are_captured() {
        // Approve-and-execute needs the tool arguments; without them the
        // approval flow is a dead end.
        let raw = r#"{"status": "complete", "summary": "s", "executed_actions": [],
          "proposals": [
            {"tool_name": "write_properties", "device_id": "d1", "summary": "set", "reason": "tune",
             "parameters": {"device_id": "d1", "properties": {"target_temp": 22}}}
          ]}"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.proposals.len(), 1);
        let params = result.proposals[0]
            .parameters
            .as_ref()
            .expect("parameters must be captured from the LLM proposal");
        assert_eq!(params["properties"]["target_temp"], 22);
    }

    #[test]
    fn test_proposal_risk_is_computed_not_llm_reported() {
        // LLM claims the firmware update is "low" risk — the parser must
        // override with the locally computed risk from tool safety.
        let raw = r#"{"status": "partial", "summary": "s", "executed_actions": [],
          "proposals": [
            {"tool_name": "firmware_update", "device_id": "d1", "summary": "update", "reason": "patch", "risk": "low"},
            {"tool_name": "write_properties", "device_id": "d2", "summary": "set", "reason": "tune", "risk": "high"}
          ]}"#;
        let result = parse_healing_report(raw, "ws1");
        assert_eq!(result.proposals.len(), 2);
        assert_eq!(result.proposals[0].risk, "high");
        assert_eq!(result.proposals[1].risk, "medium");
    }
}
