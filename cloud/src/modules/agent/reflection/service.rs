use std::sync::Arc;

use sqlx::SqlitePool;
use tinyiothub_core::memory::{
    Confidence, MemoryInput, MemorySource, MemoryStore, MemoryZone, QueueCandidateInput,
};

use super::{
    analyzers::{
        memory_analyzer::MemoryAnalyzer, security_analyzer::SecurityAnalyzer,
        skill_analyzer::SkillAnalyzer,
    },
    metrics::ReflectionMetrics,
    notifications::NotificationService,
    pipeline::*,
};

pub struct ReflectionService {
    pipeline: ReflectionPipeline,
    memory_store: Arc<dyn MemoryStore>,
    db: SqlitePool,
    pub metrics: Arc<ReflectionMetrics>,
    notification_service: Arc<NotificationService>,
    auth_token: String,
    /// Max concurrent micro_reflect calls (default 3).
    reflection_semaphore: Arc<tokio::sync::Semaphore>,
}

impl ReflectionService {
    pub fn new(
        memory_store: Arc<dyn MemoryStore>,
        db: SqlitePool,
        notification_service: Arc<NotificationService>,
        auth_token: String,
    ) -> Self {
        let provider = zeroclaw::providers::create_provider("minimaxi", Some(&auth_token))
            .expect("Failed to create reflection provider");

        let mut pipeline = ReflectionPipeline::new();
        pipeline.add_analyzer(Box::new(MemoryAnalyzer::new(provider)));
        pipeline.add_analyzer(Box::new(SkillAnalyzer::new()));
        pipeline.add_analyzer(Box::new(SecurityAnalyzer::new()));

        Self {
            pipeline,
            memory_store,
            db,
            metrics: Arc::new(ReflectionMetrics::new()),
            notification_service,
            auth_token,
            reflection_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
        }
    }

    /// Called after every chat turn (in tokio::spawn).
    pub async fn micro_reflect(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_key: &str,
        model: &str,
        turn_messages: &[ChatMessage],
    ) {
        tracing::info!(
            workspace = %workspace_id,
            agent = %agent_id,
            session = %session_key,
            turn_count = turn_messages.len(),
            "micro_reflect triggered after chat turn"
        );

        // Rate limit: max 3 concurrent reflections globally
        let _permit = match self.reflection_semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return, // semaphore closed
        };

        // 10-second dedup window
        if self.should_skip_auto_reflect(session_key).await {
            tracing::info!(session = %session_key, "micro_reflect skipped — dedup window");
            return;
        }

        let active_memories = match self.memory_store.list_active(workspace_id, agent_id).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(%e, "Failed to load active memories for reflection");
                self.metrics.record_failure();
                return;
            }
        };

        let event = ReflectionEvent {
            workspace_id: workspace_id.to_string(),
            agent_id: agent_id.to_string(),
            session_key: session_key.to_string(),
            model: model.to_string(),
            turn_messages: turn_messages.to_vec(),
            active_memories,
        };

        let results = self.pipeline.execute(&event).await;
        let mut had_failure = false;

        let total_candidates: usize = results.iter().map(|r| r.memory_candidates.len()).sum();
        tracing::info!(
            workspace = %workspace_id,
            agent = %agent_id,
            analyzer_count = results.len(),
            memory_candidates = total_candidates,
            "micro_reflect pipeline completed"
        );

        for output in results {
            for notification in &output.notifications {
                self.notification_service
                    .broadcast(workspace_id, "security_alert", notification)
                    .await;
            }
            for candidate in &output.memory_candidates {
                if let Err(e) = self
                    .process_memory_candidate(workspace_id, agent_id, session_key, candidate)
                    .await
                {
                    tracing::warn!(%e, "Failed to process memory candidate");
                    had_failure = true;
                }
            }
            for candidate in &output.skill_candidates {
                if let Err(e) = self
                    .process_skill_candidate(workspace_id, agent_id, session_key, candidate)
                    .await
                {
                    tracing::warn!(%e, "Failed to process skill candidate");
                    had_failure = true;
                }
            }
        }

        if had_failure {
            self.metrics.record_failure();
        } else {
            self.metrics.record_success();
        }
    }

    /// Public compile-profile trigger.
    pub async fn compile_profile(
        &self,
        workspace_id: &str,
        agent_id: &str,
        model: &str,
    ) -> anyhow::Result<String> {
        let memories = self.memory_store.list_active(workspace_id, agent_id).await?;
        let memories_text: String = memories
            .iter()
            .filter(|m| m.source != MemorySource::DeviceSnapshot)
            .map(|m| format!("[{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let prompt = include_str!("../../../../templates/agent/COMPILE_PROMPT.md")
            .replace("{memories_text}", &memories_text);

        let provider = zeroclaw::providers::create_provider("minimaxi", Some(&self.auth_token))
            .map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))?;

        let profile = provider
            .chat_with_system(None, &prompt, model, Some(0.3))
            .await
            .map_err(|e| anyhow::anyhow!("Profile compilation LLM call failed: {}", e))?;

        tracing::info!(workspace_id, agent_id, profile_len = profile.len(), "Profile compiled");
        Ok(profile)
    }

    async fn should_skip_auto_reflect(&self, session_key: &str) -> bool {
        let ten_secs_ago = chrono::Utc::now() - chrono::TimeDelta::seconds(10);
        let since = ten_secs_ago.format("%Y-%m-%dT%H:%M:%S").to_string();
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM reflection_log WHERE session_id = ? AND created_at > ? AND action = 'auto_accept'",
        )
        .bind(session_key)
        .bind(&since)
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten();
        row.map(|(c,)| c > 0).unwrap_or(false)
    }

    async fn process_memory_candidate(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_key: &str,
        candidate: &MemoryCandidate,
    ) -> anyhow::Result<()> {
        let confidence = match candidate.confidence.as_str() {
            "high" => Confidence::High,
            "low" => Confidence::Low,
            _ => Confidence::Medium,
        };
        let zone = match candidate.zone.as_str() {
            "core" => MemoryZone::Core,
            "work" => MemoryZone::Work,
            "episode" => MemoryZone::Episode,
            _ => MemoryZone::General,
        };

        // Reflection source: never auto-accept to core zone (safety measure)
        let actual_zone = if matches!(zone, MemoryZone::Core) { MemoryZone::Work } else { zone };

        if matches!(confidence, Confidence::High) && !matches!(actual_zone, MemoryZone::Core) {
            // Auto-accept high-confidence, non-core memories
            tracing::info!(
                workspace = %workspace_id,
                agent = %agent_id,
                zone = %actual_zone.as_str(),
                confidence = ?confidence,
                fact = %candidate.fact,
                "Memory auto-accepted"
            );
            self.memory_store
                .put(MemoryInput {
                    workspace_id: workspace_id.into(),
                    agent_id: agent_id.into(),
                    zone: actual_zone,
                    content: candidate.fact.clone(),
                    source: MemorySource::Reflection,
                    confidence,
                    tags: candidate.tags.clone(),
                    supersedes: candidate.supersedes.clone(),
                    ..Default::default()
                })
                .await?;
            self.log_action(
                session_key,
                workspace_id,
                agent_id,
                "auto_accept",
                "memory",
                &candidate.fact,
            )
            .await?;
        } else {
            // Defer to review queue
            let data = serde_json::to_string(candidate)?;
            self.memory_store
                .enqueue_candidate(QueueCandidateInput {
                    workspace_id: workspace_id.into(),
                    agent_id: agent_id.into(),
                    session_key: session_key.into(),
                    candidate_type: "memory".into(),
                    candidate_data: data,
                })
                .await?;
            self.log_action(
                session_key,
                workspace_id,
                agent_id,
                "deferred",
                "memory",
                &candidate.fact,
            )
            .await?;
        }
        Ok(())
    }

    async fn process_skill_candidate(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_key: &str,
        candidate: &SkillCandidate,
    ) -> anyhow::Result<()> {
        let data = serde_json::to_string(candidate)?;
        self.memory_store
            .enqueue_candidate(QueueCandidateInput {
                workspace_id: workspace_id.into(),
                agent_id: agent_id.into(),
                session_key: session_key.into(),
                candidate_type: "skill".into(),
                candidate_data: data,
            })
            .await?;
        self.log_action(session_key, workspace_id, agent_id, "deferred", "skill", &candidate.name)
            .await?;

        // Push skill discovery notification to frontend
        self.notification_service
            .notify_skill_discovered(workspace_id, &candidate.name, &candidate.description)
            .await;

        Ok(())
    }

    /// Generate a weekly digest via LLM.
    pub async fn generate_digest(&self, prompt: &str, model: &str) -> anyhow::Result<String> {
        let provider = zeroclaw::providers::create_provider("minimaxi", Some(&self.auth_token))
            .map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))?;
        let digest = provider
            .chat_with_system(None, prompt, model, Some(0.5))
            .await
            .map_err(|e| anyhow::anyhow!("Weekly digest LLM call failed: {}", e))?;
        Ok(digest)
    }

    async fn log_action(
        &self,
        session_id: &str,
        workspace_id: &str,
        agent_id: &str,
        action: &str,
        target_type: &str,
        label: &str,
    ) -> anyhow::Result<()> {
        let label_short: String = label.chars().take(80).collect();
        sqlx::query(
            "INSERT INTO reflection_log (session_id, workspace_id, agent_id, action, target_type, label) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(workspace_id)
        .bind(agent_id)
        .bind(action)
        .bind(target_type)
        .bind(&label_short)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
