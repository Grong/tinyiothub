//! Heartbeat loop — per-workspace async loop driving periodic AI-powered checks.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::types::{HeartbeatConfig, HeartbeatSignal, HeartbeatStatus, HeartbeatTask};
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::tool::trust::TrustConfig;

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Main heartbeat loop for a single workspace.
#[allow(clippy::too_many_arguments)]
pub async fn heartbeat_loop(
    workspace_id: String,
    tasks: Vec<HeartbeatTask>,
    trust_config: TrustConfig,
    agent_pool: Option<Arc<dyn crate::agent::pool::AgentPoolLike>>,
    _task_repo: Arc<dyn crate::heartbeat::repo::HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    config: HeartbeatConfig,
    mut signal_rx: mpsc::UnboundedReceiver<HeartbeatSignal>,
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
            let active_tasks: Vec<&HeartbeatTask> = tasks.iter().filter(|t| !t.paused).collect();
            if !active_tasks.is_empty() {
                match run_heartbeat_tick(
                    &workspace_id,
                    &active_tasks,
                    &trust_config,
                    &agent_pool,
                    &event_publisher,
                )
                .await
                {
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
                if let Some(s) = signal {
                    debug!(
                        workspace_id,
                        priority = %s.priority.label(),
                        reason = %s.reason,
                        "Heartbeat loop woken by signal"
                    );
                    if paused {
                        info!(workspace_id, "Heartbeat loop resumed after pause");
                        paused = false;
                        consecutive_failures = 0;
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

    let raw_response = tokio::time::timeout(Duration::from_secs(180), agent_pool.send_message(workspace_id, &prompt))
        .await
        .map_err(|_| "LLM call timed out after 180s".to_string())?
        .map_err(|e| format!("LLM call failed: {}", e))?;

    let result = super::report::parse_healing_report(&raw_response, workspace_id);

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
         \"proposals\": [{{\"id\": \"...\", \"tool_name\": \"...\", \"device_id\": \"...\", \"summary\": \"...\", \"reason\": \"...\", \"risk\": \"low|medium|high\"}}],\n  \
         \"error\": null\n}}\n```",
        ws_id = workspace_id,
        trust = trust_config.trust_level,
        max = trust_config.max_auto_actions_per_tick,
        tasks = tasks_text
    )
}
