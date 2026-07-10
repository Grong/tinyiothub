//! Cross-domain event callbacks -- dispatched by Orchestrator.
//!
//! AlarmCreated       --> HeartbeatRunner.signal()
//! HeartbeatCompleted --> HeartbeatTaskRepository.insert_result()
//! (Chat reflection is handled directly in chat/service.rs)
//! WorkspaceCreated    --> HeartbeatRunner.start()
//! WorkspaceDeleted    --> HeartbeatRunner.stop()

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    shutting_down: Arc<AtomicBool>,
}

impl AiEventHandler {
    pub fn new(
        heartbeat_runner: Arc<HeartbeatRunner>,
        task_repo: Arc<dyn HeartbeatTaskRepository>,
        memory_service: Arc<MemoryService>,
        event_publisher: Arc<AiEventPublisher>,
        dlq: Option<Arc<dyn DeadLetterQueue>>,
        shutting_down: Arc<AtomicBool>,
    ) -> Self {
        Self {
            heartbeat_runner,
            task_repo,
            memory_service,
            event_publisher,
            dlq,
            shutting_down,
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
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!("AiEventHandler is shutting down, skipping event");
            return;
        }

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
            AiEvent::WorkspaceCreated { workspace_id } => {
                self.heartbeat_runner.start(workspace_id).await;
            }
            AiEvent::WorkspaceDeleted { workspace_id } => {
                self.heartbeat_runner.stop(workspace_id).await;
            }
            // Self-referential events published by AiEventHandler itself —
            // these are intentionally no-ops to avoid processing loops.
            AiEvent::AlarmResolved { .. } => {}
            AiEvent::HeartbeatPersistFailed { .. } => {}
            AiEvent::ReflectionFailed { .. } => {}
            // ChatCompleted was previously handled here but reflection
            // now happens directly in chat/service.rs. Variant retained
            // for future EventBus-based reflection.
            AiEvent::ChatCompleted { .. } => {}
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
        let shutting_down = self.shutting_down.clone();

        tokio::spawn(async move {
            let mut attempt: u32 = 0;
            let max_attempts: u32 = 5;
            let base_delay = Duration::from_secs(2);

            loop {
                tokio::time::sleep(base_delay * 2u32.pow(attempt)).await;
                if shutting_down.load(Ordering::SeqCst) {
                    debug!(ws_id, "Shutting down, aborting retry");
                    return;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent};
    use tinyiothub_runtime::EventBus;

    use tinyiothub_core::event::EventHandler;

    use crate::heartbeat::repo::RepoError;
    use crate::heartbeat::types::{HeartbeatConfig, HeartbeatResult, HeartbeatStatus, HeartbeatTask};
    use crate::memory::provider::{LlmProvider, LlmResponse};
    use crate::memory::service::MemoryService;

    struct MockTaskRepo {
        insert_result_calls: Arc<Mutex<Vec<(String, HeartbeatResult)>>>,
    }

    impl MockTaskRepo {
        fn new() -> Self {
            Self {
                insert_result_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn insert_result_calls(&self) -> Arc<Mutex<Vec<(String, HeartbeatResult)>>> {
            Arc::clone(&self.insert_result_calls)
        }
    }

    #[async_trait::async_trait]
    impl crate::heartbeat::repo::HeartbeatTaskRepository for MockTaskRepo {
        async fn list_by_workspace(&self, _workspace_id: &str) -> Result<Vec<HeartbeatTask>, RepoError> {
            Ok(vec![])
        }

        async fn upsert(
            &self,
            _workspace_id: &str,
            _task: &HeartbeatTask,
            _expected_version: i64,
        ) -> Result<bool, RepoError> {
            Ok(true)
        }

        async fn insert(&self, _workspace_id: &str, _priority: &str, _text: &str) -> Result<HeartbeatTask, RepoError> {
            Err(RepoError::Database("mock".into()))
        }

        async fn set_paused(&self, _workspace_id: &str, _task_id: i64, _paused: bool) -> Result<(), RepoError> {
            Ok(())
        }

        async fn delete(&self, _workspace_id: &str, _task_id: i64) -> Result<(), RepoError> {
            Ok(())
        }

        async fn insert_result(&self, workspace_id: &str, result: &HeartbeatResult) -> Result<(), RepoError> {
            self.insert_result_calls
                .lock()
                .unwrap()
                .push((workspace_id.to_string(), result.clone()));
            Ok(())
        }
    }

    struct MockLlmProvider;

    #[async_trait::async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn chat(
            &self,
            _system: Option<&str>,
            _prompt: &str,
            _model: &str,
            _temperature: f32,
        ) -> anyhow::Result<LlmResponse> {
            anyhow::bail!("mock")
        }
    }

    struct MockMemoryStore;

    #[async_trait::async_trait]
    impl tinyiothub_core::memory::MemoryStore for MockMemoryStore {
        async fn put(
            &self,
            _input: tinyiothub_core::memory::MemoryInput,
        ) -> tinyiothub_core::error::Result<tinyiothub_core::memory::AgentMemory> {
            unimplemented!()
        }

        async fn get(&self, _id: &str) -> tinyiothub_core::error::Result<Option<tinyiothub_core::memory::AgentMemory>> {
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

        async fn set_pinned(&self, _id: &str, _pinned: bool) -> tinyiothub_core::error::Result<()> {
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
            Ok("mock_id".into())
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

    fn make_memory_service() -> Arc<MemoryService> {
        Arc::new(MemoryService::new(Arc::new(MockLlmProvider), Arc::new(MockMemoryStore)))
    }

    fn make_publisher() -> Arc<AiEventPublisher> {
        Arc::new(AiEventPublisher::new(Arc::new(EventBus::new())))
    }

    fn make_heartbeat_runner(
        task_repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository>,
    ) -> Arc<HeartbeatRunner> {
        Arc::new(HeartbeatRunner::new(
            task_repo,
            make_publisher(),
            HeartbeatConfig::default(),
        ))
    }

    /// Wrap an AiEvent inside a tinyiothub_core Event for handler dispatch.
    fn wrap_ai_event(ai_event: &AiEvent) -> Event {
        let payload = serde_json::to_string(ai_event).unwrap();
        let ai_event_type: tinyiothub_core::models::event::AiEventType = ai_event.into();
        Event::new(
            EventType::Ai(ai_event_type),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("AiEvent".to_string(), payload),
        )
        .expect("Failed to create test event")
    }

    #[tokio::test]
    async fn test_handler_construction() {
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(runner, repo, memory, publisher, None, Arc::new(AtomicBool::new(false)));
        assert_eq!(handler.name(), "AiEventHandler");
    }

    #[tokio::test]
    async fn test_should_handle_filters_ai_events() {
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(runner, repo, memory, publisher, None, Arc::new(AtomicBool::new(false)));

        let ai_event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_1".into(),
        });
        assert!(handler.should_handle(&ai_event));

        // System event should not be handled
        let system_event = Event::new(
            EventType::System(tinyiothub_core::models::event::SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "data".to_string()),
        )
        .expect("Failed to create system event");
        assert!(!handler.should_handle(&system_event));
    }

    #[tokio::test]
    async fn test_heartbeat_completed_inserts_result() {
        let repo = Arc::new(MockTaskRepo::new());
        let insert_calls = repo.insert_result_calls();
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = repo;
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            Arc::clone(&repo),
            memory,
            publisher,
            None,
            Arc::new(AtomicBool::new(false)),
        );

        let result = HeartbeatResult {
            workspace_id: "ws_test".to_string(),
            status: HeartbeatStatus::Complete,
            summary: "All good".to_string(),
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        };

        let ai_event = AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result: result.clone(),
        };

        let event = wrap_ai_event(&ai_event);
        handler.handle_ai_event(&event).await;

        let calls = insert_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "ws_test");
        assert_eq!(calls[0].1.workspace_id, "ws_test");
        assert_eq!(calls[0].1.status, HeartbeatStatus::Complete);
    }

    #[tokio::test]
    async fn test_alarm_created_non_critical_no_signal() {
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(runner, repo, memory, publisher, None, Arc::new(AtomicBool::new(false)));

        // Non-critical alarm should not trigger heartbeat signal
        let alarm = AiEvent::AlarmCreated(crate::alarm::types::AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws_1".into(),
            device_id: "d1".into(),
            alarm_type: "high_temp".into(),
            severity: "warning".into(),
            message: "Temperature is high".into(),
            rule_id: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        });

        let event = wrap_ai_event(&alarm);
        handler.handle_ai_event(&event).await;
        // No assertion needed — just verifying no panic for non-critical alarms
    }

    #[tokio::test]
    async fn test_workspace_created_and_deleted_no_panic() {
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(runner, repo, memory, publisher, None, Arc::new(AtomicBool::new(false)));

        // WorkspaceCreated (no tasks loaded → loop won't start, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceCreated {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;

        // WorkspaceDeleted (no loop running → no-op, but no panic)
        let event = wrap_ai_event(&AiEvent::WorkspaceDeleted {
            workspace_id: "ws_new".into(),
        });
        handler.handle_ai_event(&event).await;
    }

    #[tokio::test]
    async fn test_self_referential_events_are_noop() {
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(MockTaskRepo::new());
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(runner, repo, memory, publisher, None, Arc::new(AtomicBool::new(false)));

        // Self-referential events should not panic
        for event_variant in [
            AiEvent::AlarmResolved {
                alarm_id: "a1".into(),
                device_id: "d1".into(),
                rule_id: None,
            },
            AiEvent::HeartbeatPersistFailed {
                workspace_id: "ws_1".into(),
                reason: "test".into(),
            },
            AiEvent::ReflectionFailed {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                reason: "test".into(),
            },
            AiEvent::ChatCompleted {
                workspace_id: "ws_1".into(),
                agent_id: "ag1".into(),
                session_key: "sk1".into(),
                model: "gpt-4".into(),
                messages: vec![],
            },
        ] {
            let event = wrap_ai_event(&event_variant);
            handler.handle_ai_event(&event).await;
        }
    }

    #[tokio::test]
    async fn test_shutting_down_skips_handling() {
        let repo = Arc::new(MockTaskRepo::new());
        let insert_calls = repo.insert_result_calls();
        let repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository> = repo;
        let runner = make_heartbeat_runner(Arc::clone(&repo));
        let publisher = make_publisher();
        let memory = make_memory_service();

        let handler = AiEventHandler::new(
            runner,
            Arc::clone(&repo),
            memory,
            publisher,
            None,
            Arc::new(AtomicBool::new(true)), // shutting_down = true
        );

        let result = HeartbeatResult {
            workspace_id: "ws_test".to_string(),
            status: HeartbeatStatus::Complete,
            summary: "All good".to_string(),
            executed_actions: vec![],
            proposals: vec![],
            error: None,
        };

        let event = wrap_ai_event(&AiEvent::HeartbeatCompleted {
            workspace_id: "ws_test".to_string(),
            result,
        });
        handler.handle_ai_event(&event).await;

        // insert_result should NOT have been called because shutting_down is true
        let calls = insert_calls.lock().unwrap();
        assert!(calls.is_empty());
    }

    #[tokio::test]
    async fn test_extract_payload_from_text() {
        let content = RichContent::new_text("Test event".to_string(), r#"{"key":"value"}"#.to_string());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(r#"{"key":"value"}"#.to_string()));
    }

    #[tokio::test]
    async fn test_extract_payload_empty_content() {
        let content = RichContent::new_text("Test".to_string(), String::new());
        let extracted = extract_payload(&content);
        assert_eq!(extracted, Some(String::new()));
    }
}
