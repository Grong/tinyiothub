//! Heartbeat report parsing — extract structured HeartbeatResult from LLM text output.

use regex::Regex;
use std::sync::LazyLock;
use tracing::warn;

use super::types::{ExecutedAction, HeartbeatResult, HeartbeatStatus};
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
