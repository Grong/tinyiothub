//! Typed, stable suppression errors.
//!
//! Every new Harness error carries a stable snake_case `code`, a safe `what`
//! and `fix`, a recovery descriptor, an anchor into the developer docs, an
//! allowlisted context object, trace information and an explicit redaction
//! note. Payloads, memory contents, ref internals and credentials never enter
//! [`SuppressionError`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Redaction contract shared by every suppression error.
pub const REDACTION_NOTE: &str = "payloads, memory contents, ref internals and credentials omitted";

/// Recovery guidance attached to an error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub action: String,
    pub max_attempts: u32,
    pub side_effects: String,
    pub baseline_advanced: bool,
    pub reconcile: String,
}

/// Correlation data. `local_path` is only ever populated locally.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceContext {
    pub trace_id: String,
    pub local_path: Option<String>,
}

/// Stable typed error used by the suppression gate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionError {
    pub code: String,
    pub what: String,
    pub fix: String,
    pub recovery: Recovery,
    pub code_anchor: String,
    /// Allowlisted context only (IDs, versions, hashes, budgets, mode).
    pub context: Value,
    pub trace: TraceContext,
    pub redaction: String,
    pub channel: String,
}

impl SuppressionError {
    /// Build an error with safe defaults. Callers may replace `context`,
    /// `recovery` or `trace` afterwards.
    pub fn new(code: impl Into<String>, what: impl Into<String>, fix: impl Into<String>) -> Self {
        let code = code.into();
        Self {
            code_anchor: format!("docs/dev/assistant-suppression.md#{code}"),
            code,
            what: what.into(),
            fix: fix.into(),
            recovery: Recovery {
                action: "none".to_string(),
                max_attempts: 0,
                side_effects: "none".to_string(),
                baseline_advanced: false,
                reconcile: "not_required".to_string(),
            },
            context: Value::Null,
            trace: TraceContext {
                trace_id: String::new(),
                local_path: None,
            },
            redaction: REDACTION_NOTE.to_string(),
            channel: "event + report".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTION_NOTE, SuppressionError};

    #[test]
    fn error_serializes_with_camel_case_and_stable_anchor() {
        let error = SuppressionError::new("source_unavailable", "capture source", "restore source");
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "source_unavailable");
        assert_eq!(
            value["codeAnchor"],
            "docs/dev/assistant-suppression.md#source_unavailable"
        );
        assert_eq!(value["redaction"], REDACTION_NOTE);
        assert!(value.get("maxAttempts").is_none());
        assert_eq!(value["recovery"]["maxAttempts"], 0);
    }
}
