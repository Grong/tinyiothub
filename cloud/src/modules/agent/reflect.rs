// Post-conversation memory extraction — simplified from ReflectionService
//
// What was a 9-file pipeline/analyzer/notification architecture is now a single
// function: reflect_conversation_turn(). It calls the LLM once to extract memory
// facts from a completed chat turn, then writes high-confidence facts directly
// to the MemoryStore. Lower-confidence facts are enqueued for review.

use sqlx::SqlitePool;
use tinyiothub_core::memory::{
    Confidence, MemoryInput, MemorySource, MemoryStore, MemoryZone, QueueCandidateInput,
};

// ── Core: post-turn reflection ──

/// Fire-and-forget after each chat turn. Extracts memory facts via a single
/// LLM call and writes high-confidence facts to the MemoryStore.
pub async fn reflect_conversation_turn(
    memory_store: &dyn MemoryStore,
    db: &SqlitePool,
    workspace_id: &str,
    agent_id: &str,
    session_key: &str,
    model: &str,
    turn_messages: &[ChatTurnMessage],
) {
    let _ = do_reflect(memory_store, db, workspace_id, agent_id, session_key, model, turn_messages)
        .await
        .inspect_err(|e| tracing::warn!(%workspace_id, %agent_id, "Reflection failed: {}", e));
}

async fn do_reflect(
    memory_store: &dyn MemoryStore,
    db: &SqlitePool,
    workspace_id: &str,
    agent_id: &str,
    session_key: &str,
    model: &str,
    turn_messages: &[ChatTurnMessage],
) -> anyhow::Result<()> {
    // 10-second dedup window per session
    if should_skip(db, session_key).await {
        return Ok(());
    }

    let active_memories = memory_store.list_active(workspace_id, agent_id).await?;

    let active_text: String = active_memories
        .iter()
        .map(|m| format!("- [{}] {}\n", m.zone.as_str(), m.content))
        .collect();

    let turn_text: String =
        turn_messages.iter().map(|m| format!("{}: {}\n", m.role, m.content)).collect();

    let prompt = build_reflection_prompt(&active_text, &turn_text);

    let provider =
        crate::shared::config::create_minimax_provider().map_err(|e| anyhow::anyhow!("{}", e))?;
    let response = provider
        .chat_with_system(None, &prompt, model, Some(0.3))
        .await
        .map_err(|e| anyhow::anyhow!("Reflection LLM call failed: {}", e))?;

    let candidates = parse_facts(&response);
    for c in &candidates {
        let confidence = match c.confidence.as_str() {
            "high" => Confidence::High,
            "low" => Confidence::Low,
            _ => Confidence::Medium,
        };
        let zone = match c.zone.as_str() {
            "core" => MemoryZone::Core,
            "work" => MemoryZone::Work,
            "episode" => MemoryZone::Episode,
            _ => MemoryZone::General,
        };
        let actual_zone = if matches!(zone, MemoryZone::Core) { MemoryZone::Work } else { zone };

        if matches!(confidence, Confidence::High) && !matches!(actual_zone, MemoryZone::Core) {
            memory_store
                .put(MemoryInput {
                    workspace_id: workspace_id.into(),
                    agent_id: agent_id.into(),
                    zone: actual_zone,
                    content: c.fact.clone(),
                    source: MemorySource::Reflection,
                    confidence,
                    tags: c.tags.clone(),
                    supersedes: c.supersedes.clone(),
                    ..Default::default()
                })
                .await?;
            log_action(db, session_key, workspace_id, agent_id, "auto_accept", &c.fact).await?;
        } else {
            let data = serde_json::to_string(c)?;
            memory_store
                .enqueue_candidate(QueueCandidateInput {
                    workspace_id: workspace_id.into(),
                    agent_id: agent_id.into(),
                    session_key: session_key.into(),
                    candidate_type: "memory".into(),
                    candidate_data: data,
                })
                .await?;
            log_action(db, session_key, workspace_id, agent_id, "deferred", &c.fact).await?;
        }
    }

    tracing::info!(%workspace_id, %agent_id, fact_count = candidates.len(), "Reflection complete");
    Ok(())
}

// ── Profile compilation (used by memory/handler.rs) ──

pub async fn compile_profile(
    memory_store: &dyn MemoryStore,
    workspace_id: &str,
    agent_id: &str,
    model: &str,
) -> anyhow::Result<String> {
    let memories = memory_store.list_active(workspace_id, agent_id).await?;
    let memories_text: String = memories
        .iter()
        .filter(|m| m.source != tinyiothub_core::memory::MemorySource::DeviceSnapshot)
        .map(|m| format!("[{}] {}\n", m.zone.as_str(), m.content))
        .collect();

    let prompt = include_str!("../../../templates/agent/COMPILE_PROMPT.md")
        .replace("{memories_text}", &memories_text);

    let provider =
        crate::shared::config::create_minimax_provider().map_err(|e| anyhow::anyhow!("{}", e))?;
    provider
        .chat_with_system(None, &prompt, model, Some(0.3))
        .await
        .map_err(|e| anyhow::anyhow!("Profile compilation LLM call failed: {}", e))
}

// ── Weekly digest ──

pub async fn generate_weekly_digest(
    memory_store: &dyn MemoryStore,
    workspace_id: &str,
    agent_id: &str,
    model: &str,
) -> anyhow::Result<String> {
    let since =
        (chrono::Utc::now() - chrono::Duration::days(7)).format("%Y-%m-%dT%H:%M:%S").to_string();
    let new_memories = memory_store.get_since(workspace_id, agent_id, &since).await?;

    let prompt = format!(
        "Generate a brief weekly summary (~100 words) of what you learned:\n\
         New facts: {} items\n\
         Write in the user's preferred language, friendly tone.\n\n\
         Recent memories:\n{}",
        new_memories.len(),
        new_memories.iter().map(|m| format!("- {}", m.content)).collect::<Vec<_>>().join("\n"),
    );

    let provider =
        crate::shared::config::create_minimax_provider().map_err(|e| anyhow::anyhow!("{}", e))?;
    provider
        .chat_with_system(None, &prompt, model, Some(0.5))
        .await
        .map_err(|e| anyhow::anyhow!("Weekly digest LLM call failed: {}", e))
}

// ── Internal helpers ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatTurnMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MemoryFact {
    fact: String,
    zone: String,
    confidence: String,
    tags: Vec<String>,
    supersedes: Option<String>,
}

async fn should_skip(db: &SqlitePool, session_key: &str) -> bool {
    let ten_secs_ago = chrono::Utc::now() - chrono::TimeDelta::seconds(10);
    let since = ten_secs_ago.format("%Y-%m-%dT%H:%M:%S").to_string();
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM reflection_log WHERE session_id = ? AND created_at > ? AND action = 'auto_accept'",
    )
    .bind(session_key)
    .bind(&since)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    row.map(|(c,)| c > 0).unwrap_or(false)
}

async fn log_action(
    db: &SqlitePool,
    session_id: &str,
    workspace_id: &str,
    agent_id: &str,
    action: &str,
    label: &str,
) -> anyhow::Result<()> {
    let label: String = label.chars().take(80).collect();
    sqlx::query(
        "INSERT INTO reflection_log (session_id, workspace_id, agent_id, action, target_type, label) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(workspace_id)
    .bind(agent_id)
    .bind(action)
    .bind("memory")
    .bind(&label)
    .execute(db)
    .await?;
    Ok(())
}

fn build_reflection_prompt(active_memories_text: &str, turn_text: &str) -> String {
    let instruction = include_str!("../../../templates/agent/REFLECTION_PROMPT.md");
    format!(
        "## Conversation Turn\n{}\n## Active Memories\n{}\n---\n{}",
        turn_text, active_memories_text, instruction,
    )
}

fn parse_facts(raw: &str) -> Vec<MemoryFact> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_template_loaded() {
        let prompt = include_str!("../../../templates/agent/REFLECTION_PROMPT.md");
        assert!(prompt.contains("FACT|"), "prompt must contain FACT| format instruction");
        assert!(prompt.contains("NO_FACTS"));
        assert!(prompt.len() > 50);
    }

    #[test]
    fn instruction_after_data() {
        let message = build_reflection_prompt("", "user: 你好\n");
        let data_pos = message.find("## Conversation Turn").unwrap();
        let instr_pos = message.find("FACT|").unwrap();
        assert!(data_pos < instr_pos);
    }

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
}
