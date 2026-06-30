//! Reflection logic — extract facts from conversation turns.
//!
//! Pure parsing and prompt-building functions. The cloud layer wires
//! these with LLM providers, DB logging, and MemoryStore persistence.

use crate::session::types::ChatTurnMessage;

use super::types::{MemoryError, MemoryFact, MAX_REFLECTION_INPUT_CHARS, INJECTION_PATTERNS};

/// Build a reflection prompt from active memories and a conversation turn.
/// `instruction` is the reflection instructions/template (e.g., from REFLECTION_PROMPT.md).
pub fn build_reflection_prompt(instruction: &str, active_memories_text: &str, turn_text: &str) -> String {
    format!(
        "## Conversation Turn\n{}\n## Active Memories\n{}\n---\n{}",
        turn_text, active_memories_text, instruction,
    )
}

/// Build input text from chat turn messages for reflection.
pub fn build_reflection_input(messages: &[ChatTurnMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sanitize reflection input: truncate and filter injection patterns.
pub fn sanitize_input(input: &str) -> String {
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

/// Parse the LLM's raw reflection response into a list of MemoryFacts.
pub fn parse_facts(raw: &str) -> Vec<MemoryFact> {
    let text = raw.trim();
    if text.contains("NO_FACTS") || text.contains("FACT: 无") || text.contains("FACT:无") {
        return vec![];
    }

    // Try JSON fallback
    if (text.starts_with('{') || text.starts_with("```json"))
        && let Some(facts) = try_parse_json(text)
        && !facts.is_empty()
    {
        return facts;
    }

    let mut facts = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(fact) = parse_pipe_fact(line) {
            facts.push(fact);
        } else if let Some(fact) = parse_simple_fact(line) {
            facts.push(fact);
        }
    }
    facts
}

fn parse_pipe_fact(line: &str) -> Option<MemoryFact> {
    let content = line.strip_prefix("FACT|")?;
    let parts: Vec<&str> = content.splitn(3, '|').collect();
    if parts.len() < 3 {
        return None;
    }
    let fact = parts[2].trim().to_string();
    if fact.is_empty() {
        return None;
    }
    Some(MemoryFact {
        fact,
        zone: parts[0].trim().to_lowercase(),
        confidence: parts[1].trim().to_lowercase(),
        tags: vec![],
        supersedes: None,
    })
}

fn parse_simple_fact(line: &str) -> Option<MemoryFact> {
    let content = line
        .strip_prefix("FACT: ")
        .or_else(|| line.strip_prefix("FACT:"))
        .or_else(|| line.strip_prefix("FACT："))
        .or_else(|| line.strip_prefix("FACT： "))?;
    let content = content.trim();
    if content.is_empty() || content == "无" {
        return None;
    }
    Some(MemoryFact {
        fact: content.to_string(),
        zone: "general".to_string(),
        confidence: "medium".to_string(),
        tags: vec![],
        supersedes: None,
    })
}

fn try_parse_json(raw: &str) -> Option<Vec<MemoryFact>> {
    let cleaned = raw.trim().trim_start_matches("```json").trim_end_matches("```").trim();
    let parsed: serde_json::Value = serde_json::from_str(cleaned).ok()?;
    let arr = parsed.get("memory_candidates")?.as_array()?;
    let facts: Vec<MemoryFact> = arr
        .iter()
        .filter_map(|item| {
            Some(MemoryFact {
                fact: item.get("fact")?.as_str()?.to_string(),
                zone: item.get("zone").and_then(|v| v.as_str()).unwrap_or("general").to_string(),
                confidence: item
                    .get("confidence")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium")
                    .to_string(),
                tags: item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                supersedes: item.get("supersedes").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect();
    if facts.is_empty() { None } else { Some(facts) }
}

/// Reflect on a conversation turn — extract facts, update profile.
///
/// Stub: the full implementation lives in cloud because it requires
/// an LLM provider, DB, and MemoryStore. This function demonstrates
/// the pipeline without those dependencies.
pub async fn reflect(messages: &[ChatTurnMessage]) -> Result<(), MemoryError> {
    let input = build_reflection_input(messages);
    let sanitized = sanitize_input(&input);

    if sanitized.trim().is_empty() {
        return Ok(());
    }

    tracing::debug!(chars = sanitized.len(), "Reflection input ready (stub)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize / input tests ──

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

    // ── fact parsing tests ──

    #[test]
    fn parse_pipe_format() {
        let input = "FACT|general|high|用户喜欢使用物联网设备";
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].fact, "用户喜欢使用物联网设备");
        assert_eq!(output[0].zone, "general");
        assert_eq!(output[0].confidence, "high");
    }

    #[test]
    fn parse_simple_format() {
        let input = "FACT: 用户是一名物联网工程师";
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].fact, "用户是一名物联网工程师");
        assert_eq!(output[0].confidence, "medium");
        assert_eq!(output[0].zone, "general");
    }

    #[test]
    fn parse_chinese_colon() {
        let input = "FACT：用户使用Modbus协议";
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].fact, "用户使用Modbus协议");
    }

    #[test]
    fn parse_no_facts() {
        assert!(parse_facts("NO_FACTS").is_empty());
        assert!(parse_facts("FACT: 无").is_empty());
        assert!(parse_facts("FACT:无").is_empty());
    }

    #[test]
    fn parse_multiple_facts() {
        let input = "FACT|general|high|用户在北京\nFACT: 用户每天检查设备状态";
        let output = parse_facts(input);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn ignore_garbage_lines() {
        let input = "前面乱说的废话\nFACT|general|medium|用户使用MQTT协议\n后面的废话";
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
    }

    #[test]
    fn try_parse_json_array() {
        let input = r#"{"memory_candidates": [{"fact": "用户偏好中文", "zone": "general", "confidence": "high"}]}"#;
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].fact, "用户偏好中文");
        assert_eq!(output[0].confidence, "high");
    }

    #[test]
    fn try_parse_json_with_markdown_fence() {
        let input = "```json\n{\"memory_candidates\": [{\"fact\": \"test\", \"zone\": \"work\", \"confidence\": \"low\"}]}\n```";
        let output = parse_facts(input);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].fact, "test");
        assert_eq!(output[0].zone, "work");
    }

    // ── prompt tests ──

    #[test]
    fn prompt_instruction_after_data() {
        let instruction = "Extract facts using FACT| format.\nFACT|zone|confidence|fact\nNO_FACTS for no facts";
        let prompt = build_reflection_prompt(instruction, "", "user: 你好\n");
        let data_pos = prompt.find("## Conversation Turn").unwrap();
        let instr_pos = prompt.find("FACT|").unwrap();
        assert!(data_pos < instr_pos);
    }
}
