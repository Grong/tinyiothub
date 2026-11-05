//! Secret-redaction policy for public projections.
//!
//! `request_json` stays internal to dispatch. Any summary that can reach a
//! public DTO, SSE snapshot, prompt, or approval response is redacted through
//! this policy first.

use serde::{Deserialize, Serialize};

/// Keys that must never leave the internal dispatch/reconcile path. Matching is
/// case-insensitive and substring-based so `access_token`, `clientSecret`, and
/// nested `credentials.password` are all covered.
pub const DEFAULT_SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
    "credential",
    "key",
    "authorization",
];

/// Secret-redaction policy for argument summaries. `request_json` remains
/// internal; only the redacted summary may reach public DTO/SSE metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionPolicy {
    pub sensitive_keys: Vec<String>,
    pub replacement: String,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            sensitive_keys: DEFAULT_SENSITIVE_KEYS.iter().map(|key| (*key).to_string()).collect(),
            replacement: "[REDACTED]".to_string(),
        }
    }
}

impl RedactionPolicy {
    /// Whether a JSON object key holds secret material.
    pub fn is_sensitive_key(&self, key: &str) -> bool {
        let normalized = key.to_ascii_lowercase();
        self.sensitive_keys
            .iter()
            .any(|sensitive| normalized.contains(&sensitive.to_ascii_lowercase()))
    }

    /// Recursively redact object values (including inside arrays) while
    /// preserving the argument shape for display and correlation.
    pub fn redact(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut redacted = serde_json::Map::with_capacity(map.len());
                for (key, item) in map {
                    if self.is_sensitive_key(key) {
                        // A sensitive container must never leak descendant
                        // scalars (e.g. {"credentials":{"value":"hunter2"}} or
                        // {"tokens":["abc","def"]}). Preserve the shape but
                        // replace every scalar leaf.
                        redacted.insert(key.clone(), self.redact_all_scalars(item));
                    } else {
                        redacted.insert(key.clone(), self.redact(item));
                    }
                }
                serde_json::Value::Object(redacted)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|item| self.redact(item)).collect())
            }
            other => other.clone(),
        }
    }

    /// Replace every scalar descendant of a sensitive container with the
    /// redaction marker while retaining object keys and array length.
    fn redact_all_scalars(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), self.redact_all_scalars(item)))
                    .collect(),
            ),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|item| self.redact_all_scalars(item)).collect())
            }
            _ => serde_json::Value::String(self.replacement.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::RedactionPolicy;

    #[test]
    fn redacts_nested_secret_containers_and_arrays() {
        let policy = RedactionPolicy::default();
        let redacted = policy.redact(&json!({
            "level": 7,
            "credentials": {"password": "s3cret", "user": "operator"},
            "tokens": ["a", "b"],
            "nested": [{"clientSecret": "x"}],
        }));
        let text = redacted.to_string();
        assert!(!text.contains("s3cret"));
        assert!(!text.contains("\"a\""));
        assert!(!text.contains("\"x\""));
        assert_eq!(redacted["level"], 7);
        assert_eq!(redacted["credentials"]["password"], "[REDACTED]");
        assert_eq!(redacted["credentials"]["user"], "[REDACTED]");
        assert_eq!(redacted["tokens"][0], "[REDACTED]");
        assert_eq!(redacted["nested"][0]["clientSecret"], "[REDACTED]");
    }
}
