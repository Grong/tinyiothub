use async_trait::async_trait;
use zeroclaw::providers::traits::Provider;

use super::super::pipeline::*;

pub struct MemoryAnalyzer {
    provider: Box<dyn Provider>,
}

impl MemoryAnalyzer {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Analyzer for MemoryAnalyzer {
    fn name(&self) -> &str {
        "memory_analyzer"
    }

    async fn analyze(&self, event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
        let active_memories_text: String = event
            .active_memories
            .iter()
            .map(|m| format!("- [{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let turn_text: String =
            event.turn_messages.iter().map(|m| format!("{}: {}\n", m.role, m.content)).collect();

        let message = build_reflection_message(&active_memories_text, &turn_text);

        tracing::info!(
            message_len = message.len(),
            model = %event.model,
            "MemoryAnalyzer sending LLM request"
        );

        let response = self
            .provider
            .chat_with_system(None, &message, &event.model, Some(0.3))
            .await
            .map_err(|e| anyhow::anyhow!("Reflection LLM call failed: {}", e))?;

        tracing::info!(
            workspace = %event.workspace_id,
            agent = %event.agent_id,
            response_len = response.len(),
            response_preview = %response.chars().take(300).collect::<String>(),
            "MemoryAnalyzer received LLM response"
        );

        parse_reflection_response(&response)
    }
}

/// Build the message sent to the LLM.
/// Instruction is placed AFTER the data so the model's recency bias
/// makes it more likely to treat this as an extraction task.
fn build_reflection_message(active_memories_text: &str, turn_text: &str) -> String {
    let instruction = include_str!("../../../../../templates/agent/REFLECTION_PROMPT.md");

    format!(
        "## Conversation Turn\n{}\n## Active Memories\n{}\n---\n{}",
        turn_text, active_memories_text, instruction,
    )
}

/// Parse reflection response. Supports two formats:
/// 1. `FACT|zone|confidence|content` (preferred, pipe-delimited)
/// 2. `FACT: content` or `FACT: 无` (simple fallback)
fn parse_reflection_response(raw: &str) -> anyhow::Result<AnalyzerOutput> {
    let text = raw.trim();

    // Also try JSON as fallback in case the model does output JSON
    if (text.starts_with('{') || text.starts_with("```json"))
        && let Ok(output) = parse_json_format(text)
        && (!output.memory_candidates.is_empty() || !output.skill_candidates.is_empty())
    {
        return Ok(output);
    }

    if text.contains("NO_FACTS") || text.contains("FACT: 无") || text.contains("FACT:无") {
        return Ok(AnalyzerOutput::default());
    }

    let mut memory_candidates: Vec<MemoryCandidate> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(candidate) = try_parse_pipe_format(line) {
            memory_candidates.push(candidate);
        } else if let Some(candidate) = try_parse_simple_format(line) {
            memory_candidates.push(candidate);
        }
    }

    Ok(AnalyzerOutput { memory_candidates, skill_candidates: vec![], notifications: vec![] })
}

/// Parse `FACT|zone|confidence|content`
fn try_parse_pipe_format(line: &str) -> Option<MemoryCandidate> {
    let content = line.strip_prefix("FACT|")?;
    let parts: Vec<&str> = content.splitn(3, '|').collect();
    if parts.len() < 3 {
        return None;
    }
    let zone = parts[0].trim().to_lowercase();
    let confidence = parts[1].trim().to_lowercase();
    let fact = parts[2].trim().to_string();
    if fact.is_empty() {
        return None;
    }
    Some(MemoryCandidate {
        fact,
        zone,
        confidence,
        tags: vec![],
        supersedes: None,
        reasoning: String::new(),
    })
}

/// Parse `FACT: content` (simple format, defaults to general/medium)
fn try_parse_simple_format(line: &str) -> Option<MemoryCandidate> {
    let content = line
        .strip_prefix("FACT: ")
        .or_else(|| line.strip_prefix("FACT:"))
        .or_else(|| line.strip_prefix("FACT："))
        .or_else(|| line.strip_prefix("FACT： "))?;
    let content = content.trim();
    if content.is_empty() || content == "无" {
        return None;
    }
    Some(MemoryCandidate {
        fact: content.to_string(),
        zone: "general".to_string(),
        confidence: "medium".to_string(),
        tags: vec![],
        supersedes: None,
        reasoning: String::new(),
    })
}

/// Fallback JSON parser (for models that can produce JSON).
fn parse_json_format(raw: &str) -> anyhow::Result<AnalyzerOutput> {
    let cleaned = raw.trim().trim_start_matches("```json").trim_end_matches("```").trim();

    let parsed: serde_json::Value =
        serde_json::from_str(cleaned).unwrap_or_else(|_| serde_json::json!({}));

    let memory_candidates: Vec<MemoryCandidate> = parsed
        .get("memory_candidates")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(MemoryCandidate {
                        fact: item.get("fact")?.as_str()?.to_string(),
                        zone: item
                            .get("zone")
                            .and_then(|v| v.as_str())
                            .unwrap_or("general")
                            .to_string(),
                        confidence: item
                            .get("confidence")
                            .and_then(|v| v.as_str())
                            .unwrap_or("medium")
                            .to_string(),
                        tags: item
                            .get("tags")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter().filter_map(|t| t.as_str().map(String::from)).collect()
                            })
                            .unwrap_or_default(),
                        supersedes: item
                            .get("supersedes")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        reasoning: item
                            .get("reasoning")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let skill_candidates: Vec<SkillCandidate> = parsed
        .get("skill_candidates")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(SkillCandidate {
                        name: item.get("name")?.as_str()?.to_string(),
                        description: item
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        triggers: item
                            .get("triggers")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter().filter_map(|t| t.as_str().map(String::from)).collect()
                            })
                            .unwrap_or_default(),
                        body: item.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        reasoning: item
                            .get("reasoning")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(AnalyzerOutput { memory_candidates, skill_candidates, notifications: vec![] })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── prompt & message format ──

    #[test]
    fn prompt_template_loaded() {
        let prompt = include_str!("../../../../../templates/agent/REFLECTION_PROMPT.md");
        assert!(prompt.contains("FACT|"), "prompt must contain FACT| format instruction");
        assert!(prompt.contains("NO_FACTS"), "prompt must mention NO_FACTS");
        assert!(prompt.len() > 50, "prompt should not be empty or too short");
    }

    #[test]
    fn instruction_after_data_in_built_message() {
        let message = build_reflection_message("", "user: 你好\n");
        let data_pos = message.find("## Conversation Turn").unwrap();
        let instr_pos = message.find("FACT|").unwrap();
        assert!(
            data_pos < instr_pos,
            "instruction should come AFTER data (recency bias), \
             found data_pos={data_pos}, instr_pos={instr_pos}"
        );
    }

    #[test]
    fn built_message_contains_all_sections() {
        let message =
            build_reflection_message("- [work] old memory\n", "user: query\nassistant: answer\n");
        assert!(message.contains("## Conversation Turn"));
        assert!(message.contains("## Active Memories"));
        assert!(message.contains("FACT|"));
        assert!(message.contains("NO_FACTS"));
    }

    // ── parser: pipe format ──

    #[test]
    fn parse_pipe_format() {
        let input = "FACT|general|high|用户喜欢使用物联网设备";
        let output = parse_reflection_response(input).unwrap();
        assert_eq!(output.memory_candidates.len(), 1);
        assert_eq!(output.memory_candidates[0].fact, "用户喜欢使用物联网设备");
        assert_eq!(output.memory_candidates[0].zone, "general");
        assert_eq!(output.memory_candidates[0].confidence, "high");
    }

    #[test]
    fn parse_simple_format() {
        let input = "FACT: 用户是一名物联网工程师";
        let output = parse_reflection_response(input).unwrap();
        assert_eq!(output.memory_candidates.len(), 1);
        assert_eq!(output.memory_candidates[0].fact, "用户是一名物联网工程师");
        assert_eq!(output.memory_candidates[0].zone, "general");
        assert_eq!(output.memory_candidates[0].confidence, "medium");
    }

    #[test]
    fn parse_chinese_colon_format() {
        let input = "FACT：用户使用Modbus协议";
        let output = parse_reflection_response(input).unwrap();
        assert_eq!(output.memory_candidates.len(), 1);
        assert_eq!(output.memory_candidates[0].fact, "用户使用Modbus协议");
    }

    #[test]
    fn parse_no_facts() {
        let input = "NO_FACTS";
        let output = parse_reflection_response(input).unwrap();
        assert!(output.memory_candidates.is_empty());

        let input2 = "FACT: 无";
        let output2 = parse_reflection_response(input2).unwrap();
        assert!(output2.memory_candidates.is_empty());
    }

    #[test]
    fn parse_multiple_facts() {
        let input = "FACT|general|high|用户在北京\nFACT: 用户每天检查设备状态";
        let output = parse_reflection_response(input).unwrap();
        assert_eq!(output.memory_candidates.len(), 2);
    }

    #[test]
    fn ignore_garbage_lines() {
        let input = "这是模型前面乱说的废话\nFACT|general|medium|用户使用MQTT协议\n后面也是废话";
        let output = parse_reflection_response(input).unwrap();
        assert_eq!(output.memory_candidates.len(), 1);
        assert_eq!(output.memory_candidates[0].fact, "用户使用MQTT协议");
    }
}
