use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Reflection event passed to all analyzers.
#[derive(Clone)]
pub struct ReflectionEvent {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_key: String,
    pub model: String,
    pub turn_messages: Vec<ChatMessage>,
    pub active_memories: Vec<tinyiothub_core::memory::AgentMemory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Output from a single analyzer.
#[derive(Debug, Clone, Default)]
pub struct AnalyzerOutput {
    pub memory_candidates: Vec<MemoryCandidate>,
    pub skill_candidates: Vec<SkillCandidate>,
    pub notifications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub fact: String,
    pub zone: String,
    pub confidence: String,
    pub tags: Vec<String>,
    pub supersedes: Option<String>,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCandidate {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub body: String,
    pub reasoning: String,
}

/// Analyzer trait — each implementation processes ReflectionEvents.
#[async_trait]
pub trait Analyzer: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput>;
}

/// Pipeline executes analyzers sequentially, isolated via tokio::spawn.
pub struct ReflectionPipeline {
    analyzers: Vec<Arc<dyn Analyzer>>,
}

impl ReflectionPipeline {
    pub fn new() -> Self {
        Self { analyzers: vec![] }
    }

    pub fn add_analyzer(&mut self, analyzer: Box<dyn Analyzer>) {
        self.analyzers.push(Arc::from(analyzer));
    }

    pub async fn execute(&self, event: &ReflectionEvent) -> Vec<AnalyzerOutput> {
        let mut results = vec![];
        for analyzer in &self.analyzers {
            let event = event.clone();
            let analyzer_name = analyzer.name().to_string();
            let analyzer = Arc::clone(analyzer);
            let handle = tokio::spawn(async move {
                analyzer.analyze(&event).await
            });
            match handle.await {
                Ok(Ok(output)) => results.push(output),
                Ok(Err(e)) => tracing::error!(analyzer = %analyzer_name, error = %e, "Analyzer failed"),
                Err(join_err) => {
                    let msg = match join_err.try_into_panic() {
                        Ok(p) => p
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| p.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".to_string()),
                        Err(_) => "cancelled".to_string(),
                    };
                    tracing::error!(analyzer = %analyzer_name, panic = %msg, "Analyzer panicked");
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct PanicAnalyzer;
    #[async_trait]
    impl Analyzer for PanicAnalyzer {
        fn name(&self) -> &str {
            "panic_test"
        }
        async fn analyze(&self, _event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
            panic!("deliberate panic for testing");
        }
    }

    struct OkAnalyzer;
    #[async_trait]
    impl Analyzer for OkAnalyzer {
        fn name(&self) -> &str {
            "ok_test"
        }
        async fn analyze(&self, _event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
            Ok(AnalyzerOutput::default())
        }
    }

    #[tokio::test]
    async fn pipeline_catches_analyzer_panic() {
        let mut pipeline = ReflectionPipeline::new();
        pipeline.add_analyzer(Box::new(PanicAnalyzer));
        pipeline.add_analyzer(Box::new(OkAnalyzer));

        let event = ReflectionEvent {
            workspace_id: "ws".into(),
            agent_id: "a".into(),
            session_key: "sk".into(),
            model: "minimax-m2".into(),
            turn_messages: vec![],
            active_memories: vec![],
        };

        let results = pipeline.execute(&event).await;
        // PanicAnalyzer panicked -> skipped; OkAnalyzer still ran
        assert_eq!(results.len(), 1);
    }
}
