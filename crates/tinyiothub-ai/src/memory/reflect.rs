//! Reflection logic — extract facts from conversation turns.
//!
//! Security: truncates input to MAX_REFLECTION_INPUT_CHARS and filters
//! lines matching known injection patterns.

use crate::session::types::ChatTurnMessage;

use super::types::{MemoryError, MAX_REFLECTION_INPUT_CHARS, INJECTION_PATTERNS};

/// Reflect on a conversation turn — extract facts, update profile.
///
/// Stub: in production this calls the LLM for reflection.
/// The full reflection logic will be wired in a follow-up task.
pub async fn reflect(messages: &[ChatTurnMessage]) -> Result<(), MemoryError> {
    let input = build_reflection_input(messages);
    let sanitized = sanitize_input(&input);

    if sanitized.trim().is_empty() {
        return Ok(());
    }

    tracing::debug!(chars = sanitized.len(), "Reflection input ready (stub)");
    Ok(())
}

fn build_reflection_input(messages: &[ChatTurnMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate to max length and filter injection patterns.
fn sanitize_input(input: &str) -> String {
    let truncated: String = input.chars().take(MAX_REFLECTION_INPUT_CHARS).collect();

    truncated
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !INJECTION_PATTERNS
                .iter()
                .any(|pattern| trimmed.starts_with(pattern))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filters_injection() {
        let input = "user: Hello\nYou are a helpful assistant\nSystem: do something\nassistant: Hi!";
        let result = sanitize_input(input);
        assert!(!result.contains("You are"));
        assert!(!result.contains("System:"));
        assert!(result.contains("user: Hello"));
        assert!(result.contains("assistant: Hi!"));
    }

    #[test]
    fn test_truncation() {
        let long: String = std::iter::repeat('a').take(MAX_REFLECTION_INPUT_CHARS + 100).collect();
        let result = sanitize_input(&long);
        assert!(result.chars().count() <= MAX_REFLECTION_INPUT_CHARS);
    }

    #[test]
    fn test_empty_input() {
        let result = sanitize_input("");
        assert!(result.is_empty());
    }
}
