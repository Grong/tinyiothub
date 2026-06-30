//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatRunner.signal()
//! HeartbeatCompleted --> HeartbeatTaskRepository.insert_result()
//! ChatCompleted      --> MemoryService.reflect()
//! WorkspaceCreated    --> HeartbeatRunner.start()
//! WorkspaceDeleted    --> HeartbeatRunner.stop()

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tinyiothub_core::models::event::{ContentElement, Event, EventType, RichContent};
use tracing::{debug, error, info, warn};

use crate::event::bus::AiEventPublisher;
use crate::event::dlq::DeadLetterQueue;
use crate::event::types::AiEvent;
use crate::heartbeat::repo::HeartbeatTaskRepository;
use crate::heartbeat::runner::HeartbeatRunner;
use crate::heartbeat::types::SignalPriority;
use crate::memory::service::MemoryService;

/// Cross-domain callback handler.
pub struct AiEventHandler {
    heartbeat_runner: Arc<HeartbeatRunner>,
    task_repo: Arc<dyn HeartbeatTaskRepository>,
    memory_service: Arc<MemoryService>,
    event_publisher: Arc<AiEventPublisher>,
    dlq: Option<Arc<dyn DeadLetterQueue>>,
}

impl AiEventHandler {
    pub fn new(
        heartbeat_runner: Arc<HeartbeatRunner>,
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        memory_service: Arc<MemoryService>,
        event_publisher: Arc<AiEventPublisher>,
        dlq: Option<Arc<dyn DeadLetterQueue>>,
    ) -> Self {
        Self {
            heartbeat_runner,
            task_repo,
            memory_service,
            event_publisher,
            dlq,
        }
    }

    pub fn heartbeat_runner(&self) -> &Arc<HeartbeatRunner> {
        &self.heartbeat_runner
    }

    pub fn memory_service(&self) -> &Arc<MemoryService> {
        &self.memory_service
    }

    /// Handle an AiEvent variant, dispatched by the EventBus.
    pub async fn handle_ai_event(&self, event: &Event) {
        let _ai_event_type = match event.event_type() {
            EventType::Ai(t) => t,
            _ => return,
        };

        let payload_str = match extract_payload(event.content()) {
            Some(s) => s,
            None => {
                debug!("AiEvent has no text payload -- skipping");
                return;
            }
        };

        let ai_event: AiEvent = match serde_json::from_str(&payload_str) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to deserialize AiEvent payload");
                return;
            }
        };

        match &ai_event {
            AiEvent::AlarmCreated(alarm) => {
                let severity = alarm.severity.to_lowercase();
                if severity == "critical" || severity == "error" {
                    self.heartbeat_runner.signal(crate::heartbeat::types::HeartbeatSignal {
                        workspace_id: alarm.workspace_id.clone(),
                        reason: format!("Alarm: {}", alarm.message),
                        context: format!("device_id={}, alarm_type={}", alarm.device_id, alarm.alarm_type),
                        priority: if severity == "critical" {
                            SignalPriority::Critical
                        } else {
                            SignalPriority::High
                        },
                        device_id: Some(alarm.device_id.clone()),
                        alarm_type: Some(alarm.alarm_type.clone()),
                        rule_id: alarm.rule_id.clone(),
                    });
                }
            }
            AiEvent::HeartbeatCompleted { workspace_id, result } => {
                match self.task_repo.insert_result(workspace_id, result).await {
                    Ok(_) => debug!(workspace_id, "Heartbeat result persisted"),
                    Err(e) => {
                        error!(workspace_id, error = %e, "Failed to persist heartbeat result");

                        // Retry with exponential backoff
                        self.retry_with_backoff(workspace_id, result).await;
                    }
                }
            }
            AiEvent::ChatCompleted {
                workspace_id,
                agent_id,
                session_key,
                model,
                messages,
            } => {
                if let Err(e) = self
                    .memory_service
                    .reflect_conversation_turn(workspace_id, agent_id, session_key, model, messages)
                    .await
                {
                    warn!(
                        workspace_id,
                        agent_id,
                        error = %e,
                        "Memory reflection failed"
                    );
                    self.event_publisher.publish(AiEvent::ReflectionFailed {
                        workspace_id: workspace_id.clone(),
                        agent_id: agent_id.clone(),
                        session_key: session_key.clone(),
                        reason: e.to_string(),
                    });
                }
            }
            AiEvent::WorkspaceCreated { workspace_id } => {
                self.heartbeat_runner.start(workspace_id).await;
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                self.heartbeat_runner.stop(workspace_id).await;
            }
            AiEvent::AlarmResolved { .. } => {}
            AiEvent::HeartbeatPersistFailed { .. } => {}
            AiEvent::ReflectionFailed { .. } => {}
            AiEvent::ProposalCreated {
                workspace_id,
                proposal_id,
                tool_name,
            } => {
                info!(
                    workspace_id,
                    proposal_id, tool_name, "HITL proposal created — awaiting human approval"
                );
            }
            AiEvent::ProposalResolved {
                workspace_id,
                proposal_id,
                approved,
            } => {
                info!(workspace_id, proposal_id, approved, "HITL proposal resolved");
            }
        }
    }

    /// Retry heartbeat result persistence with exponential backoff.
    async fn retry_with_backoff(&self, workspace_id: &str, result: &crate::heartbeat::types::HeartbeatResult) {
        let result = result.clone();
        let ws_id = workspace_id.to_string();
        let task_repo = self.task_repo.clone();
        let event_publisher = self.event_publisher.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            let mut attempt: u32 = 0;
            let max_attempts: u32 = 5;
            let base_delay = Duration::from_secs(2);

            loop {
                tokio::time::sleep(base_delay * 2u32.pow(attempt)).await;
                match task_repo.insert_result(&ws_id, &result).await {
                    Ok(_) => {
                        debug!(ws_id, attempt, "Heartbeat result persisted on retry");
                        return;
                    }
                    Err(e) => {
                        attempt += 1;
                        if attempt >= max_attempts {
                            error!(
                                workspace_id = %ws_id,
                                attempts = attempt,
                                error = %e,
                                "Heartbeat persist exhausted retries, enqueuing to DLQ"
                            );
                            if let Some(ref dlq) = dlq {
                                let _ = dlq
                                    .enqueue(
                                        &ws_id,
                                        "HeartbeatCompleted",
                                        &serde_json::to_string(&result).unwrap_or_default(),
                                        &e.to_string(),
                                    )
                                    .await;
                            }
                            event_publisher.publish(AiEvent::HeartbeatPersistFailed {
                                workspace_id: ws_id,
                                reason: format!("Failed after {} attempts: {}", max_attempts, e),
                            });
                            return;
                        }
                        warn!(ws_id, attempt, error = %e, "Heartbeat persist retry");
                    }
                }
            }
        });
    }
}

#[async_trait]
impl tinyiothub_core::event::EventHandler for AiEventHandler {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        self.handle_ai_event(event).await;
        Ok(())
    }

    fn name(&self) -> &str {
        "AiEventHandler"
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Ai(_))
    }
}

fn extract_payload(content: &RichContent) -> Option<String> {
    content.elements().iter().find_map(|el| match el {
        ContentElement::Text { content, .. } => Some(content.clone()),
        _ => None,
    })
}
