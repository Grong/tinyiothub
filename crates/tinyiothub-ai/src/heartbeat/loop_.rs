//! Heartbeat loop — per-workspace async loop driving periodic AI-powered checks.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::types::{HeartbeatConfig, HeartbeatStatus, HeartbeatTask, LoopSignal};
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::tool::trust::TrustConfig;

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Main heartbeat loop for a single workspace.
#[allow(clippy::too_many_arguments)]
pub async fn heartbeat_loop(
    workspace_id: String,
    tasks: Arc<RwLock<Vec<HeartbeatTask>>>,
    trust_config: Arc<RwLock<TrustConfig>>,
    agent_pool: Option<Arc<dyn crate::agent::pool::AgentPoolLike>>,
    task_repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    config: HeartbeatConfig,
    mut signal_rx: mpsc::UnboundedReceiver<LoopSignal>,
    cancel_rx: oneshot::Receiver<()>,
) {
    let agent_pool = match agent_pool {
        Some(p) => p,
        None => {
            error!(workspace_id, "AgentPool not set, heartbeat loop cannot start");
            return;
        }
    };

    let interval = Duration::from_secs((config.interval_minutes as u64) * 60);
    let mut consecutive_failures: u32 = 0;
    let mut paused = false;

    tokio::pin! {
        let cancel = cancel_rx;
    }

    loop {
        if !paused {
            let active_tasks: Vec<HeartbeatTask> = tasks.read().await.iter().filter(|t| !t.paused).cloned().collect();
            let trust = trust_config.read().await.clone();

            if !active_tasks.is_empty() {
                let task_refs: Vec<&HeartbeatTask> = active_tasks.iter().collect();
                match run_heartbeat_tick(&workspace_id, &task_refs, &trust, &agent_pool, &event_publisher).await {
                    Ok(_) => consecutive_failures = 0,
                    Err(e) => {
                        consecutive_failures += 1;
                        error!(workspace_id, error = %e, consecutive_failures, "Heartbeat tick failed");
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            warn!(
                                workspace_id,
                                consecutive_failures, "Too many consecutive failures, pausing heartbeat loop"
                            );
                            paused = true;
                            event_publisher.publish(AiEvent::HeartbeatCompleted {
                                workspace_id: workspace_id.clone(),
                                result: crate::heartbeat::types::HeartbeatResult {
                                    workspace_id: workspace_id.clone(),
                                    status: HeartbeatStatus::Error,
                                    summary: format!(
                                        "Heartbeat loop paused after {} consecutive failures",
                                        consecutive_failures
                                    ),
                                    task_count: 0,
                                    executed_actions: vec![],
                                    proposals: vec![],
                                    error: Some(e.to_string()),
                                },
                            });
                        }
                    }
                }
            }
        }

        tokio::select! {
            _ = &mut cancel => {
                info!(workspace_id, "Heartbeat loop cancelled");
                return;
            }
            signal = signal_rx.recv() => {
                match signal {
                    Some(LoopSignal::External(s)) => {
                        debug!(
                            workspace_id,
                            priority = %s.priority.label(),
                            reason = %s.reason,
                            "Heartbeat loop woken by external signal"
                        );
                        if paused {
                            info!(workspace_id, "Heartbeat loop resumed after pause");
                            paused = false;
                            consecutive_failures = 0;
                        }
                    }
                    Some(LoopSignal::ReloadTasks) => {
                        info!(workspace_id, "Heartbeat loop reloading tasks");
                        match task_repo.list_by_workspace(&workspace_id).await {
                            Ok(new_tasks) => {
                                let count = new_tasks.len();
                                *tasks.write().await = new_tasks;
                                info!(workspace_id, count, "Heartbeat tasks reloaded");
                            }
                            Err(e) => {
                                warn!(workspace_id, error = %e, "Failed to reload heartbeat tasks");
                            }
                        }
                    }
                    Some(LoopSignal::ReloadConfig) => {
                        // Config is read from the shared Arc<RwLock<TrustConfig>>
                        // on each tick, so this signal just forces an immediate tick.
                        info!(workspace_id, "Heartbeat loop config refresh acknowledged");
                    }
                    None => {
                        debug!(workspace_id, "Signal channel closed, exiting heartbeat loop");
                        return;
                    }
                }
            }
            _ = tokio::time::sleep(interval), if !paused => {}
        }
    }
}

async fn run_heartbeat_tick(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
    agent_pool: &Arc<dyn crate::agent::pool::AgentPoolLike>,
    event_publisher: &AiEventPublisher,
) -> Result<(), String> {
    let prompt = build_heartbeat_prompt(workspace_id, tasks, trust_config);

    let output = tokio::time::timeout(Duration::from_secs(180), agent_pool.send_message(workspace_id, &prompt))
        .await
        .map_err(|_| "LLM call timed out after 180s".to_string())?
        .map_err(|e| format!("LLM call failed: {}", e))?;

    let mut result = super::report::parse_healing_report(&output.text, workspace_id);
    result.task_count = tasks.len() as u32;
    // Audit-trail reconciliation: executed_actions come from the framework's
    // actual tool-call records, not the LLM's self-report (which can fabricate
    // actions it never performed).
    result.executed_actions = output
        .tool_calls
        .into_iter()
        .map(|c| crate::heartbeat::types::ExecutedAction {
            tool_name: c.tool_name,
            device_id: c.device_id,
            success: c.success,
            details: c.details,
        })
        .collect();

    event_publisher.publish(AiEvent::HeartbeatCompleted {
        workspace_id: workspace_id.to_string(),
        result,
    });

    Ok(())
}

fn build_heartbeat_prompt(workspace_id: &str, tasks: &[&HeartbeatTask], trust_config: &TrustConfig) -> String {
    let tasks_text: String = tasks
        .iter()
        .map(|t| format!("- [{}] {}", t.priority, crate::memory::reflect::sanitize_input(&t.text)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are an IoT heartbeat agent for workspace {ws_id}.\n\
         Trust level: {trust:?}\n\
         Max auto-actions per tick: {max}\n\n\
         ## Tasks:\n{tasks}\n\n\
         Execute each task. Output a JSON report:\n\
         ```json\n\
         {{\n  \"status\": \"complete|partial|error\",\n  \
         \"summary\": \"...\",\n  \
         \"executed_actions\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"success\": true, \"details\": \"...\"}}],\n  \
         \"proposals\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"summary\": \"...\", \"reason\": \"...\", \"risk\": \"low|medium|high\", \"parameters\": {{...}}}}],\n  \
         \"error\": null\n}}\n```",
        ws_id = workspace_id,
        trust = trust_config.trust_level,
        max = trust_config.max_auto_actions_per_tick,
        tasks = tasks_text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::pool::{AgentPoolLike, AgentRunOutput, ToolCallRecord};
    use crate::heartbeat::types::HeartbeatTask;
    use std::sync::Mutex;

    struct MockPool {
        output: AgentRunOutput,
    }

    #[async_trait::async_trait]
    impl AgentPoolLike for MockPool {
        async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
            Ok("agent".into())
        }
        async fn send_message(&self, _workspace_id: &str, _prompt: &str) -> anyhow::Result<AgentRunOutput> {
            Ok(self.output.clone())
        }
        async fn shutdown(&self) {}
        fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
        fn cleanup_idle(&self) -> usize {
            0
        }
    }

    struct RecordingHandler {
        seen: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl tinyiothub_core::event::EventHandler for RecordingHandler {
        async fn handle(
            &self,
            event: &tinyiothub_core::models::event::Event,
        ) -> tinyiothub_core::error::Result<()> {
            self.seen.lock().unwrap().push(event.content().to_plain_text());
            Ok(())
        }
        fn name(&self) -> &str {
            "recording"
        }
        fn should_handle(&self, _event: &tinyiothub_core::models::event::Event) -> bool {
            true
        }
    }

    fn sample_task() -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws".into(),
            priority: "high".into(),
            text: "check devices".into(),
            paused: false,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn prompt_asks_proposals_for_parameters() {
        // Without parameters the approve-and-execute flow has nothing to run.
        let task = sample_task();
        let prompt = build_heartbeat_prompt("ws", &[&task], &TrustConfig::default());
        assert!(
            prompt.contains("\"parameters\""),
            "proposal schema in the prompt must request tool parameters"
        );
    }

    #[tokio::test]
    async fn executed_actions_come_from_actual_tool_calls_not_llm_report() {
        let bus = Arc::new(tinyiothub_runtime::EventBus::new());
        let seen = Arc::new(RecordingHandler {
            seen: Mutex::new(Vec::new()),
        });
        bus.register_handler(seen.clone());
        let publisher = AiEventPublisher::new(bus);

        // The LLM claims it ran "fake_tool"; the framework only recorded "real_tool".
        let pool: Arc<dyn AgentPoolLike> = Arc::new(MockPool {
            output: AgentRunOutput {
                text: r#"{"status":"complete","summary":"done","executed_actions":[{"tool_name":"fake_tool","device_id":"d_fake","success":true,"details":"LLM self-report"}],"proposals":[]}"#.into(),
                tool_calls: vec![ToolCallRecord {
                    tool_name: "real_tool".into(),
                    device_id: Some("d_real".into()),
                    success: true,
                    details: "actually executed".into(),
                }],
            },
        });

        let task = sample_task();
        run_heartbeat_tick("ws", &[&task], &TrustConfig::default(), &pool, &publisher)
            .await
            .unwrap();
        publisher.shutdown().await;

        let seen = seen.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].contains("real_tool"), "actual tool call must be recorded");
        assert!(seen[0].contains("d_real"));
        assert!(
            !seen[0].contains("fake_tool"),
            "LLM self-reported action must be discarded"
        );
    }
}
