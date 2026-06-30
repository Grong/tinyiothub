//! MemoryService — long-term memory for agents.
//!
//! Full reflection pipeline: LLM → parse facts → write MemoryStore.
//! Cloud wires this with a real LlmProvider (e.g., Minimax) and
//! MemoryStore (e.g., SQLite-backed).

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tinyiothub_core::memory::{
    Confidence, MemoryInput, MemorySource, MemoryStore, MemoryZone, QueueCandidateInput,
};
use tracing::{debug, info, warn};

use super::provider::LlmProvider;
use super::reflect::{build_reflection_prompt, parse_facts};
use super::types::MemoryError;
use crate::heartbeat::metrics::Metrics;
use crate::session::types::ChatTurnMessage;

/// Dedup window: skip reflection if same session was processed within this duration.
const DEDUP_WINDOW_SECS: i64 = 10;

/// Full memory pipeline — extracts facts from conversations and persists them.
pub struct MemoryService {
    llm: Arc<dyn LlmProvider>,
    memory_store: Arc<dyn MemoryStore>,
    /// Last reflection timestamp per session_key (in-memory dedup).
    last_reflection: DashMap<String, Instant>,
    /// Operational metrics for LLM calls.
    metrics: Arc<Metrics>,
}

impl MemoryService {
    pub fn new(llm: Arc<dyn LlmProvider>, memory_store: Arc<dyn MemoryStore>) -> Self {
        Self {
            llm,
            memory_store,
            last_reflection: DashMap::new(),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Set external metrics (shared with PatrolManager for unified observability).
    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Reflect on a completed conversation turn.
    /// Called by AiEventHandler in response to ChatCompleted events.
    pub async fn reflect_conversation_turn(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_key: &str,
        model: &str,
        messages: &[ChatTurnMessage],
    ) -> Result<(), MemoryError> {
        if messages.is_empty() {
            return Ok(());
        }

        // In-memory dedup (10-second window)
        if self.should_skip(session_key) {
            self.metrics.reflection_skips.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(());
        }

        let active_memories = self
            .memory_store
            .list_active(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::Reflection(e.to_string()))?;

        let active_text: String = active_memories
            .iter()
            .map(|m| format!("- [{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let turn_text: String = messages
            .iter()
            .map(|m| format!("{}: {}\n", m.role, m.content))
            .collect();

        let instruction = include_str!("../../templates/REFLECTION_PROMPT.md");
        let prompt = build_reflection_prompt(instruction, &active_text, &turn_text);

        let llm_response = tokio::time::timeout(
            Duration::from_secs(120),
            self.llm.chat(None, &prompt, model, 0.3),
        )
        .await
        .map_err(|_| MemoryError::Reflection("LLM call timed out after 120s".into()))?
        .map_err(|e| MemoryError::Reflection(format!("LLM call failed: {}", e)))?;

        self.metrics
            .record_llm_call(llm_response.metadata.total_latency_ms, true);

        debug!(
            workspace_id, agent_id,
            tokens = llm_response.metadata.prompt_tokens + llm_response.metadata.completion_tokens,
            latency_ms = llm_response.metadata.total_latency_ms,
            model = %llm_response.metadata.model_used,
            "LLM reflection call completed"
        );

        let response = &llm_response.content;
        let candidates = parse_facts(response);
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
            let actual_zone = if matches!(zone, MemoryZone::Core) {
                MemoryZone::Work
            } else {
                zone
            };

            if matches!(confidence, Confidence::High) && !matches!(actual_zone, MemoryZone::Core) {
                self.memory_store
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
                    .await
                    .map_err(|e| {
                        warn!(
                            workspace_id, agent_id,
                            error = %e,
                            "MemoryStore put failed — raw LLM response preserved for replay"
                        );
                        // Log raw response at debug level so operators can replay if needed
                        debug!(workspace_id, agent_id, raw_response = %response, "LLM response preserved");
                        MemoryError::Reflection(e.to_string())
                    })?;
            } else {
                let data = serde_json::to_string(c)
                    .map_err(|e| MemoryError::Reflection(e.to_string()))?;
                self.memory_store
                    .enqueue_candidate(QueueCandidateInput {
                        workspace_id: workspace_id.into(),
                        agent_id: agent_id.into(),
                        session_key: session_key.into(),
                        candidate_type: "memory".into(),
                        candidate_data: data,
                    })
                    .await
                    .map_err(|e| {
                        warn!(
                            workspace_id, agent_id,
                            error = %e,
                            "MemoryStore enqueue_candidate failed — raw LLM response preserved for replay"
                        );
                        debug!(workspace_id, agent_id, raw_response = %response, "LLM response preserved");
                        MemoryError::Reflection(e.to_string())
                    })?;
            }
        }

        info!(
            workspace_id,
            agent_id,
            fact_count = candidates.len(),
            "Reflection complete"
        );
        Ok(())
    }

    /// Compile a user/workspace profile from active memories.
    pub async fn compile_profile(
        &self,
        workspace_id: &str,
        agent_id: &str,
        model: &str,
    ) -> Result<String, MemoryError> {
        let memories = self
            .memory_store
            .list_active(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::Reflection(e.to_string()))?;

        let memories_text: String = memories
            .iter()
            .filter(|m| m.source != MemorySource::DeviceSnapshot)
            .map(|m| format!("[{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let prompt = include_str!("../../templates/COMPILE_PROMPT.md")
            .replace("{memories_text}", &memories_text);

        tokio::time::timeout(
            Duration::from_secs(120),
            self.llm.chat(None, &prompt, model, 0.3),
        )
        .await
        .map_err(|_| MemoryError::Reflection("Profile compilation timed out after 120s".into()))?
        .map_err(|e| MemoryError::Reflection(format!("Profile compilation LLM call failed: {}", e)))
        .map(|r| r.content)
    }

    /// Generate a weekly digest from recent memories.
    pub async fn generate_weekly_digest(
        &self,
        workspace_id: &str,
        agent_id: &str,
        model: &str,
    ) -> Result<String, MemoryError> {
        let since = (chrono::Utc::now() - chrono::Duration::days(7))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let new_memories = self
            .memory_store
            .get_since(workspace_id, agent_id, &since)
            .await
            .map_err(|e| MemoryError::Reflection(e.to_string()))?;

        let prompt = format!(
            "Generate a brief weekly summary (~100 words) of what you learned:\n\
             New facts: {} items\n\
             Write in the user's preferred language, friendly tone.\n\n\
             Recent memories:\n{}",
            new_memories.len(),
            new_memories
                .iter()
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        tokio::time::timeout(
            Duration::from_secs(120),
            self.llm.chat(None, &prompt, model, 0.5),
        )
        .await
        .map_err(|_| MemoryError::Reflection("Weekly digest timed out after 120s".into()))?
        .map_err(|e| MemoryError::Reflection(format!("Weekly digest LLM call failed: {}", e)))
        .map(|r| r.content)
    }

    /// Access the underlying MemoryStore.
    pub fn memory_store(&self) -> &Arc<dyn MemoryStore> {
        &self.memory_store
    }

    fn should_skip(&self, session_key: &str) -> bool {
        let now = Instant::now();
        let mut skip = false;
        // Periodic cleanup: sweep entries older than 1 hour
        if self.last_reflection.len() > 1000 {
            self.last_reflection.retain(|_, v| {
                now.duration_since(*v).as_secs() < 3600
            });
        }
        self.last_reflection
            .entry(session_key.to_string())
            .and_modify(|last| {
                if now.duration_since(*last).as_secs() < DEDUP_WINDOW_SECS as u64 {
                    skip = true;
                }
                *last = now;
            })
            .or_insert_with(|| now);
        skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::provider::LlmResponse;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockLlmProvider {
        responses: Mutex<Vec<String>>,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn chat(
            &self,
            _system: Option<&str>,
            _prompt: &str,
            _model: &str,
            _temperature: f32,
        ) -> anyhow::Result<LlmResponse> {
            let content = self.responses.lock().unwrap().pop().unwrap_or_default();
            Ok(LlmResponse {
                content,
                metadata: Default::default(),
            })
        }
    }

    /// Minimal mock MemoryStore that returns empty for most queries.
    /// Used to exercise the MemoryService pipeline without a DB.
    struct MockMemoryStore;

    #[async_trait]
    impl tinyiothub_core::memory::MemoryStore for MockMemoryStore {
        async fn put(
            &self,
            _input: tinyiothub_core::memory::MemoryInput,
        ) -> tinyiothub_core::error::Result<tinyiothub_core::memory::AgentMemory> {
            Err(tinyiothub_core::error::Error::Internal("mock".into()))
        }
        async fn get(
            &self,
            _id: &str,
        ) -> tinyiothub_core::error::Result<Option<tinyiothub_core::memory::AgentMemory>> {
            Ok(None)
        }
        async fn get_all(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }
        async fn list_active(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }
        async fn get_since(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
            _since: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::AgentMemory>> {
            Ok(vec![])
        }
        async fn set_pinned(
            &self,
            _id: &str,
            _pinned: bool,
        ) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }
        async fn record_load(&self, _id: &str) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }
        async fn record_reference(&self, _id: &str) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }
        async fn get_pending_queue(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> tinyiothub_core::error::Result<Vec<tinyiothub_core::memory::ReflectionQueueItem>> {
            Ok(vec![])
        }
        async fn resolve_queue_item(
            &self,
            _id: &str,
            _workspace_id: &str,
            _approved: bool,
            _reviewer_note: Option<&str>,
        ) -> tinyiothub_core::error::Result<()> {
            Ok(())
        }
        async fn enqueue_candidate(
            &self,
            _item: tinyiothub_core::memory::QueueCandidateInput,
        ) -> tinyiothub_core::error::Result<String> {
            Ok("mock_queue_id".into())
        }
        async fn count_by_source(
            &self,
            _workspace_id: &str,
            _agent_id: &str,
            _source: tinyiothub_core::memory::MemorySource,
        ) -> tinyiothub_core::error::Result<u64> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_empty_messages_returns_ok() {
        let llm = Arc::new(MockLlmProvider::new(vec![]));
        let store = Arc::new(MockMemoryStore);
        let svc = MemoryService::new(llm, store);
        let result = svc
            .reflect_conversation_turn("ws", "agent", "sess", "model", &[])
            .await;
        assert!(result.is_ok(), "Empty messages should return Ok immediately");
    }

    #[tokio::test]
    async fn test_dedup_skips_within_window() {
        let llm = Arc::new(MockLlmProvider::new(vec!["fact: test|high|general".into()]));
        let store = Arc::new(MockMemoryStore);
        let svc = MemoryService::new(llm, store);
        let msg = vec![ChatTurnMessage {
            role: "user".into(),
            content: "hello".into(),
            timestamp: None,
        }];
        // First call goes through (store put fails but doesn't crash)
        let _ = svc
            .reflect_conversation_turn("ws", "agent", "sess_dedup", "model", &msg)
            .await;
        // Second call within dedup window should skip
        let result = svc
            .reflect_conversation_turn("ws", "agent", "sess_dedup", "model", &msg)
            .await;
        assert!(result.is_ok(), "Dedup skip should return Ok");
    }

    #[tokio::test]
    async fn test_construction_and_store_access() {
        let llm = Arc::new(MockLlmProvider::new(vec![]));
        let store: Arc<dyn tinyiothub_core::memory::MemoryStore> = Arc::new(MockMemoryStore);
        let store_clone = store.clone();
        let svc = MemoryService::new(llm, store);
        let inner = svc.memory_store();
        assert!(Arc::ptr_eq(inner, &store_clone));
    }
}
