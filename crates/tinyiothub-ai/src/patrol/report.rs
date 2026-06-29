//! Patrol report parsing — extract structured PatrolReport from LLM text output.

use regex::Regex;
use tracing::warn;

use super::types::{AutoExecutedAction, PatrolReport, PatrolStatus, PendingProposal};

/// Parse an LLM-generated healing report (JSON inside ```json fence or raw JSON).
pub fn parse_healing_report(raw: &str, workspace_id: &str) -> PatrolReport {
    let json_str = extract_json(raw);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(value) => PatrolReport {
            workspace_id: workspace_id.to_string(),
            status: parse_status(&value),
            summary: value["summary"].as_str().unwrap_or("").to_string(),
            executed_actions: parse_executed_actions(&value),
            pending_proposals: parse_pending_proposals(&value),
            error: value["error"].as_str().map(|s| s.to_string()),
        },
        Err(e) => {
            warn!(workspace_id, error = %e, "Failed to parse HealingReport JSON, returning error report");
            PatrolReport {
                workspace_id: workspace_id.to_string(),
                status: PatrolStatus::Error,
                summary: String::new(),
                executed_actions: vec![],
                pending_proposals: vec![],
                error: Some(format!("JSON parse error: {}", e)),
            }
        }
    }
}

fn extract_json(raw: &str) -> String {
    // Try ```json fence first
    let fence_re = Regex::new(r"```json\s*\n([\s\S]*?)\n```").unwrap();
    if let Some(captures) = fence_re.captures(raw) {
        return captures[1].to_string();
    }
    // Fallback: find first { ... } block
    if let Some(start) = raw.find('{')
        && let Some(end) = raw.rfind('}')
    {
        return raw[start..=end].to_string();
    }
    raw.to_string()
}

fn parse_status(value: &serde_json::Value) -> PatrolStatus {
    match value["status"].as_str() {
        Some("partial") | Some("Partial") => PatrolStatus::Partial,
        Some("error") | Some("Error") => PatrolStatus::Error,
        _ => PatrolStatus::Complete,
    }
}

fn parse_executed_actions(value: &serde_json::Value) -> Vec<AutoExecutedAction> {
    value["executed_actions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| AutoExecutedAction {
                    tool_name: a["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: a["device_id"].as_str().map(|s| s.to_string()),
                    success: a["success"].as_bool().unwrap_or(true),
                    details: a["details"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_pending_proposals(value: &serde_json::Value) -> Vec<PendingProposal> {
    value["pending_proposals"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| PendingProposal {
                    tool_name: p["tool_name"].as_str().unwrap_or("").to_string(),
                    device_id: p["device_id"].as_str().map(|s| s.to_string()),
                    proposed_action: p["proposed_action"].as_str().unwrap_or("").to_string(),
                    rationale: p["rationale"].as_str().unwrap_or("").to_string(),
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
  "pending_proposals": []
}
```"#;
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Complete);
        assert_eq!(report.summary, "All devices healthy");
        assert_eq!(report.executed_actions.len(), 1);
        assert_eq!(report.executed_actions[0].tool_name, "check_temp");
        assert_eq!(report.executed_actions[0].device_id, Some("d1".to_string()));
        assert!(report.executed_actions[0].success);
    }

    #[test]
    fn test_parse_without_fence() {
        let raw = r#"{"status": "error", "summary": "Timeout", "error": "LLM timeout"}"#;
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Error);
        assert_eq!(report.summary, "Timeout");
        assert!(report.error.is_some());
    }

    #[test]
    fn test_parse_partial() {
        let raw = r#"{"status": "partial", "summary": "Some actions failed", "executed_actions": [], "pending_proposals": []}"#;
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Partial);
    }

    #[test]
    fn test_parse_malformed_json() {
        let raw = "not json at all";
        let report = parse_healing_report(raw, "ws1");
        assert_eq!(report.status, PatrolStatus::Error);
        assert!(report.error.is_some());
    }
}
