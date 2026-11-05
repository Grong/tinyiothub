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

/// Split a JSON key into lowercase word tokens on separators and camelCase
/// boundaries. Token-boundary matching keeps `hotkey`/`monkey`/`keywords`
/// visible while still redacting a real `key` field.
fn key_tokens(key: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in key.chars() {
        if ch.is_alphanumeric() {
            if ch.is_uppercase() && previous_was_lower_or_digit && !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            previous_was_lower_or_digit = false;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Drop trailing ASCII digits so `password1`/`token123` still match their
/// secret word.
fn trim_trailing_digits(value: &str) -> &str {
    value.trim_end_matches(|ch: char| ch.is_ascii_digit())
}

impl RedactionPolicy {
    /// Whether a JSON object key holds secret material.
    ///
    /// Matching is token-boundary-aware but still catches real-world key
    /// variants: exact tokens (`api_key`, `key`), secret words with only
    /// digits appended (`password123`, `token123`), compounds ending in a
    /// multi-character secret word (`clientsecret`, `mysecret`,
    /// `ACCESSTOKEN`, `PRIVATEKEY`), and plurals. Benign words that merely
    /// contain `key` (`keywords`, `hotkey`, `monkey`) stay visible because the
    /// bare `key` token is excluded from compound matching.
    pub fn is_sensitive_key(&self, key: &str) -> bool {
        let normalized = key.to_ascii_lowercase();
        let compact: Vec<String> = self
            .sensitive_keys
            .iter()
            .map(|sensitive| {
                sensitive
                    .chars()
                    .filter(|ch| ch.is_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase()
            })
            .collect();
        if compact.iter().any(|sensitive| sensitive == &normalized) {
            return true;
        }
        // Whole-key compound: only multi-character secret words participate so
        // `monkey`/`hotkey`/`keywords` never match through the bare `key`. The
        // word may appear anywhere in the key, not only as a suffix, so
        // `secretkey`, `passwordhash`, `tokenvalue`, and `accesskeyid` are
        // covered without matching the benign `key` words.
        let without_digits = trim_trailing_digits(&normalized);
        if !without_digits.is_empty()
            && compact
                .iter()
                .any(|sensitive| sensitive.len() >= 4 && without_digits.contains(sensitive.as_str()))
        {
            return true;
        }
        key_tokens(key).into_iter().any(|token| {
            let singular = token.strip_suffix('s').unwrap_or(&token);
            if self
                .sensitive_keys
                .iter()
                .any(|sensitive| sensitive.eq_ignore_ascii_case(&token) || sensitive.eq_ignore_ascii_case(singular))
            {
                return true;
            }
            // A secret word with only digits appended (`password1`).
            let token_word = trim_trailing_digits(&token);
            !token_word.is_empty()
                && token_word.len() < token.len()
                && self
                    .sensitive_keys
                    .iter()
                    .any(|sensitive| sensitive.eq_ignore_ascii_case(token_word))
        })
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
            "key": "k",
            "apiKey": "ak",
            "access_token": "at",
        }));
        let text = redacted.to_string();
        for leaked in ["s3cret", "\"a\"", "\"x\"", "\"k\"", "\"ak\"", "\"at\""] {
            assert!(!text.contains(leaked), "leaked {leaked}: {text}");
        }
        assert_eq!(redacted["level"], 7);
        assert_eq!(redacted["credentials"]["password"], "[REDACTED]");
        assert_eq!(redacted["credentials"]["user"], "[REDACTED]");
        assert_eq!(redacted["tokens"][0], "[REDACTED]");
        assert_eq!(redacted["nested"][0]["clientSecret"], "[REDACTED]");
        assert_eq!(redacted["key"], "[REDACTED]");
        assert_eq!(redacted["apiKey"], "[REDACTED]");
        assert_eq!(redacted["access_token"], "[REDACTED]");
    }

    #[test]
    fn redacts_concatenated_numbered_and_case_variant_secret_keys() {
        let policy = RedactionPolicy::default();
        for key in [
            "password1",
            "password123",
            "token123",
            "clientsecret",
            "mysecret",
            "ACCESSTOKEN",
            "PRIVATEKEY",
            "apiKey1",
            "keys",
            // Prefix/compound secret words must match anywhere in the key, not
            // only as a suffix.
            "secretkey",
            "passwordhash",
            "tokenvalue",
            "accesskeyid",
        ] {
            assert!(policy.is_sensitive_key(key), "{key} must be redacted");
        }
        for key in ["keywords", "hotkey", "monkey", "monkey1", "hotkey123"] {
            assert!(!policy.is_sensitive_key(key), "{key} must stay visible");
        }
        let redacted = policy.redact(&json!({
            "password1": "p1",
            "token123": "t1",
            "clientsecret": "c1",
            "ACCESSTOKEN": "a1",
            "PRIVATEKEY": "k1",
            "mysecret": "m1",
            "keywords": ["visible"],
            "hotkey": "visible",
            "monkey": "visible",
        }));
        let text = redacted.to_string();
        for leaked in ["\"p1\"", "\"t1\"", "\"c1\"", "\"a1\"", "\"k1\"", "\"m1\""] {
            assert!(!text.contains(leaked), "leaked {leaked}: {text}");
        }
        assert_eq!(redacted["keywords"][0], "visible");
        assert_eq!(redacted["hotkey"], "visible");
        assert_eq!(redacted["monkey"], "visible");
    }

    #[test]
    fn benign_keys_containing_key_are_not_redacted() {
        let policy = RedactionPolicy::default();
        let redacted = policy.redact(&json!({
            "keywords": ["level", "speed"],
            "hotkey": "ctrl+k",
            "monkey": "banana",
            "keys": "visible",
        }));
        assert_eq!(redacted["keywords"][0], "level");
        assert_eq!(redacted["hotkey"], "ctrl+k");
        assert_eq!(redacted["monkey"], "banana");
        assert_eq!(
            redacted["keys"], "[REDACTED]",
            "the plural of a real secret key is still sensitive"
        );
    }
}
