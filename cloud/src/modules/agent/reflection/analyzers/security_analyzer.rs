use async_trait::async_trait;
use super::super::pipeline::*;

pub struct SecurityAnalyzer;

impl SecurityAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Analyzer for SecurityAnalyzer {
    fn name(&self) -> &str {
        "security_analyzer"
    }

    async fn analyze(&self, _event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
        // Stub: returns empty. Real implementation in Phase 4+ will:
        // - Detect prompt injection patterns in user messages
        // - Flag suspicious memory candidates
        // - Enforce source=Reflection -> confidence <= medium
        Ok(AnalyzerOutput::default())
    }
}
