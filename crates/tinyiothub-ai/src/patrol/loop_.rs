//! Patrol loop — per-workspace async loop driving periodic AI inspection.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::types::{HeartbeatConfig, HeartbeatTask, TrustConfig, WakeSignal};
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;

/// Maximum number of consecutive LLM failures before pausing the loop.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Main patrol loop for a single workspace.
///
/// Sleeps for the configured interval, then iterates through heartbeat tasks,
/// calling the LLM for each. Publishes `PatrolCompleted` events instead of
/// directly calling `ActionRepo::insert()` — an ActionRepo subscriber handles persistence.
#[allow(clippy::too_many_arguments)]
pub async fn patrol_loop(
    workspace_id: String,
    tasks: Vec<HeartbeatTask>,
    trust_config: TrustConfig,
    agent_pool: Option<Arc<dyn crate::agent::pool::AgentPoolLike>>,
    _task_repo: Arc<dyn crate::patrol::repo::HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    config: HeartbeatConfig,
    mut wake_rx: mpsc::UnboundedReceiver<WakeSignal>,
    cancel_rx: oneshot::Receiver<()>,
) {
    let agent_pool = match agent_pool {
        Some(p) => p,
        None => {
            error!(workspace_id, "AgentPool not set, patrol loop cannot start");
            return;
        }
    };

    let interval = Duration::from_secs((config.interval_minutes as u64) * 60);
    let mut consecutive_failures: u32 = 0;

    tokio::pin! {
        let cancel = cancel_rx;
    }

    loop {
        // Run a patrol tick
        let active_tasks: Vec<&HeartbeatTask> = tasks.iter().filter(|t| !t.paused).collect();
        if !active_tasks.is_empty() {
            match run_patrol_tick(
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
                    error!(workspace_id, error = %e, consecutive_failures, "Patrol tick failed");
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        warn!(
                            workspace_id,
                            consecutive_failures,
                            "Too many consecutive failures, publishing error report and pausing"
                        );
                        event_publisher.publish(AiEvent::PatrolCompleted {
                            workspace_id: workspace_id.clone(),
                            report: crate::patrol::types::PatrolReport {
                                workspace_id: workspace_id.clone(),
                                status: crate::patrol::types::PatrolStatus::Error,
                                summary: format!("{} consecutive failures", consecutive_failures),
                                executed_actions: vec![],
                                pending_proposals: vec![],
                                error: Some(e.to_string()),
                            },
                        });
                    }
                }
            }
        }

        // Wait for next interval, wake signal, or cancel
        tokio::select! {
            _ = &mut cancel => {
                info!(workspace_id, "Patrol loop cancelled");
                return;
            }
            signal = wake_rx.recv() => {
                if let Some(s) = signal {
                    debug!(
                        workspace_id,
                        priority = %s.priority.label(),
                        reason = %s.reason,
                        "Patrol loop woken by signal"
                    );
                }
                // Immediate tick — loop continues
            }
            _ = tokio::time::sleep(interval) => {
                // Normal interval tick
            }
        }
    }
}

async fn run_patrol_tick(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
    agent_pool: &Arc<dyn crate::agent::pool::AgentPoolLike>,
    event_publisher: &AiEventPublisher,
) -> Result<(), String> {
    // Build prompt from tasks
    let prompt = build_patrol_prompt(workspace_id, tasks, trust_config);

    // Call LLM with timeout
    let raw_response = tokio::time::timeout(
        Duration::from_secs(180),
        agent_pool.send_message(workspace_id, &prompt),
    )
    .await
    .map_err(|_| "LLM call timed out after 180s".to_string())?
    .map_err(|e| format!("LLM call failed: {}", e))?;

    // Parse the response
    let report = super::report::parse_healing_report(&raw_response, workspace_id);

    // Publish PatrolCompleted — ActionRepo subscriber handles persistence
    event_publisher.publish(AiEvent::PatrolCompleted {
        workspace_id: workspace_id.to_string(),
        report,
    });

    Ok(())
}

fn build_patrol_prompt(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
) -> String {
    let tasks_text: String = tasks
        .iter()
        .map(|t| format!("- [{}] {}", t.priority, t.text))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are an IoT patrol agent for workspace {ws_id}.\n\
         Trust level: {trust:?}\n\
         Max auto-actions per tick: {max}\n\n\
         ## Patrol Tasks:\n{tasks}\n\n\
         Execute each task. Output a JSON report:\n\
         ```json\n\
         {{\n  \"status\": \"complete|partial|error\",\n  \
         \"summary\": \"...\",\n  \
         \"executed_actions\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"success\": true, \"details\": \"...\"}}],\n  \
         \"pending_proposals\": [{{\"tool_name\": \"...\", \"device_id\": \"...\", \"proposed_action\": \"...\", \"rationale\": \"...\"}}],\n  \
         \"error\": null\n}}\n```",
        ws_id = workspace_id,
        trust = trust_config.trust_level,
        max = trust_config.max_auto_actions_per_tick,
        tasks = tasks_text
    )
}
