use async_trait::async_trait;
use super::super::pipeline::*;

pub struct MemoryAnalyzer;

impl MemoryAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Analyzer for MemoryAnalyzer {
    fn name(&self) -> &str {
        "memory_analyzer"
    }

    async fn analyze(&self, event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
        let reflection_prompt =
            include_str!("../../../../../templates/agent/REFLECTION_PROMPT.md");

        let active_memories_text: String = event
            .active_memories
            .iter()
            .map(|m| format!("- [{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let turn_text: String = event
            .turn_messages
            .iter()
            .map(|m| format!("{}: {}\n", m.role, m.content))
            .collect();

        let full_prompt = format!(
            "{}\n\n## Active Memories\n{}\n## Conversation Turn\n{}\n\nOutput JSON:",
            reflection_prompt, active_memories_text, turn_text
        );

        // For now, return empty — LLM integration will be wired in Task 13 (ReflectionService)
        // The MemoryAnalyzer formats the prompt; the ReflectionService will call the LLM
        tracing::debug!(
            workspace = %event.workspace_id,
            agent = %event.agent_id,
            prompt_len = full_prompt.len(),
            "MemoryAnalyzer prepared reflection prompt"
        );

        Ok(AnalyzerOutput::default())
    }
}
