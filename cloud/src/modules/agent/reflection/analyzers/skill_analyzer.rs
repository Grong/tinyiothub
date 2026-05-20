use async_trait::async_trait;
use super::super::pipeline::*;

pub struct SkillAnalyzer;

impl SkillAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Analyzer for SkillAnalyzer {
    fn name(&self) -> &str {
        "skill_analyzer"
    }

    async fn analyze(&self, _event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
        // Skill detection will be handled by the LLM in MemoryAnalyzer's reflection prompt.
        // For now the stub proves the pluggable architecture works.
        Ok(AnalyzerOutput::default())
    }
}
