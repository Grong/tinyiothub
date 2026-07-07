//! Heartbeat loop — per-workspace async loop driving periodic AI-powered checks.
//!
//! Now routes through the 6-stage harness pipeline instead of raw LLM calls.
//! The harness provides PreToolUse checks, PostToolUse verification, lie detection,
//! and structured reporting.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::types::{HeartbeatConfig, HeartbeatTask, LoopSignal};
use crate::agent::pool::AgentPoolLike;
use crate::event::bus::AiEventPublisher;
use crate::event::types::AiEvent;
use crate::harness::{
    HarnessSignal, LieCounter, LoopContext, LoopReport, SignalPayload, SignalSource, MAX_HISTORY_TICKS,
};
use crate::harness::orchestrator::run_harness_tick;
use crate::heartbeat::report::build_heartbeat_result_from_report;
use crate::heartbeat::types::SignalPriority;
use crate::policy::PolicyEngine;
use crate::tool::trust::TrustConfig;

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Main heartbeat loop for a single workspace.
#[allow(clippy::too_many_arguments)]
pub async fn heartbeat_loop(
    workspace_id: String,
    tasks: Arc<RwLock<Vec<HeartbeatTask>>>,
    trust_config: Arc<RwLock<TrustConfig>>,
    agent_pool: Option<Arc<dyn AgentPoolLike>>,
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

    let policy_engine: Arc<dyn PolicyEngine> =
        Arc::new(crate::policy::RateLimitingPolicyEngine::new(10));

    let interval = Duration::from_secs((config.interval_minutes as u64) * 60);
    let mut consecutive_failures: u32 = 0;
    let mut lie_counter = LieCounter::new(3);
    let mut history: VecDeque<LoopReport> = VecDeque::new();
    let mut paused = false;

    tokio::pin! {
        let cancel = cancel_rx;
    }

    loop {
        if !paused && !lie_counter.is_degraded() {
            let active_tasks: Vec<HeartbeatTask> =
                tasks.read().await.iter().filter(|t| !t.paused).cloned().collect();
            let trust = trust_config.read().await.clone();

            if !active_tasks.is_empty() {
                match run_harness_heartbeat_tick(
                    &workspace_id,
                    &active_tasks,
                    &trust,
                    &agent_pool,
                    &policy_engine,
                    &event_publisher,
                    &mut lie_counter,
                    &mut history,
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
                                consecutive_failures,
                                "Too many consecutive failures, pausing heartbeat loop"
                            );
                            paused = true;
                            event_publisher.publish(AiEvent::HeartbeatCompleted {
                                workspace_id: workspace_id.clone(),
                                result: crate::heartbeat::types::HeartbeatResult {
                                    workspace_id: workspace_id.clone(),
                                    status: crate::heartbeat::types::HeartbeatStatus::Error,
                                    summary: format!(
                                        "Heartbeat loop paused after {} consecutive failures",
                                        consecutive_failures
                                    ),
                                    executed_actions: vec![],
                                    proposals: vec![],
                                    error: Some(e),
                                    pipeline_verdict: String::new(),
                                    lie_detected: false,
                                    tool_call_count: 0,
                                    duration_ms: 0,
                                },
                            });
                        }
                    }
                }
            }
        } else if lie_counter.is_degraded() {
            debug!(
                workspace_id,
                consecutive = lie_counter.consecutive_ticks,
                "Agent is degraded, skipping tick"
            );
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
                        // Handle external signal immediately
                        let active_tasks: Vec<HeartbeatTask> =
                            tasks.read().await.iter().filter(|t| !t.paused).cloned().collect();
                        let trust = trust_config.read().await.clone();

                        if !active_tasks.is_empty() && !lie_counter.is_degraded() {
                            let signal = HarnessSignal {
                                workspace_id: workspace_id.clone(),
                                source: SignalSource::Event,
                                payload: SignalPayload::Alarm(s),
                                priority: SignalPriority::Critical,
                            };

                            let context = LoopContext {
                                workspace_id: workspace_id.clone(),
                                agent_id: format!("agent:{}", workspace_id),
                                trust_config: trust,
                                tasks: active_tasks,
                                history: history.clone(),
                                system_prompt: String::new(),
                            };

                            let report = run_harness_tick(
                                &signal,
                                &context,
                                &agent_pool,
                                &policy_engine,
                                &event_publisher,
                                &mut lie_counter,
                            ).await;

                            // Cap history
                            history.push_back(report.clone());
                            if history.len() > MAX_HISTORY_TICKS {
                                history.pop_front();
                            }

                            // Backward compat: publish HeartbeatCompleted
                            let hb_result = build_heartbeat_result_from_report(&report);
                            let _ = task_repo.insert_result(&workspace_id, &hb_result).await;
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

/// Run a single heartbeat tick through the harness pipeline.
async fn run_harness_heartbeat_tick(
    workspace_id: &str,
    tasks: &[HeartbeatTask],
    trust_config: &TrustConfig,
    agent_pool: &Arc<dyn AgentPoolLike>,
    policy_engine: &Arc<dyn PolicyEngine>,
    event_publisher: &AiEventPublisher,
    lie_counter: &mut LieCounter,
    history: &mut VecDeque<LoopReport>,
) -> Result<(), String> {
    let signal = HarnessSignal {
        workspace_id: workspace_id.to_string(),
        source: SignalSource::Timer,
        payload: SignalPayload::Timer,
        priority: SignalPriority::Normal,
    };

    let context = LoopContext {
        workspace_id: workspace_id.to_string(),
        agent_id: format!("agent:{}", workspace_id),
        trust_config: trust_config.clone(),
        tasks: tasks.to_vec(),
        history: history.clone(),
        system_prompt: String::new(),
    };

    let report = run_harness_tick(
        &signal,
        &context,
        agent_pool,
        policy_engine,
        event_publisher,
        lie_counter,
    )
    .await;

    // Cap history window
    history.push_back(report.clone());
    if history.len() > MAX_HISTORY_TICKS {
        history.pop_front();
    }

    // Backward compat: also publish HeartbeatCompleted for DB persistence
    let heartbeat_result = build_heartbeat_result_from_report(&report);
    event_publisher.publish(AiEvent::HeartbeatCompleted {
        workspace_id: workspace_id.to_string(),
        result: heartbeat_result,
    });

    Ok(())
}
