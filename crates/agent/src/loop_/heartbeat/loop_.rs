//! Heartbeat loop — per-workspace async loop driving periodic AI-powered checks.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::metrics::Metrics;
use super::types::{HeartbeatConfig, HeartbeatStatus, HeartbeatTask, LoopSignal};
use crate::loop_::event::bus::AiEventPublisher;
use crate::loop_::event::types::AiEvent;
use tinyiothub_skills::trust::TrustConfig;

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Single tick-level bound for one heartbeat run. AgentPoolLike implementations
/// must not impose a shorter inner timeout on the same call — the inner one
/// would always fire first and leave this bound unreachable.
const TICK_TIMEOUT: Duration = Duration::from_secs(180);

/// Main heartbeat loop for a single workspace.
#[allow(clippy::too_many_arguments)]
pub async fn heartbeat_loop(
    workspace_id: String,
    tasks: Arc<RwLock<Vec<HeartbeatTask>>>,
    trust_config: Arc<RwLock<TrustConfig>>,
    agent_pool: Option<Arc<dyn crate::loop_::agent::pool::AgentPoolLike>>,
    task_repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository>,
    event_publisher: Arc<AiEventPublisher>,
    config: HeartbeatConfig,
    mut signal_rx: mpsc::Receiver<LoopSignal>,
    cancel_rx: oneshot::Receiver<()>,
    metrics: Arc<Metrics>,
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
                match run_heartbeat_tick(
                    &workspace_id,
                    &task_refs,
                    &trust,
                    &agent_pool,
                    &event_publisher,
                    &metrics,
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
                            metrics.paused_loops.fetch_add(1, Ordering::Relaxed);
                            event_publisher.publish(AiEvent::HeartbeatCompleted {
                                workspace_id: workspace_id.clone(),
                                result: crate::loop_::heartbeat::types::HeartbeatResult {
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
                if paused {
                    metrics.paused_loops.fetch_sub(1, Ordering::Relaxed);
                }
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
                            metrics.paused_loops.fetch_sub(1, Ordering::Relaxed);
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
                        if paused {
                            metrics.paused_loops.fetch_sub(1, Ordering::Relaxed);
                        }
                        return;
                    }
                }
            }
            _ = tokio::time::sleep(interval) => {
                // Cooldown: a paused loop retries after one interval instead of
                // waiting forever for an external signal that may never come.
                if paused {
                    info!(workspace_id, "Heartbeat loop auto-resuming after cooldown");
                    paused = false;
                    metrics.paused_loops.fetch_sub(1, Ordering::Relaxed);
                    consecutive_failures = 0;
                }
            }
        }
    }
}

async fn run_heartbeat_tick(
    workspace_id: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
    agent_pool: &Arc<dyn crate::loop_::agent::pool::AgentPoolLike>,
    event_publisher: &AiEventPublisher,
    metrics: &Metrics,
) -> Result<(), String> {
    let prompt = build_heartbeat_prompt(workspace_id, tasks, trust_config);

    let started = std::time::Instant::now();
    let output = match tokio::time::timeout(TICK_TIMEOUT, agent_pool.send_message(workspace_id, &prompt)).await {
        Ok(Ok(output)) => {
            metrics.record_llm_call(started.elapsed().as_millis() as u64, true);
            output
        }
        Ok(Err(e)) => {
            metrics.record_llm_call(started.elapsed().as_millis() as u64, false);
            return Err(format!("LLM call failed: {}", e));
        }
        Err(_) => {
            metrics.record_llm_call(started.elapsed().as_millis() as u64, false);
            return Err("LLM call timed out after 180s".to_string());
        }
    };

    let mut result = super::report::parse_healing_report(&output.text, workspace_id);
    result.task_count = tasks.len() as u32;
    // Audit-trail reconciliation: executed_actions come from the framework's
    // actual tool-call records, not the LLM's self-report (which can fabricate
    // actions it never performed).
    result.executed_actions = output
        .tool_calls
        .into_iter()
        .map(|c| crate::loop_::heartbeat::types::ExecutedAction {
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
        .map(|t| {
            format!(
                "- [{}] {}",
                t.priority,
                tinyiothub_memory::reflect::sanitize_input(&t.text)
            )
        })
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
    use crate::loop_::agent::pool::{AgentPoolLike, AgentRunOutput, ToolCallRecord};
    use crate::loop_::heartbeat::types::HeartbeatTask;
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
        async fn handle(&self, event: &tinyiothub_core::models::event::Event) -> tinyiothub_core::error::Result<()> {
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
        let metrics = crate::loop_::heartbeat::metrics::Metrics::new();

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
        run_heartbeat_tick("ws", &[&task], &TrustConfig::default(), &pool, &publisher, &metrics)
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

    struct FailPool;

    #[async_trait::async_trait]
    impl AgentPoolLike for FailPool {
        async fn get_or_create_agent(&self, _workspace_id: &str) -> anyhow::Result<String> {
            Ok("agent".into())
        }
        async fn send_message(&self, _workspace_id: &str, _prompt: &str) -> anyhow::Result<AgentRunOutput> {
            anyhow::bail!("llm down")
        }
        async fn shutdown(&self) {}
        fn set_trust_config(&self, _workspace_id: &str, _config: TrustConfig) {}
        fn cleanup_idle(&self) -> usize {
            0
        }
    }

    struct NoopRepo;

    #[async_trait::async_trait]
    impl crate::loop_::heartbeat::repo::HeartbeatTaskRepository for NoopRepo {
        async fn list_by_workspace(
            &self,
            _workspace_id: &str,
        ) -> Result<Vec<HeartbeatTask>, crate::loop_::heartbeat::repo::RepoError> {
            Ok(vec![])
        }
        async fn upsert(
            &self,
            _workspace_id: &str,
            _task: &HeartbeatTask,
            _expected_version: i64,
        ) -> Result<bool, crate::loop_::heartbeat::repo::RepoError> {
            Ok(true)
        }
        async fn insert(
            &self,
            _workspace_id: &str,
            _priority: &str,
            _text: &str,
        ) -> Result<HeartbeatTask, crate::loop_::heartbeat::repo::RepoError> {
            Err(crate::loop_::heartbeat::repo::RepoError::Database("noop".into()))
        }
        async fn set_paused(
            &self,
            _workspace_id: &str,
            _task_id: i64,
            _paused: bool,
        ) -> Result<(), crate::loop_::heartbeat::repo::RepoError> {
            Ok(())
        }
        async fn delete(&self, _workspace_id: &str, _task_id: i64) -> Result<(), crate::loop_::heartbeat::repo::RepoError> {
            Ok(())
        }
        async fn insert_result(
            &self,
            _workspace_id: &str,
            _result: &crate::loop_::heartbeat::types::HeartbeatResult,
        ) -> Result<(), crate::loop_::heartbeat::repo::RepoError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn tick_records_successful_llm_call_in_metrics() {
        let bus = Arc::new(tinyiothub_runtime::EventBus::new());
        let publisher = AiEventPublisher::new(bus);
        let metrics = crate::loop_::heartbeat::metrics::Metrics::new();

        let pool: Arc<dyn AgentPoolLike> = Arc::new(MockPool {
            output: AgentRunOutput {
                text: r#"{"status":"complete","summary":"done","proposals":[]}"#.into(),
                tool_calls: vec![],
            },
        });

        let task = sample_task();
        run_heartbeat_tick("ws", &[&task], &TrustConfig::default(), &pool, &publisher, &metrics)
            .await
            .unwrap();
        publisher.shutdown().await;

        assert_eq!(metrics.llm_calls_total(), 1, "tick must record the LLM call");
        assert_eq!(metrics.llm_calls_failed(), 0);
    }

    #[tokio::test]
    async fn tick_records_failed_llm_call_in_metrics() {
        let bus = Arc::new(tinyiothub_runtime::EventBus::new());
        let publisher = AiEventPublisher::new(bus);
        let metrics = crate::loop_::heartbeat::metrics::Metrics::new();
        let pool: Arc<dyn AgentPoolLike> = Arc::new(FailPool);

        let task = sample_task();
        let r = run_heartbeat_tick("ws", &[&task], &TrustConfig::default(), &pool, &publisher, &metrics).await;
        publisher.shutdown().await;

        assert!(r.is_err());
        assert_eq!(metrics.llm_calls_total(), 1);
        assert_eq!(metrics.llm_calls_failed(), 1, "failed LLM call must be counted");
    }

    fn sample_signal() -> crate::loop_::heartbeat::types::HeartbeatSignal {
        crate::loop_::heartbeat::types::HeartbeatSignal {
            workspace_id: "ws".into(),
            reason: "test".into(),
            context: String::new(),
            priority: crate::loop_::heartbeat::types::SignalPriority::High,
            device_id: None,
            alarm_type: None,
            rule_id: None,
        }
    }

    #[tokio::test]
    async fn paused_loop_updates_paused_metric_and_resumes() {
        use std::sync::atomic::Ordering;

        let metrics = Arc::new(crate::loop_::heartbeat::metrics::Metrics::new());
        let tasks = Arc::new(RwLock::new(vec![sample_task()]));
        let trust = Arc::new(RwLock::new(TrustConfig::default()));
        let pool: Arc<dyn AgentPoolLike> = Arc::new(FailPool);
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(NoopRepo);
        let publisher = Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())));
        let config = HeartbeatConfig {
            enabled: true,
            interval_minutes: 600,
        };
        let (signal_tx, signal_rx) = mpsc::channel::<LoopSignal>(16);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let m = metrics.clone();
        let handle = tokio::spawn(heartbeat_loop(
            "ws".into(),
            tasks,
            trust,
            Some(pool),
            repo,
            publisher,
            config,
            signal_rx,
            cancel_rx,
            m,
        ));

        // Initial tick fails once; 4 more wakeups reach the pause threshold.
        for _ in 0..4 {
            signal_tx.send(LoopSignal::External(sample_signal())).await.unwrap();
        }
        let mut paused_observed = false;
        for _ in 0..100 {
            if metrics.paused_loops.load(Ordering::Relaxed) == 1 {
                paused_observed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(paused_observed, "loop must count itself paused after repeated failures");

        // A wakeup while paused resumes the loop.
        signal_tx.send(LoopSignal::External(sample_signal())).await.unwrap();
        let mut resumed = false;
        for _ in 0..100 {
            if metrics.paused_loops.load(Ordering::Relaxed) == 0 {
                resumed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(resumed, "resume must clear the paused metric");

        let _ = cancel_tx.send(());
        handle.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn paused_loop_auto_resumes_after_cooldown() {
        use std::sync::atomic::Ordering;

        // A paused loop that only resumes on external signals stays dead
        // forever when no signal comes — it must retry after a cooldown.
        let metrics = Arc::new(crate::loop_::heartbeat::metrics::Metrics::new());
        let tasks = Arc::new(RwLock::new(vec![sample_task()]));
        let trust = Arc::new(RwLock::new(TrustConfig::default()));
        let pool: Arc<dyn AgentPoolLike> = Arc::new(FailPool);
        let repo: Arc<dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository> = Arc::new(NoopRepo);
        let publisher = Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())));
        let config = HeartbeatConfig {
            enabled: true,
            interval_minutes: 1,
        };
        let (signal_tx, signal_rx) = mpsc::channel::<LoopSignal>(16);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let m = metrics.clone();
        let handle = tokio::spawn(heartbeat_loop(
            "ws".into(),
            tasks,
            trust,
            Some(pool),
            repo,
            publisher,
            config,
            signal_rx,
            cancel_rx,
            m,
        ));

        for _ in 0..4 {
            signal_tx.send(LoopSignal::External(sample_signal())).await.unwrap();
        }
        for _ in 0..100 {
            if metrics.paused_loops.load(Ordering::Relaxed) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(metrics.paused_loops.load(Ordering::Relaxed), 1, "loop must be paused");

        // No signal arrives; the cooldown interval elapses.
        tokio::time::advance(Duration::from_secs(61)).await;
        for _ in 0..100 {
            if metrics.paused_loops.load(Ordering::Relaxed) == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            metrics.paused_loops.load(Ordering::Relaxed),
            0,
            "paused loop must auto-resume after the cooldown interval"
        );

        let _ = cancel_tx.send(());
        handle.await.unwrap();
    }
}
