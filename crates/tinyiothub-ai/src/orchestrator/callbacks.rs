//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated          --> PatrolManager.wake()
//! PatrolCompleted       --> ActionRepo.insert()
//! ChatCompleted         --> MemoryService.reflect()
//! WorkspaceCreated      --> PatrolManager.start()
//! WorkspaceDeleted      --> PatrolManager.stop()

use std::sync::Arc;
use std::time::Duration;

use tinyiothub_core::models::event::{ContentElement, Event, EventType};
use tracing::{debug, error, info, warn};

use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::memory::service::MemoryService;
use crate::patrol::manager::PatrolManager;
use crate::patrol::repo::ActionRepository;
use crate::patrol::types::WakePriority;

/// Cross-domain callback handler.
///
/// Registered on the shared EventBus by Orchestrator::start().
/// Dispatches AiEvent variants to the appropriate domain service.
pub struct AiEventHandler {
    patrol_manager: Arc<PatrolManager>,
    action_repo: Arc<dyn ActionRepository>,
    memory_service: Arc<MemoryService>,
    #[allow(dead_code)]
    event_publisher: Arc<AiEventPublisher>,
}

impl AiEventHandler {
    pub fn new(
        patrol_manager: Arc<PatrolManager>,
        action_repo: Arc<dyn ActionRepository>,
        memory_service: Arc<MemoryService>,
        event_publisher: Arc<AiEventPublisher>,
    ) -> Self {
        Self {
            patrol_manager,
            action_repo,
            memory_service,
            event_publisher,
        }
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
                    self.patrol_manager.wake(crate::patrol::types::WakeSignal {
                        workspace_id: alarm.workspace_id.clone(),
                        reason: format!("Alarm: {}", alarm.message),
                        context: format!(
                            "device_id={}, alarm_type={}",
                            alarm.device_id, alarm.alarm_type
                        ),
                        priority: if severity == "critical" {
                            WakePriority::Critical
                        } else {
                            WakePriority::High
                        },
                        device_id: Some(alarm.device_id.clone()),
                        alarm_type: Some(alarm.alarm_type.clone()),
                        rule_id: alarm.rule_id.clone(),
                    });
                }
            }
            AiEvent::PatrolCompleted {
                workspace_id,
                report,
            } => {
                match self
                    .action_repo
                    .insert_patrol_actions(workspace_id, report)
                    .await
                {
                    Ok(_) => info!(workspace_id, "Patrol actions persisted"),
                    Err(e) => {
                        error!(
                            workspace_id,
                            error = %e,
                            "Failed to persist patrol actions -- starting retry"
                        );
                        self.retry_with_backoff(workspace_id, report).await;
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
                    .reflect_conversation_turn(
                        workspace_id,
                        agent_id,
                        session_key,
                        model,
                        messages,
                    )
                    .await
                {
                    warn!(
                        workspace_id,
                        agent_id,
                        error = %e,
                        "Memory reflection failed"
                    );
                }
            }
            AiEvent::WorkspaceCreated { workspace_id } => {
                self.patrol_manager.start(workspace_id).await;
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                self.patrol_manager.stop(workspace_id).await;
            }
            AiEvent::AlarmResolved { .. } => {
                // No action needed for alarm resolution
            }
        }
    }

    /// Retry action persistence with exponential backoff in a background task.
    async fn retry_with_backoff(
        &self,
        workspace_id: &str,
        report: &crate::patrol::types::PatrolReport,
    ) {
        let report = report.clone();
        let ws_id = workspace_id.to_string();
        let action_repo = self.action_repo.clone();

        tokio::spawn(async move {
            let max_attempts = 5;
            for attempt in 1..=max_attempts {
                // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms
                let delay_ms = 100u64 * 2u64.pow(attempt as u32 - 1);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                match action_repo.insert_patrol_actions(&ws_id, &report).await {
                    Ok(_) => {
                        info!(
                            workspace_id = %ws_id,
                            attempt,
                            "Patrol actions persisted after retry"
                        );
                        return;
                    }
                    Err(e) if attempt >= 3 => {
                        error!(
                            workspace_id = %ws_id,
                            attempt,
                            error = %e,
                            "Action persistence failed after 3+ retries -- giving up"
                        );
                        return;
                    }
                    Err(e) => {
                        warn!(
                            workspace_id = %ws_id,
                            attempt,
                            error = %e,
                            "Retrying action persistence"
                        );
                    }
                }
            }
        });
    }
}

/// Extract the text payload from the first Text element of RichContent.
fn extract_payload(content: &tinyiothub_core::models::event::RichContent) -> Option<String> {
    content.elements().first().and_then(|el| {
        if let ContentElement::Text { content, .. } = el {
            Some(content.clone())
        } else {
            None
        }
    })
}

#[async_trait::async_trait]
impl tinyiothub_core::event::EventHandler for AiEventHandler {
    fn name(&self) -> &str {
        "AiEventHandler"
    }

    fn priority(&self) -> u8 {
        10
    }

    fn should_handle(&self, event: &Event) -> bool {
        matches!(event.event_type(), EventType::Ai(_))
    }

    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        self.handle_ai_event(event).await;
        Ok(())
    }
}
