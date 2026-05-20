use std::sync::Arc;
use sqlx::SqlitePool;
use tinyiothub_core::memory::{
    Confidence, MemoryInput, MemorySource, MemoryStore, MemoryZone,
    QueueCandidateInput,
};

use super::analyzers::memory_analyzer::MemoryAnalyzer;
use super::analyzers::security_analyzer::SecurityAnalyzer;
use super::analyzers::skill_analyzer::SkillAnalyzer;
use super::metrics::ReflectionMetrics;
use super::notifications::NotificationService;
use super::pipeline::*;

pub struct ReflectionService {
    pipeline: ReflectionPipeline,
    memory_store: Arc<dyn MemoryStore>,
    db: SqlitePool,
    pub metrics: Arc<ReflectionMetrics>,
    notification_service: Arc<NotificationService>,
}

impl ReflectionService {
    pub fn new(
        memory_store: Arc<dyn MemoryStore>,
        db: SqlitePool,
        notification_service: Arc<NotificationService>,
    ) -> Self {
        let mut pipeline = ReflectionPipeline::new();
        pipeline.add_analyzer(Box::new(MemoryAnalyzer::new()));
        pipeline.add_analyzer(Box::new(SkillAnalyzer::new()));
        pipeline.add_analyzer(Box::new(SecurityAnalyzer::new()));

        Self {
            pipeline,
            memory_store,
            db,
            metrics: Arc::new(ReflectionMetrics::new()),
            notification_service,
        }
    }

    /// Called after every chat turn (in tokio::spawn).
    pub async fn micro_reflect(
        &self,
        workspace_id: &str,
        agent_id: &str,
        session_key: &str,
        turn_messages: &[ChatMessage],
    ) {
        // 10-second dedup window
        if self.should_skip_auto_reflect(session_key).await {
            return;
        }

        let active_memories = match self
            .memory_store
            .list_active(workspace_id, agent_id)
            .await
        {
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
            turn_messages: turn_messages.to_vec(),
            active_memories,
        };

        let results = self.pipeline.execute(&event).await;
        let mut had_failure = false;

        for output in results {
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
    ) -> anyhow::Result<String> {
        let memories = self.memory_store.list_active(workspace_id, agent_id).await?;
        let memories_text: String = memories
            .iter()
            .filter(|m| m.source != MemorySource::DeviceSnapshot)
            .map(|m| format!("[{}] {}\n", m.zone.as_str(), m.content))
            .collect();

        let prompt = include_str!("../../../../templates/agent/COMPILE_PROMPT.md")
            .replace("{memories_text}", &memories_text);

        // Return the prompt — LLM call is deferred to the caller
        // (the agent's existing provider infrastructure handles LLM calls)
        tracing::info!(workspace_id, agent_id, "Profile compilation prompt prepared");
        Ok(prompt)
    }

    async fn should_skip_auto_reflect(&self, session_key: &str) -> bool {
        let ten_secs_ago =
            chrono::Utc::now() - chrono::TimeDelta::seconds(10);
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

        // Reflection source: confidence capped at medium, never auto-accept to core
        let actual_confidence = if matches!(confidence, Confidence::High) {
            Confidence::Medium
        } else {
            confidence
        };
        let actual_zone = if matches!(zone, MemoryZone::Core) {
            MemoryZone::Work
        } else {
            zone
        };

        if matches!(actual_confidence, Confidence::High)
            && !matches!(actual_zone, MemoryZone::Core)
        {
            // Auto-accept (but confidence is capped, so this branch won't fire often)
            self.memory_store
                .put(MemoryInput {
                    workspace_id: workspace_id.into(),
                    agent_id: agent_id.into(),
                    zone: actual_zone,
                    content: candidate.fact.clone(),
                    source: MemorySource::Reflection,
                    confidence: actual_confidence,
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
        self.log_action(
            session_key,
            workspace_id,
            agent_id,
            "deferred",
            "skill",
            &candidate.name,
        )
        .await?;

        // Push skill discovery notification to frontend
        self.notification_service.notify_skill_discovered(
            workspace_id,
            &candidate.name,
            &candidate.description,
        )
        .await;

        Ok(())
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
