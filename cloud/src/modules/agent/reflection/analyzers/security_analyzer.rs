use async_trait::async_trait;

use super::super::pipeline::*;

/// Placeholder analyzer — currently a no-op.
///
/// Real prompt injection detection requires semantic analysis (embedding
/// similarity, LLM-as-judge), not substring matching. The 18 hardcoded
/// patterns previously here created a false sense of security. This stub
/// will be filled in when real security patterns are identified from
/// production data (TODOS.md TODO-4).
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
        tracing::debug!("SecurityAnalyzer: stub — no patterns evaluated");
        Ok(AnalyzerOutput::default())
    }
}
