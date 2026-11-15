//! 自主唤醒：把一条到期的 `AssistantSchedule` 变成一个 scheduled Session 的 Turn。
//!
//! 触发与执行分离——本节只负责“接到 wake 后怎么跑”，何时唤醒由 `TriggerRouter` 决定。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::port::runtime::AgentLoopHandle;
use tinyiothub_core::heartbeat::HeartbeatTask;
use tinyiothub_skills::LoadedSkill;
use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::{AssistantSchedule, SuppressionConfig};
use tinyiothub_storage::session::Session;
use tinyiothub_storage::suppression::{
    FullAttempt, FullAttemptCompletion, SkipAttempt, SuppressionState, SuppressionTurnObservation, SyncPending,
    TURN_STATE_VERSION,
};

use super::trigger::EventWake;
use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::prompt::PromptMemorySource;
use crate::domains::agent::session::chat::service::ensure_agent;
use crate::domains::agent::session::context::TurnContext;
use crate::domains::agent::session::events::{SessionEventSink, session_event, session_started_payload};
use crate::domains::agent::session::runner::{SessionRunner, TurnCancellation, TurnCounts, TurnOutcome};
use crate::domains::agent::session::suppression::SuppressionMode;
use crate::domains::agent::session::suppression::audit::{
    AtomicSkipAudit, PlannedWake, RecoveryReport, SkipAuditRequest, SuppressionNotifier, SuppressionRecovery,
    WakeAttempt, attempt_event,
};
use crate::domains::agent::session::suppression::contract::SourceDescriptor;
use crate::domains::agent::session::suppression::error::{
    ALERT_EVENT_TYPE, ErrorCode, SuppressionAlert, SuppressionError,
};
use crate::domains::agent::session::suppression::preflight::{PreflightDecision, SkipVeto, SuppressionService};
use crate::domains::agent::session::suppression::registry::{
    self, SemanticSessionEvent, SessionEventsCapture, SourceContext, SourceRegistry, WorkspaceSnapshotProvider,
};
use crate::domains::agent::session::todos::TodoStore;
use crate::domains::agent::session::types::{PermissionMode, Todo, TodoStatus};

/// Assistant agent id 前缀。**不含冒号**：session key 的规范形状是
/// `agent:<ws>:<assistant>/<session>`，id 里多一个 `:` 会让 `SessionKey::parse`
/// 解析失败，scheduled 会话将无法被 chat 会话 API 打开/授权。
pub const ASSISTANT_AGENT_PREFIX: &str = "__assistant__";

/// 每个 workspace 一个常驻 Assistant 的 agent id。
pub fn assistant_agent_id(workspace_id: &str) -> String {
    format!("{ASSISTANT_AGENT_PREFIX}{workspace_id}")
}

/// 自定义 prompt > inspection 技能正文 > 未暂停的 legacy heartbeat tasks > 默认职责。
/// 只有未加载 inspection 技能时才会命中“任务全暂停 → `None`（本轮不唤醒）”分支；
/// 加载了技能则技能正文优先。
pub fn build_wake_message(
    schedule: &AssistantSchedule,
    inspection: Option<&LoadedSkill>,
    tasks: &[HeartbeatTask],
) -> Option<String> {
    let custom = schedule.prompt.trim();
    if !custom.is_empty() {
        return Some(custom.to_string());
    }
    if let Some(skill) = inspection {
        return Some(format!("按「{}」技能执行本轮检查。\n\n{}", skill.title, skill.body));
    }
    let active: Vec<&HeartbeatTask> = tasks.iter().filter(|task| !task.paused).collect();
    if !tasks.is_empty() && active.is_empty() {
        return None;
    }
    if active.is_empty() {
        return Some(
            "按照你的默认职责，检查当前工作区状态并处理需要关注的事项。若无事可做，简短说明即可。".to_string(),
        );
    }
    let mut message = String::from("本轮默认职责：\n");
    for task in active {
        message.push_str(&format!("- {}\n", task.text));
    }
    message.push_str("按顺序执行；用 todo_write 维护进度，完成后总结结果。");
    Some(message)
}

/// `forceFullReason` for an operator/API-triggered reconcile. Ordinary wakes
/// and reconcile share the same full-Turn function; only this reason differs.
pub const FORCE_FULL_REASON_RECONCILE: &str = "reconcile_requested";

/// One preflight capture reused by the full-Turn baseline and the atomic skip.
///
/// The registry owns the adapters' tombstone/payload caches, so the same
/// captured snapshots serve the preflight decision, the eventual full-Turn
/// baseline and the skip audit. Rebuilding it between decision and commit is
/// forbidden.
pub(crate) struct SuppressionObservation {
    /// The durable state row this decision was computed from (or its initial
    /// in-memory shape). `begin_full_attempt`/`begin_skip_attempt` CAS against
    /// this row.
    state: SuppressionState,
    /// Present only when the pre-turn capture and decision could be built.
    captured: Option<CapturedPreflight>,
    control_cursor: i64,
    observed_at: DateTime<Utc>,
    /// Why the decision is unavailable. The Turn is still durably fenced but
    /// it can never skip or install a baseline.
    unavailable_reason: Option<String>,
    /// True when the user cursor could not be read. The Turn may still install
    /// a baseline from the pre-turn capture, but it must never skip.
    skip_disabled: bool,
    /// A manual operation in `off` mode has no Source capture, so it settles
    /// its durable attempt without installing a baseline (and keeps the old
    /// baseline ineligible).
    settle_without_baseline: bool,
    /// Wall-clock preflight duration recorded as gate evidence.
    preflight_latency_ms: u64,
}

struct CapturedPreflight {
    registry: Arc<SourceRegistry>,
    decision: PreflightDecision,
}

/// Durable link from a replayed full attempt to the committed skip it
/// supersedes (`previousAttemptId` / `supersedes` in the event stream).
#[derive(Clone, Debug)]
struct ReplayLink {
    previous_attempt_id: String,
    supersedes: bool,
}

impl SuppressionObservation {
    fn decision(&self) -> Option<&PreflightDecision> {
        self.captured.as_ref().map(|captured| &captured.decision)
    }

    fn registry_version(&self) -> Option<String> {
        self.captured
            .as_ref()
            .map(|captured| captured.registry.registry_version())
            .or_else(|| self.state.registry_version.clone())
            .filter(|value| !value.trim().is_empty())
    }
}

fn fencing_observation(state: SuppressionState, observed_at: DateTime<Utc>, reason: &str) -> SuppressionObservation {
    SuppressionObservation {
        state,
        captured: None,
        control_cursor: 0,
        observed_at,
        unavailable_reason: Some(reason.to_string()),
        skip_disabled: true,
        settle_without_baseline: false,
        preflight_latency_ms: 0,
    }
}

/// Record a human-confirmed false skip through the real Session Events
/// producer. Shared by the wake executor and the admin endpoint so there is
/// exactly one `false_skip_detected` emission path.
pub async fn emit_confirmed_false_skip(
    db: &Db,
    events: &dyn SessionEventSink,
    workspace_id: &str,
    turn_id: &str,
    reviewer: &str,
    root_cause: &str,
) -> Result<(), String> {
    if reviewer.trim().is_empty() {
        return Err("false skip reviewer must not be empty".to_string());
    }
    let observation = db
        .suppression_turn_observation(workspace_id, turn_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("turn {turn_id} has no recorded suppression observation"))?;
    if !observation.would_skip || !matches!(observation.mode, SuppressionMode::Canary | SuppressionMode::Enforce) {
        return Err(format!(
            "turn {turn_id} is not a recorded canary/enforce wouldSkip observation"
        ));
    }
    let session_key = format!("agent:{workspace_id}:{}/main", assistant_agent_id(workspace_id));
    let state = db
        .load_suppression_state(workspace_id, &session_key)
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| SuppressionState::initial(workspace_id, &session_key));
    let trace_id = Uuid::new_v4().to_string();
    let context = TurnContext::new(
        workspace_id.to_string(),
        assistant_agent_id(workspace_id),
        session_key,
        trace_id.clone(),
        0,
    );
    let payload = json!({
        "code": ErrorCode::FalseSkipDetected.as_str(),
        "workspaceId": workspace_id,
        "turnId": turn_id,
        "baselineStateId": &state.baseline_state_id,
        "currentRegistryVersion": &observation.registry_version,
        "baselineRegistryVersion": &state.registry_version,
        "vetoes": &observation.vetoes,
        "consecutiveFailures": 0,
        "metrics": {
            "reviewer": reviewer,
            "rootCause": root_cause,
            "source": "confirmed_false_skip_endpoint",
        },
        "traceId": trace_id,
    });
    events
        .emit(session_event(&context, ALERT_EVENT_TYPE, payload))
        .await
        .map_err(|error| error.to_string())?;
    tracing::error!(
        event_type = ALERT_EVENT_TYPE,
        alert_code = ErrorCode::FalseSkipDetected.as_str(),
        %workspace_id,
        %turn_id,
        %reviewer,
        "assistant suppression alert"
    );
    Ok(())
}

/// Post-commit notification consumer backed by the Session Events sink.
struct SessionEventNotifier {
    events: Arc<dyn SessionEventSink>,
}

#[async_trait::async_trait]
impl SuppressionNotifier for SessionEventNotifier {
    async fn notify(&self, workspace_id: &str, session_id: &str) -> std::result::Result<(), String> {
        self.events
            .notify(workspace_id, session_id)
            .await
            .map_err(|error| error.to_string())
    }
}

impl WakeRunner {
    fn suppression_recovery(&self) -> SuppressionRecovery {
        SuppressionRecovery::new(
            self.db.clone(),
            Arc::new(SessionEventNotifier {
                events: self.events.clone(),
            }),
        )
    }

    /// Startup/restart recovery entry point (ruling R16).
    ///
    /// The Assistant session key is deterministic per workspace, so recovery
    /// inspects exactly the durable suppression state that scheduled and
    /// manual Turns share.
    pub async fn recover_suppression(&self, workspace_id: &str) -> Vec<RecoveryReport> {
        let session_key = format!("agent:{}:{}/main", workspace_id, assistant_agent_id(workspace_id));
        match self
            .suppression_recovery()
            .recover_session(workspace_id, &session_key)
            .await
        {
            Ok(report) => {
                for alert in &report.alerts {
                    self.emit_suppression_alert(&session_key, alert).await;
                }
                tracing::info!(
                    %workspace_id,
                    %session_key,
                    replayed_committed_skip = report.replayed_committed_skip,
                    settled_full_attempt = report.settled_full_attempt.as_deref().unwrap_or(""),
                    settled_audit_attempt = report.settled_audit_attempt.as_deref().unwrap_or(""),
                    cleared = ?report.cleared,
                    alerts = report.alerts.len(),
                    "suppression startup recovery finished"
                );
                vec![report]
            }
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression startup recovery failed");
                Vec::new()
            }
        }
    }

    /// Emit an alert through Session Events, tracing and the report projection
    /// (`assistant.suppression.alert` is excluded from semantic work).
    async fn emit_suppression_alert(&self, session_key: &str, alert: &SuppressionAlert) {
        tracing::error!(
            event_type = ALERT_EVENT_TYPE,
            alert_code = %alert.code,
            workspace_id = %alert.workspace_id,
            turn_id = alert.turn_id.as_deref().unwrap_or(""),
            baseline_state_id = alert.baseline_state_id.as_deref().unwrap_or(""),
            current_registry_version = alert.current_registry_version.as_deref().unwrap_or(""),
            baseline_registry_version = alert.baseline_registry_version.as_deref().unwrap_or(""),
            vetoes = ?alert.vetoes,
            consecutive_failures = alert.consecutive_failures,
            trace_id = %alert.trace_id,
            "assistant suppression alert"
        );
        let context = TurnContext::new(
            alert.workspace_id.clone(),
            assistant_agent_id(&alert.workspace_id),
            session_key.to_string(),
            alert.trace_id.clone(),
            0,
        );
        let input = session_event(&context, ALERT_EVENT_TYPE, alert.payload());
        if let Err(error) = self.events.emit(input).await {
            tracing::warn!(%error, alert_code = %alert.code, "failed to persist suppression alert event");
        }
    }

    async fn preflight_alert(
        &self,
        workspace_id: &str,
        session_key: &str,
        state: &SuppressionState,
        cause: &str,
    ) -> SuppressionAlert {
        SuppressionAlert {
            code: ErrorCode::PreflightTimeout.as_str().to_string(),
            workspace_id: workspace_id.to_string(),
            turn_id: None,
            baseline_state_id: state.baseline_state_id.clone(),
            current_registry_version: None,
            baseline_registry_version: state.registry_version.clone(),
            vetoes: Vec::new(),
            consecutive_failures: self.consecutive_failures(workspace_id, session_key).await,
            metrics: json!({ "cause": cause }),
            trace_id: Uuid::new_v4().to_string(),
        }
    }

    /// Trailing failed outcomes plus this failure. Safe counts only.
    async fn consecutive_failures(&self, workspace_id: &str, session_key: &str) -> u32 {
        let prior = match self.db.recent_suppression_outcomes(workspace_id, session_key, 10).await {
            Ok(outcomes) => outcomes
                .iter()
                .take_while(|outcome| !matches!(outcome.outcome.as_str(), "completed" | "skipped"))
                .count() as u32,
            Err(_) => 0,
        };
        prior.saturating_add(1)
    }

    /// The one full-input prompt builder used by scheduled wake, reconcile and
    /// event wake. The caller passes the single Session Events capture so the
    /// user cursor and the semantic events come from the same read.
    async fn wake_prompt(
        &self,
        workspace_id: &str,
        assistant_id: &str,
        turn_input: String,
        semantic_events: &[SemanticSessionEvent],
    ) -> anyhow::Result<crate::domains::agent::prompt::assistant::PromptPackage> {
        let snapshot = match &self.snapshot_provider {
            Some(provider) => provider.workspace_snapshot(workspace_id).await,
            None => None,
        };
        crate::domains::agent::prompt::assistant::build_assistant_prompt_with_sources(
            &self.system_prompts,
            workspace_id,
            assistant_id,
            turn_input,
            self.memory_source.as_ref(),
            snapshot.as_ref(),
            semantic_events,
        )
        .await
    }

    /// The wake-level Session Events read. Test-only fault injection lets the
    /// lifecycle tests exercise a transient read failure without breaking the
    /// `session.events` Source adapter (which has its own read).
    async fn load_wake_session_events(
        &self,
        workspace_id: &str,
        session_key: &str,
    ) -> Result<SessionEventsCapture, SuppressionError> {
        #[cfg(test)]
        if self.fail_session_events_read.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SuppressionError::new(
                "source_unavailable",
                "injected session events read failure",
                "retry the wake",
            ));
        }
        registry::load_session_events(&self.db, workspace_id, session_key).await
    }

    /// Read-only suppression preflight observation.
    ///
    /// Exactly one registry and capture is built per wake. The caller supplies
    /// the user cursor from the same Session Events read used to build the
    /// full prompt, so preflight adds no second Session Events pass.
    ///
    /// Returns `None` only when no suppression config exists or the mode is
    /// `off`. Whenever suppression is configured, *some* observation is
    /// returned: even an unreadable config/state/capture produces a
    /// fencing-only observation, so a failed preflight can never leave the old
    /// baseline eligible for the next skip.
    async fn observe_suppression(
        &self,
        workspace_id: &str,
        session_key: &str,
        current_user_seq: Option<i64>,
    ) -> Option<SuppressionObservation> {
        let config = match self.db.get_suppression_config(workspace_id).await {
            Ok(Some(config)) => config,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression preflight: config read failed");
                let state = self
                    .db
                    .load_suppression_state(workspace_id, session_key)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| SuppressionState::initial(workspace_id, session_key));
                let alert = self
                    .preflight_alert(workspace_id, session_key, &state, "config_read_failed")
                    .await;
                self.emit_suppression_alert(session_key, &alert).await;
                return Some(fencing_observation(state, chrono::Utc::now(), "config_read_failed"));
            }
        };
        if config.mode == SuppressionMode::Off {
            return None;
        }
        let state = match self.db.load_suppression_state(workspace_id, session_key).await {
            Ok(Some(state)) => state,
            Ok(None) => SuppressionState::initial(workspace_id, session_key),
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "suppression preflight: state read failed");
                let state = SuppressionState::initial(workspace_id, session_key);
                let alert = self
                    .preflight_alert(workspace_id, session_key, &state, "state_read_failed")
                    .await;
                self.emit_suppression_alert(session_key, &alert).await;
                return Some(fencing_observation(state, chrono::Utc::now(), "state_read_failed"));
            }
        };
        Some(
            self.observe_with(workspace_id, session_key, &config, state, current_user_seq, true)
                .await,
        )
    }

    /// Build one preflight capture over an already-read config/state.
    ///
    /// Any capture failure produces a fencing-only observation: the Turn may
    /// run but can never skip or install a baseline from incomplete evidence.
    /// `alert_on_failure` is false for read-only manual preflight.
    async fn observe_with(
        &self,
        workspace_id: &str,
        session_key: &str,
        config: &SuppressionConfig,
        state: SuppressionState,
        current_user_seq: Option<i64>,
        alert_on_failure: bool,
    ) -> SuppressionObservation {
        let registry = match registry::default_sources(
            self.system_prompts.clone(),
            self.db.clone(),
            self.memory_source.clone(),
            self.snapshot_provider.clone(),
        ) {
            Ok(registry) => Arc::new(registry),
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    %workspace_id,
                    "suppression preflight: registry build failed"
                );
                if alert_on_failure {
                    let alert = SuppressionAlert {
                        code: ErrorCode::SourceContractInvalid.as_str().to_string(),
                        workspace_id: workspace_id.to_string(),
                        turn_id: None,
                        baseline_state_id: state.baseline_state_id.clone(),
                        current_registry_version: None,
                        baseline_registry_version: state.registry_version.clone(),
                        vetoes: Vec::new(),
                        consecutive_failures: self.consecutive_failures(workspace_id, session_key).await,
                        metrics: json!({ "cause": "registry_build_failed" }),
                        trace_id: Uuid::new_v4().to_string(),
                    };
                    self.emit_suppression_alert(session_key, &alert).await;
                }
                return fencing_observation(state, chrono::Utc::now(), "registry_build_failed");
            }
        };
        let observed_at = chrono::Utc::now();
        let context = SourceContext {
            workspace_id: workspace_id.to_string(),
            session_id: session_key.to_string(),
            observed_at,
            // The service applies the configured mode before capture.
            mode: config.mode.into(),
        };
        let service = SuppressionService::new(registry.clone());
        let preflight_started = std::time::Instant::now();
        let evaluated = service
            .evaluate(context, Some(&state), config, current_user_seq.unwrap_or(0))
            .await;
        let preflight_latency_ms = preflight_started.elapsed().as_millis() as u64;
        let mut decision = match evaluated {
            Ok(decision) => decision,
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    %workspace_id,
                    "suppression preflight failed; fenced full turn"
                );
                if alert_on_failure {
                    let code = ErrorCode::parse(&error.code).unwrap_or(ErrorCode::PreflightTimeout);
                    let alert = SuppressionAlert {
                        code: code.as_str().to_string(),
                        workspace_id: workspace_id.to_string(),
                        turn_id: None,
                        baseline_state_id: state.baseline_state_id.clone(),
                        current_registry_version: None,
                        baseline_registry_version: state.registry_version.clone(),
                        vetoes: Vec::new(),
                        consecutive_failures: self.consecutive_failures(workspace_id, session_key).await,
                        metrics: json!({ "cause": "preflight_capture_failed" }),
                        trace_id: if error.trace.trace_id.is_empty() {
                            Uuid::new_v4().to_string()
                        } else {
                            error.trace.trace_id.clone()
                        },
                    };
                    self.emit_suppression_alert(session_key, &alert).await;
                }
                return fencing_observation(state, observed_at, "preflight_capture_failed");
            }
        };
        // An unknown cursor must never authorize a skip; the pre-turn capture
        // is still valid baseline evidence, but its control cursor is unknown,
        // so record 0 (the conservative preflight answer).
        let skip_disabled = current_user_seq.is_none();
        if skip_disabled {
            decision.would_skip = false;
            if !decision.vetoes.contains(&SkipVeto::NewUserInput) {
                decision.vetoes.push(SkipVeto::NewUserInput);
            }
        }
        tracing::info!(
            workspace_id = %workspace_id,
            would_skip = decision.would_skip,
            vetoes = ?decision.vetoes,
            fence = %decision.fence,
            baseline_state_id = decision.baseline_state_id.as_deref().unwrap_or(""),
            config_version = decision.config_version,
            mode = %config.mode.as_str(),
            skip_disabled,
            "suppression preflight observed"
        );
        SuppressionObservation {
            state,
            captured: Some(CapturedPreflight { registry, decision }),
            control_cursor: current_user_seq.unwrap_or(0),
            observed_at,
            unavailable_reason: None,
            skip_disabled,
            settle_without_baseline: config.mode == SuppressionMode::Off,
            preflight_latency_ms,
        }
    }

    /// Record one completed suppression-aware Turn as gate evidence.
    ///
    /// Only `shadow|canary|enforce` are recorded. A replay keeps the first row
    /// (the DB primary key is `(workspace_id, turn_id)`), so retries cannot
    /// inflate denominators.
    #[allow(clippy::too_many_arguments)]
    async fn record_turn_observation(
        &self,
        workspace_id: &str,
        session_key: &str,
        turn_id: &str,
        attempt_id: &str,
        observation: &SuppressionObservation,
        decision: &PreflightDecision,
        counts: TurnCounts,
    ) {
        if !matches!(
            decision.mode,
            SuppressionMode::Shadow | SuppressionMode::Canary | SuppressionMode::Enforce
        ) {
            return;
        }
        let Some(registry_version) = observation.registry_version() else {
            tracing::warn!(
                %workspace_id,
                %turn_id,
                "suppression observation skipped: no registryVersion"
            );
            return;
        };
        let record = SuppressionTurnObservation {
            workspace_id: workspace_id.to_string(),
            session_id: session_key.to_string(),
            turn_id: turn_id.to_string(),
            attempt_id: attempt_id.to_string(),
            mode: decision.mode,
            observed_at: observation.observed_at,
            would_skip: decision.would_skip,
            vetoes: decision.vetoes.iter().map(|veto| veto.as_str().to_string()).collect(),
            model_calls: counts.model_calls,
            tool_calls: counts.tool_calls,
            preflight_latency_ms: observation.preflight_latency_ms,
            registry_version,
        };
        if let Err(error) = self.db.record_suppression_turn_observation(&record).await {
            tracing::warn!(
                %error,
                %workspace_id,
                %turn_id,
                "failed to record suppression observation"
            );
        }
    }

    /// Record a human-confirmed false skip through the real alert producer.
    ///
    /// The event is `assistant.suppression.alert` with code
    /// `false_skip_detected`; R18 staleness and the report read it back from
    /// the workspace session events.
    pub async fn record_confirmed_false_skip(
        &self,
        workspace_id: &str,
        turn_id: &str,
        reviewer: &str,
        root_cause: &str,
    ) -> Result<(), String> {
        emit_confirmed_false_skip(
            &self.db,
            self.events.as_ref(),
            workspace_id,
            turn_id,
            reviewer,
            root_cause,
        )
        .await
    }

    /// Read-only public preflight used by the GET endpoints.
    ///
    /// Unlike a scheduled wake this never creates a schedule row, state row or
    /// event. It reports `off` as an ordinary decision so operators can see
    /// the kill-switch state and the registered Source IDs.
    pub async fn manual_preflight(&self, workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        let session_key = format!("agent:{}:{}/main", workspace_id, assistant_agent_id(workspace_id));
        let control_cursor = match self.load_wake_session_events(workspace_id, &session_key).await {
            Ok(capture) => Some(capture.control_cursor),
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    workspace_id = %workspace_id,
                    "manual preflight: session events read failed"
                );
                None
            }
        };
        let config = self
            .db
            .get_suppression_config(workspace_id)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?
            .unwrap_or_else(|| SuppressionConfig::off(workspace_id));
        let state = self
            .db
            .load_suppression_state(workspace_id, &session_key)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?
            .unwrap_or_else(|| SuppressionState::initial(workspace_id, &session_key));
        let observation = self
            .observe_with(workspace_id, &session_key, &config, state, control_cursor, false)
            .await;
        let (registry_version, descriptors) = match &observation.captured {
            Some(captured) => (captured.registry.registry_version(), captured.registry.descriptors()),
            None => {
                // A capture failure must stay visible; still name the
                // registered Sources and registry version.
                let registry = registry::default_sources(
                    self.system_prompts.clone(),
                    self.db.clone(),
                    self.memory_source.clone(),
                    self.snapshot_provider.clone(),
                )
                .map_err(|error| ManualStartError::Unavailable(error.what.clone()))?;
                (registry.registry_version(), registry.descriptors())
            }
        };
        let decision = observation.decision().cloned().unwrap_or_else(|| PreflightDecision {
            would_skip: false,
            mode: config.mode,
            vetoes: vec![SkipVeto::Disabled],
            snapshot: Vec::new(),
            baseline_state_id: None,
            config_version: config.config_version,
            fence: String::new(),
        });
        Ok(ManualPreflight {
            session_key,
            mode: config.mode,
            reconciliation_seconds: config.reconciliation_seconds,
            config_version: config.config_version,
            registry_version,
            descriptors,
            decision,
        })
    }

    /// Allocate the durable attempt for a manual wake/reconcile.
    ///
    /// This runs before the HTTP 202: the returned `turnId` is the executed
    /// attempt's identity, never a predicted one. Reconcile forces the full
    /// path with the one shared Turn implementation; wake may allocate a skip
    /// attempt when the normal scheduled-wake decision allows it.
    pub async fn prepare_manual(
        &self,
        schedule: &AssistantSchedule,
        operation: ManualOperation,
    ) -> Result<PreparedManual, ManualStartError> {
        let workspace_id = schedule.workspace_id.as_str();
        let session_key = format!("agent:{}:{}/main", workspace_id, schedule.assistant_id);
        let (semantic_events, control_cursor) = match self.load_wake_session_events(workspace_id, &session_key).await {
            Ok(capture) => (capture.events, Some(capture.control_cursor)),
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    workspace_id = %workspace_id,
                    "manual operation: session events read failed; fenced full turn"
                );
                (Vec::new(), None)
            }
        };
        let config = self
            .db
            .get_suppression_config(workspace_id)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?
            .unwrap_or_else(|| SuppressionConfig::off(workspace_id));
        let state = self
            .db
            .load_suppression_state(workspace_id, &session_key)
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?
            .unwrap_or_else(|| SuppressionState::initial(workspace_id, &session_key));
        let observation = self
            .observe_with(workspace_id, &session_key, &config, state, control_cursor, true)
            .await;
        let wake = PlannedWake::unplanned(workspace_id, session_key.clone());

        let wants_skip = operation == ManualOperation::Wake
            && !observation.skip_disabled
            && observation
                .decision()
                .is_some_and(|decision| decision.would_skip && decision.mode.allows_skip());
        if wants_skip {
            match self.db.begin_skip_attempt(&session_key, &observation.state).await {
                Ok(skip) => {
                    let attempt = wake.attempt(skip.attempt_no);
                    return Ok(PreparedManual {
                        schedule: schedule.to_owned(),
                        session_key,
                        semantic_events,
                        observation,
                        wake,
                        attempt,
                        kind: PreparedManualKind::Skip(skip),
                        force_full_reason: None,
                    });
                }
                Err(error) => {
                    // A concurrent change (mode, config, baseline or fence)
                    // falls back to the shared full path.
                    tracing::warn!(%error, %session_key, "manual wake: skip allocation rejected; running a full Turn");
                }
            }
        }

        let durable = match self.db.begin_full_attempt(&session_key, &observation.state).await {
            Ok(durable) => durable,
            Err(tinyiothub_storage::error::DbError::CasConflict { .. }) => {
                return Err(ManualStartError::InFlight);
            }
            Err(error) => return Err(ManualStartError::Unavailable(error.to_string())),
        };
        let attempt = wake.attempt(durable.attempt_no);
        Ok(PreparedManual {
            schedule: schedule.to_owned(),
            session_key,
            semantic_events,
            observation,
            wake,
            attempt,
            kind: PreparedManualKind::Full(durable),
            force_full_reason: (operation == ManualOperation::Reconcile).then_some(FORCE_FULL_REASON_RECONCILE),
        })
    }

    /// Execute a prepared manual attempt. The 202 has already been returned.
    pub async fn execute_manual(&self, prepared: PreparedManual, cancel: CancellationToken) -> WakeReport {
        let PreparedManual {
            schedule,
            session_key,
            semantic_events,
            observation,
            wake,
            attempt,
            kind,
            force_full_reason,
        } = prepared;
        let assistant_id = assistant_agent_id(&schedule.workspace_id);

        // A prepared skip commits without building a prompt or starting the
        // shared agent loop.
        if let PreparedManualKind::Skip(skip) = &kind {
            if let Err(error) = self
                .db
                .ensure_session(&session_key, &schedule.workspace_id, &assistant_id)
                .await
            {
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
            let Some(decision) = observation.decision() else {
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::AuditFailed,
                };
            };
            let audit = AtomicSkipAudit::new(self.db.clone());
            match audit
                .commit(SkipAuditRequest {
                    wake: &wake,
                    attempt: skip,
                    decision,
                    checked_at: chrono::Utc::now(),
                })
                .await
            {
                Ok(_) => {
                    // Manual operations do not advance the Schedule; commit
                    // of the skip lifecycle is the whole reconciliation.
                    if let Err(error) = self
                        .db
                        .clear_sync_pending(&schedule.workspace_id, &session_key, SyncPending::Schedule)
                        .await
                    {
                        tracing::warn!(%error, %session_key, "manual wake: schedule barrier clear failed");
                    }
                    self.record_turn_observation(
                        &schedule.workspace_id,
                        &session_key,
                        &attempt.turn_id,
                        &skip.attempt_id,
                        &observation,
                        decision,
                        TurnCounts::default(),
                    )
                    .await;
                    let session = match self.db.get_session(&session_key).await {
                        Ok(Some(session)) => session,
                        Ok(None) => {
                            Session::new(session_key.clone(), schedule.workspace_id.clone(), assistant_id.clone())
                        }
                        Err(error) => {
                            return WakeReport {
                                session_key,
                                outcome: WakeOutcome::Failed {
                                    error: error.to_string(),
                                },
                            };
                        }
                    };
                    self.reconcile_committed_skip(&session, decision).await;
                    WakeReport {
                        session_key,
                        outcome: WakeOutcome::Skipped,
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, %session_key, "manual wake: prepared skip commit failed");
                    WakeReport {
                        session_key,
                        outcome: WakeOutcome::AuditFailed,
                    }
                }
            }
        } else {
            let tasks = self
                .db
                .list_heartbeat_tasks(&schedule.workspace_id)
                .await
                .unwrap_or_default();
            let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(&schedule.workspace_id);
            let skills = crate::domains::agent::pool::load_workspace_skills(&ws_dir);
            let inspection = skills.iter().find(|skill| skill.name == "inspection");
            let Some(message) = build_wake_message(&schedule, inspection, &tasks) else {
                // Nothing to run: settle the durable attempt without a
                // baseline so it never blocks later work.
                let PreparedManualKind::Full(durable) = &kind else {
                    unreachable!("skip kind handled above");
                };
                if let Err(error) = self
                    .db
                    .settle_full_attempt_without_baseline(&schedule.workspace_id, &durable.attempt_id, "skipped")
                    .await
                {
                    tracing::warn!(%error, %durable.attempt_id, "manual wake: empty-message settlement failed");
                }
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Skipped,
                };
            };
            let prompt = match self
                .wake_prompt(
                    &schedule.workspace_id,
                    &schedule.assistant_id,
                    message,
                    &semantic_events,
                )
                .await
            {
                Ok(prompt) => prompt,
                Err(error) => {
                    let PreparedManualKind::Full(durable) = &kind else {
                        unreachable!("skip kind handled above");
                    };
                    self.fail_full_attempt(&schedule.workspace_id, durable, "prompt_build_failed")
                        .await;
                    return WakeReport {
                        session_key,
                        outcome: WakeOutcome::Failed {
                            error: error.to_string(),
                        },
                    };
                }
            };
            let agent = match ensure_agent(
                &self.pool,
                &schedule.assistant_id,
                &schedule.workspace_id,
                &AgentRuntimeConfig::default(),
                true,
                &prompt,
            )
            .await
            {
                Ok(agent) => agent,
                Err(error) => {
                    let PreparedManualKind::Full(durable) = &kind else {
                        unreachable!("skip kind handled above");
                    };
                    self.fail_full_attempt(&schedule.workspace_id, durable, "agent_start_failed")
                        .await;
                    return WakeReport {
                        session_key,
                        outcome: WakeOutcome::Failed {
                            error: error.to_string(),
                        },
                    };
                }
            };
            let PreparedManualKind::Full(durable) = &kind else {
                unreachable!("skip kind handled above");
            };
            let prepared_ref = PreparedAttemptRef {
                attempt: durable,
                wake_attempt: &attempt,
            };
            let report = self
                .run_session_with_agent(
                    SessionOrigin {
                        workspace_id: &schedule.workspace_id,
                        assistant_id: &schedule.assistant_id,
                        kind: "scheduled",
                        trigger: "schedule",
                        source_id: None,
                        planned_wake_at: None,
                        suppression: Some(&observation),
                        force_full_reason,
                        prepared: Some(prepared_ref),
                    },
                    agent,
                    &prompt.turn_input,
                    schedule.permission_mode,
                    cancel,
                )
                .await;
            // The manual attempt was allocated before the 202. A Session
            // lifecycle failure can return before the shared settlement, so
            // never leave that durable fence in progress. Repeating the
            // settlement after the normal failure path is idempotent.
            if matches!(&report.outcome, WakeOutcome::Failed { .. })
                && let Err(error) = self
                    .db
                    .fail_full_attempt(&schedule.workspace_id, &durable.attempt_id, "manual_turn_failed")
                    .await
            {
                tracing::warn!(%error, %durable.attempt_id, "manual turn: fence settlement failed");
            }
            report
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOutcome {
    Completed,
    Partial,
    Failed {
        error: String,
    },
    Cancelled,
    Skipped,
    /// The skip audit failed or rolled back. TriggerRouter persists this as the
    /// Schedule outcome and it may not be overwritten by a later generic
    /// skipped/failed outcome.
    AuditFailed,
}

impl WakeOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::AuditFailed => "audit_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeReport {
    pub session_key: String,
    pub outcome: WakeOutcome,
}

/// Operator-triggered suppression operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualOperation {
    /// Normal scheduled-wake semantics: preflight may decide to skip.
    Wake,
    /// Force one full Turn, ignoring `wouldSkip`.
    Reconcile,
}

impl ManualOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wake => "wake",
            Self::Reconcile => "reconcile",
        }
    }
}

/// Why a manual operation could not be accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualStartError {
    /// A scheduled or manual Turn is already in flight for this workspace.
    InFlight,
    /// Global preflight capacity is saturated (Task 9 owns the semaphore).
    Backpressure { retry_after_ms: u64 },
    /// The operation is not wired or an internal step failed.
    Unavailable(String),
}

/// Read-only preflight projection for the public GET endpoints.
#[derive(Clone, Debug)]
pub struct ManualPreflight {
    pub session_key: String,
    pub mode: SuppressionMode,
    pub reconciliation_seconds: u32,
    pub config_version: i64,
    pub registry_version: String,
    pub descriptors: Vec<SourceDescriptor>,
    pub decision: PreflightDecision,
}

/// The already-durable attempt named by a 202 response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualTurnStart {
    pub session_key: String,
    pub turn_id: String,
    /// Durable suppression attempt id (`{workspace}:{session}:attempt:{n}`).
    /// Kept separate from `turn_id` so recovery can join
    /// `suppression_state.latest_attempt_id`.
    pub attempt_id: String,
}

/// A manual Turn whose durable attempt was allocated before the 202 response.
///
/// Fields stay private so callers cannot forge an attempt identity; the HTTP
/// layer may only read the session key and turn id it must return.
pub struct PreparedManual {
    schedule: AssistantSchedule,
    session_key: String,
    semantic_events: Vec<SemanticSessionEvent>,
    observation: SuppressionObservation,
    wake: PlannedWake,
    attempt: WakeAttempt,
    kind: PreparedManualKind,
    force_full_reason: Option<&'static str>,
}

enum PreparedManualKind {
    Skip(SkipAttempt),
    Full(FullAttempt),
}

impl PreparedManual {
    pub fn session_key(&self) -> &str {
        &self.session_key
    }

    pub fn turn_id(&self) -> &str {
        &self.attempt.turn_id
    }

    /// Durable attempt id allocated by the storage layer.
    pub fn attempt_id(&self) -> &str {
        match &self.kind {
            PreparedManualKind::Skip(skip) => &skip.attempt_id,
            PreparedManualKind::Full(full) => &full.attempt_id,
        }
    }
}

/// A manual full attempt that is already durably allocated.
#[derive(Clone, Copy)]
pub(crate) struct PreparedAttemptRef<'a> {
    attempt: &'a FullAttempt,
    wake_attempt: &'a WakeAttempt,
}

#[async_trait::async_trait]
pub trait WakeExecutor: Send + Sync {
    async fn execute(&self, schedule: AssistantSchedule, cancel: CancellationToken) -> WakeReport;

    /// 事件 wake 入口。默认跳过——既有实现者无需改动即可继续编译。
    async fn execute_event(&self, _wake: EventWake, _cancel: CancellationToken) -> WakeReport {
        WakeReport {
            session_key: String::new(),
            outcome: WakeOutcome::Skipped,
        }
    }

    /// Startup/restart recovery for durable suppression fences. The default is
    /// a no-op so existing test executors keep compiling.
    async fn recover_suppression(&self, _workspace_id: &str) -> Vec<RecoveryReport> {
        Vec::new()
    }

    /// Read-only public preflight. Default: manual operations are unwired.
    async fn manual_preflight(&self, _workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        Err(ManualStartError::Unavailable(
            "suppression manual operations are unavailable".to_string(),
        ))
    }

    /// Allocate the durable attempt synchronously. Default: unwired.
    async fn prepare_manual(
        &self,
        _schedule: &AssistantSchedule,
        _operation: ManualOperation,
    ) -> Result<PreparedManual, ManualStartError> {
        Err(ManualStartError::Unavailable(
            "suppression manual operations are unavailable".to_string(),
        ))
    }

    /// Execute a prepared manual attempt asynchronously.
    async fn execute_manual(&self, _prepared: PreparedManual, _cancel: CancellationToken) -> WakeReport {
        WakeReport {
            session_key: String::new(),
            outcome: WakeOutcome::Failed {
                error: "suppression manual operations are unavailable".to_string(),
            },
        }
    }

    /// Record a human-confirmed false skip through the real alert producer.
    /// Default: unwired.
    async fn record_confirmed_false_skip(
        &self,
        _workspace_id: &str,
        _turn_id: &str,
        _reviewer: &str,
        _root_cause: &str,
    ) -> Result<(), ManualStartError> {
        Err(ManualStartError::Unavailable(
            "confirmed false-skip recording is unavailable".to_string(),
        ))
    }
}

pub struct WakeRunner {
    pub(crate) db: Arc<Db>,
    pool: Arc<AgentPool>,
    events: Arc<dyn SessionEventSink>,
    todos: Arc<TodoStore>,
    runner: Arc<SessionRunner>,
    system_prompts: Arc<tinyiothub_core::config::SystemPromptsConfig>,
    /// The same long-term memory selection the chat path injects.
    memory_source: Option<Arc<dyn PromptMemorySource>>,
    /// Existing live workspace snapshot; injected by the composition layer.
    snapshot_provider: Option<Arc<dyn WorkspaceSnapshotProvider>>,
    /// Test-only fault injection for the wake-level Session Events read.
    #[cfg(test)]
    fail_session_events_read: std::sync::atomic::AtomicBool,
}

/// Build the memory port over the platform database, matching the chat
/// composition (`MemoryStore::new(db.pool())`).
fn default_memory_source(db: &Arc<Db>) -> Arc<dyn PromptMemorySource> {
    Arc::new(
        crate::domains::agent::session::persist::memory::PromptMemoryStoreAdapter(Arc::new(
            tinyiothub_storage::memory::MemoryStore::new(db.pool().clone()),
        )),
    )
}

/// 一次 wake session 的来源（scheduled / event 共用同一套生命周期写入）。
///
/// `source_id` 为 `None` 时，`trigger.received.sourceId` 回落到本轮的
/// `wake.dispatched.wakeEventId`（scheduled 路径的相关性保持）。
struct SessionOrigin<'a> {
    workspace_id: &'a str,
    assistant_id: &'a str,
    kind: &'a str,
    trigger: &'a str,
    source_id: Option<String>,
    /// Stable planned wake time for scheduled dispatches. `None` keeps the
    /// previous per-invocation identity used by direct and event wakes.
    planned_wake_at: Option<DateTime<Utc>>,
    /// The one preflight capture for this wake, if suppression is configured.
    suppression: Option<&'a SuppressionObservation>,
    /// `forceFullReason`: set by reconcile to bypass the skip path. Ordinary
    /// scheduled wakes leave it `None`; the full-Turn path is identical.
    force_full_reason: Option<&'a str>,
    /// A manual attempt allocated before the HTTP 202 response. Ordinary
    /// scheduled/event wakes leave it `None`.
    prepared: Option<PreparedAttemptRef<'a>>,
}

impl WakeRunner {
    pub fn from_runtime(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        runtime: &crate::domains::agent::session::SessionRuntime,
    ) -> Self {
        let memory_source = Some(default_memory_source(&db));
        Self {
            db,
            pool,
            events: runtime.events.clone(),
            todos: runtime.todos.clone(),
            runner: runtime.runner.clone(),
            system_prompts: runtime.system_prompts.clone(),
            memory_source,
            snapshot_provider: None,
            #[cfg(test)]
            fail_session_events_read: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn from_parts(
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        events: Arc<dyn SessionEventSink>,
        todos: Arc<TodoStore>,
        runner: Arc<SessionRunner>,
    ) -> Self {
        let memory_source = Some(default_memory_source(&db));
        Self {
            db,
            pool,
            events,
            todos,
            runner,
            system_prompts: Arc::new(tinyiothub_core::config::SystemPromptsConfig::default()),
            memory_source,
            snapshot_provider: None,
            #[cfg(test)]
            fail_session_events_read: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Inject the existing live workspace snapshot port. Without it the
    /// `workspace.snapshot` Source is degraded (never silently empty).
    pub fn with_snapshot_provider(mut self, provider: Arc<dyn WorkspaceSnapshotProvider>) -> Self {
        self.snapshot_provider = Some(provider);
        self
    }

    /// Override the long-term memory port (tests/composition).
    pub fn with_memory_source(mut self, source: Arc<dyn PromptMemorySource>) -> Self {
        self.memory_source = Some(source);
        self
    }

    pub async fn run(&self, schedule: &AssistantSchedule, cancel: CancellationToken) -> WakeReport {
        self.run_scheduled(schedule, cancel, None).await
    }

    /// Forced full Turn for an operator/API reconcile.
    ///
    /// Reconcile and ordinary scheduled wake share [`Self::run_scheduled`] and
    /// the same full-Turn function; only `forceFullReason` differs, so the
    /// history/model/tool path is never duplicated.
    pub async fn run_reconcile(&self, schedule: &AssistantSchedule, cancel: CancellationToken) -> WakeReport {
        self.run_scheduled(schedule, cancel, Some(FORCE_FULL_REASON_RECONCILE))
            .await
    }

    async fn run_scheduled(
        &self,
        schedule: &AssistantSchedule,
        cancel: CancellationToken,
        force_full_reason: Option<&str>,
    ) -> WakeReport {
        if !schedule.enabled {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        }
        let tasks = self
            .db
            .list_heartbeat_tasks(&schedule.workspace_id)
            .await
            .unwrap_or_default();
        let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(&schedule.workspace_id);
        let skills = crate::domains::agent::pool::load_workspace_skills(&ws_dir);
        let inspection = skills.iter().find(|skill| skill.name == "inspection");
        let Some(message) = build_wake_message(schedule, inspection, &tasks) else {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        };
        // One Session Events read per wake: the same capture supplies the
        // preflight user cursor and the prompt's semantic events.
        let session_key = format!("agent:{}:{}/main", schedule.workspace_id, schedule.assistant_id);
        let (semantic_events, control_cursor) = match self
            .load_wake_session_events(&schedule.workspace_id, &session_key)
            .await
        {
            Ok(capture) => (capture.events, Some(capture.control_cursor)),
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    what = %error.what,
                    workspace_id = %schedule.workspace_id,
                    "failed to load semantic session events; fenced full turn"
                );
                (Vec::new(), None)
            }
        };
        // One shared registry/capture per wake survives until the Turn ends;
        // the same decision serves the durable skip audit and the full-Turn
        // baseline. A configured workspace always yields an observation, even
        // when the cursor/capture failed, so the Turn is fenced.
        let observation = self
            .observe_suppression(&schedule.workspace_id, &session_key, control_cursor)
            .await;
        let prompt = match self
            .wake_prompt(
                &schedule.workspace_id,
                &schedule.assistant_id,
                message,
                &semantic_events,
            )
            .await
        {
            Ok(prompt) => prompt,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        let agent = match ensure_agent(
            &self.pool,
            &schedule.assistant_id,
            &schedule.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
            &prompt,
        )
        .await
        {
            Ok(agent) => agent,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        self.run_scheduled_with_agent(
            schedule,
            agent,
            &prompt.turn_input,
            cancel,
            observation.as_ref(),
            force_full_reason,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn run_with_agent(
        &self,
        schedule: &AssistantSchedule,
        agent: AgentLoopHandle,
        message: &str,
        cancel: CancellationToken,
    ) -> WakeReport {
        self.run_scheduled_with_agent(schedule, agent, message, cancel, None, None)
            .await
    }

    async fn run_scheduled_with_agent(
        &self,
        schedule: &AssistantSchedule,
        agent: AgentLoopHandle,
        message: &str,
        cancel: CancellationToken,
        suppression: Option<&SuppressionObservation>,
        force_full_reason: Option<&str>,
    ) -> WakeReport {
        self.run_session_with_agent(
            SessionOrigin {
                workspace_id: &schedule.workspace_id,
                assistant_id: &schedule.assistant_id,
                kind: "scheduled",
                trigger: "schedule",
                source_id: None,
                planned_wake_at: schedule.next_wake_at,
                suppression,
                force_full_reason,
                prepared: None,
            },
            agent,
            message,
            schedule.permission_mode,
            cancel,
        )
        .await
    }

    /// 告警等外部事件 wake：与定时 wake 共用生命周期事件，kind 为 `event`。
    ///
    /// 暂停态（`schedule.enabled == false`）的工作区不跑自主 Turn——告警仍
    /// 进入 Session Loop 的准入层，但在暂停态下被跳过。无 schedule 行时先
    /// ensure（继承 legacy autonomy mapping），确保 legacy `off` 工作区不会
    /// 在一次事件唤醒中恢复；ensure 失败按 fail-closed 跳过。
    pub async fn run_event(&self, wake: &EventWake, cancel: CancellationToken) -> WakeReport {
        let workspace_id = &wake.workspace_id;
        let schedule = match self
            .db
            .ensure_assistant_schedule(workspace_id, &assistant_agent_id(workspace_id), chrono::Utc::now())
            .await
        {
            Ok(schedule) => schedule,
            Err(error) => {
                tracing::warn!(%error, %workspace_id, "event wake: ensure schedule failed");
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Skipped,
                };
            }
        };
        if !schedule.enabled {
            return WakeReport {
                session_key: String::new(),
                outcome: WakeOutcome::Skipped,
            };
        }
        let permission_mode = schedule.permission_mode;
        let message = match wake.trigger.as_str() {
            "watch" => format!("收到设备值守命中：{}\n请评估影响；涉及写设备必须走审批。", wake.summary),
            "thing_event" => format!(
                "收到设备事件：{}\n请判断是否需要处理；涉及写设备必须走审批。",
                wake.summary
            ),
            _ => format!(
                "收到告警：{}\n请按「应急预案」技能（如未加载则按默认职责）评估影响、给出处置建议；涉及写设备必须走审批。",
                wake.summary
            ),
        };
        let assistant_id = assistant_agent_id(&wake.workspace_id);
        let session_key = format!("agent:{}:{assistant_id}/main", wake.workspace_id);
        let semantic_events = match self.load_wake_session_events(&wake.workspace_id, &session_key).await {
            Ok(capture) => capture.events,
            Err(error) => {
                tracing::warn!(
                    code = %error.code,
                    what = %error.what,
                    workspace_id = %wake.workspace_id,
                    "failed to load semantic session events for event wake"
                );
                Vec::new()
            }
        };
        let prompt = match self
            .wake_prompt(&wake.workspace_id, &assistant_id, message, &semantic_events)
            .await
        {
            Ok(prompt) => prompt,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        let agent = match ensure_agent(
            &self.pool,
            &assistant_id,
            &wake.workspace_id,
            &AgentRuntimeConfig::default(),
            true,
            &prompt,
        )
        .await
        {
            Ok(agent) => agent,
            Err(error) => {
                return WakeReport {
                    session_key: String::new(),
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        self.run_event_with_agent(wake.clone(), agent, &prompt.turn_input, permission_mode, cancel)
            .await
    }

    pub(crate) async fn run_event_with_agent(
        &self,
        wake: EventWake,
        agent: AgentLoopHandle,
        message: &str,
        permission_mode: PermissionMode,
        cancel: CancellationToken,
    ) -> WakeReport {
        let assistant_id = assistant_agent_id(&wake.workspace_id);
        self.run_session_with_agent(
            SessionOrigin {
                workspace_id: &wake.workspace_id,
                assistant_id: &assistant_id,
                kind: "event",
                trigger: &wake.trigger,
                source_id: Some(wake.source_id.clone()),
                planned_wake_at: None,
                suppression: None,
                force_full_reason: None,
                prepared: None,
            },
            agent,
            message,
            permission_mode,
            cancel,
        )
        .await
    }

    async fn reseed_session_history(
        &self,
        agent: &dyn tinyiothub_agent::port::runtime::AgentLoop,
        workspace_id: &str,
        session_key: &str,
    ) -> anyhow::Result<()> {
        let filter = tinyiothub_storage::harness_event::EventFilter {
            session_id: Some(session_key.to_string()),
            run_id: None,
            task_id: None,
            execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
        };
        let mut messages = Vec::new();
        let mut after = 0;
        loop {
            let page = self.db.session_event_page(workspace_id, &filter, after, 200).await?;
            let Some(last) = page.events.last() else {
                break;
            };
            after = last.seq;
            for event in page.events {
                if event.event_type != "message.created" {
                    continue;
                }
                let Some(role) = event.payload_json["role"].as_str() else {
                    continue;
                };
                if role != "user" && role != "assistant" {
                    continue;
                }
                let Some(text) = event.payload_json["text"].as_str() else {
                    continue;
                };
                messages.push(tinyiothub_agent::port::provider::ChatMessage {
                    role: role.to_string(),
                    content: text.to_string(),
                    attachments: vec![],
                });
            }
            if after >= page.high_watermark {
                break;
            }
        }
        agent.clear_history().await;
        agent.seed_history(&messages).await
    }

    async fn run_session_with_agent(
        &self,
        origin: SessionOrigin<'_>,
        agent: AgentLoopHandle,
        message: &str,
        permission_mode: PermissionMode,
        cancel: CancellationToken,
    ) -> WakeReport {
        let session_key = format!("agent:{}:{}/main", origin.workspace_id, origin.assistant_id);
        if let Err(error) = self
            .db
            .ensure_session(&session_key, origin.workspace_id, origin.assistant_id)
            .await
        {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed {
                    error: error.to_string(),
                },
            };
        }
        let mut session = match self.db.get_session(&session_key).await {
            Ok(Some(session)) => session,
            Ok(None) => Session::new(
                session_key.clone(),
                origin.workspace_id.to_string(),
                origin.assistant_id.to_string(),
            )
            .with_kind(origin.kind),
            Err(error) => {
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
        };
        session.kind = origin.kind.to_string();
        if session.label.is_none() {
            session.label = Some("大脑".to_string());
        }
        session.status = "active".to_string();
        session.touch();
        if let Err(error) = self.db.update_session(&session).await {
            return WakeReport {
                session_key,
                outcome: WakeOutcome::Failed {
                    error: error.to_string(),
                },
            };
        }
        // A planned scheduled wake owns a stable wakeId derived from
        // `(workspaceId, plannedWakeAt)` and an attempt-derived turnId. Direct
        // and event wakes keep their previous per-invocation identity.
        let planned_wake = origin
            .planned_wake_at
            .map(|planned_wake_at| PlannedWake::planned(origin.workspace_id, session_key.clone(), planned_wake_at));

        // The skip/replay decision happens before any lifecycle write: the
        // committed skip writes the complete six-event lifecycle atomically, so
        // the wake must not append its first three events separately.
        //
        // A fenced observation (capture/cursor unavailable) never attempts a
        // skip. A changed-source replay carries its linkage into the full Turn.
        let mut replay_link: Option<ReplayLink> = None;
        if let (Some(wake), Some(observation)) = (planned_wake.as_ref(), origin.suppression)
            && origin.force_full_reason.is_none()
            && origin.prepared.is_none()
            && !observation.skip_disabled
            && let Some(decision) = observation.decision()
            && let Some(report) = self
                .try_suppression_skip(&session, wake, observation, decision, &mut replay_link)
                .await
        {
            return report;
        }

        // A full Turn durably fences the old baseline before any agent work.
        // Only a completely persisted, all-Source-available successful Turn
        // may clear it.
        let mut full_attempt: Option<FullAttempt> = None;
        let attempt = match origin.prepared {
            Some(prepared) => {
                full_attempt = Some((*prepared.attempt).clone());
                Some((*prepared.wake_attempt).clone())
            }
            None => match (planned_wake.as_ref(), origin.suppression) {
                (Some(wake), Some(observation)) => {
                    match self.db.begin_full_attempt(&session_key, &observation.state).await {
                        Ok(durable) => {
                            let attempt = wake.attempt(durable.attempt_no);
                            full_attempt = Some(durable);
                            Some(attempt)
                        }
                        Err(error) => {
                            tracing::warn!(%error, %session_key, "suppression: begin full attempt failed");
                            return WakeReport {
                                session_key,
                                outcome: WakeOutcome::Failed {
                                    error: error.to_string(),
                                },
                            };
                        }
                    }
                }
                // Only a genuinely unconfigured/off wake uses the predicted
                // identity: configured workspaces always have an observation
                // and therefore a durable allocation.
                (Some(wake), None) => Some(wake.attempt(self.next_attempt_no(origin.workspace_id, &session_key).await)),
                (None, _) => None,
            },
        };
        if let Some(reason) = origin.force_full_reason {
            tracing::info!(%session_key, reason, "suppression: forced full Turn");
        }
        let context = wake_context(
            &origin,
            session_key.clone(),
            permission_mode,
            attempt.as_ref().map(|attempt| attempt.turn_id.clone()),
        );
        let wake_event_id = attempt
            .as_ref()
            .map(|attempt| attempt.wake_event_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        // scheduled 无显式 source：与本轮 wake.dispatched.wakeEventId 保持同一
        // 身份；event 使用告警 id。
        let trigger_source_id = origin.source_id.unwrap_or_else(|| wake_event_id.clone());
        let mut events_healthy = true;
        events_healthy &= self
            .emit_lifecycle(
                &context,
                attempt.as_ref(),
                "trigger.received",
                json!({ "trigger": origin.trigger, "sourceId": trigger_source_id }),
            )
            .await;
        let mut dispatched = json!({ "wakeEventId": &wake_event_id });
        if let Some(link) = &replay_link {
            dispatched["previousAttemptId"] = json!(&link.previous_attempt_id);
            dispatched["supersedes"] = json!(link.supersedes);
        }
        events_healthy &= self
            .emit_lifecycle(&context, attempt.as_ref(), "wake.dispatched", dispatched)
            .await;
        events_healthy &= self
            .emit_lifecycle(
                &context,
                attempt.as_ref(),
                "session.started",
                session_started_payload(&session_key, origin.kind),
            )
            .await;

        let turn = {
            let guard = agent.lock().await;
            // The shared loop must contain only this Assistant session's history.
            if let Err(error) = self
                .reseed_session_history(&*guard, origin.workspace_id, &session_key)
                .await
            {
                drop(guard);
                if let Some(durable) = full_attempt.as_ref() {
                    self.fail_full_attempt(origin.workspace_id, durable, "history_seed_failed")
                        .await;
                }
                return WakeReport {
                    session_key,
                    outcome: WakeOutcome::Failed {
                        error: error.to_string(),
                    },
                };
            }
            let turn = Some(
                self.runner
                    .run_turn_tracked(
                        &*guard,
                        &context,
                        message,
                        None,
                        Some(TurnCancellation::schedule(cancel.clone())),
                        attempt.as_ref(),
                        replay_link.as_ref().map(|link| link.previous_attempt_id.as_str()),
                    )
                    .await,
            );
            drop(guard);
            turn
        };
        let (mut outcome, turn_health, turn_counts) = match turn {
            Some((turn, health, counts)) => (outcome_with_todos(turn, &self.todos.get(&session_key)), health, counts),
            None => (WakeOutcome::Cancelled, Default::default(), TurnCounts::default()),
        };
        // Missing lifecycle/terminal events are failed evidence, never a
        // successful Turn. This decides the reported outcome *and* the
        // `session.ended` payload, so a persisted event never disagrees with
        // the report.
        if (!events_healthy || !turn_health.is_healthy()) && matches!(outcome, WakeOutcome::Completed) {
            outcome = WakeOutcome::Failed {
                error: "session event write failed".to_string(),
            };
        }
        let session_ended_written = self
            .emit_lifecycle(
                &context,
                attempt.as_ref(),
                "session.ended",
                json!({ "sessionId": &session_key, "outcome": outcome.as_str() }),
            )
            .await;
        if !session_ended_written && matches!(outcome, WakeOutcome::Completed) {
            outcome = WakeOutcome::Failed {
                error: "session event write failed".to_string(),
            };
        }

        let mut ended = session;
        ended.status = "ended".to_string();
        ended.touch();
        if let Err(error) = self.db.update_session(&ended).await {
            tracing::warn!(%error, %session_key, "failed to mark wake session ended");
        }

        if let (Some(durable), Some(observation)) = (full_attempt.as_ref(), origin.suppression) {
            let events_ok = events_healthy && turn_health.is_healthy() && session_ended_written;
            outcome = self
                .settle_full_attempt(
                    origin.workspace_id,
                    durable,
                    observation,
                    attempt.as_ref(),
                    outcome,
                    events_ok,
                )
                .await;
            if let Some(decision) = observation.decision() {
                self.record_turn_observation(
                    origin.workspace_id,
                    &session_key,
                    &context.turn_id,
                    &durable.attempt_id,
                    observation,
                    decision,
                    turn_counts,
                )
                .await;
            }
        }
        // A previous committed skip may have left projection/notification
        // barriers that no later skip can clear (begin_skip_attempt requires
        // all flags clear). Full Turns do not clear them, so reconcile the
        // committed skip's consumers after any suppression-aware Turn.
        if origin.suppression.is_some() {
            let alerts = self
                .suppression_recovery()
                .reconcile_committed_consumers(origin.workspace_id, &session_key)
                .await;
            for alert in &alerts {
                self.emit_suppression_alert(&session_key, alert).await;
            }
        }
        WakeReport { session_key, outcome }
    }

    /// Decide and perform a durable skip for one planned wake.
    ///
    /// Returns `Some(report)` when this wake is settled as a skip (committed
    /// now or replayed from a previous commit), `None` when a full Turn must
    /// run. A shadow decision never reaches the skip path: the session-event
    /// capture is reused and `ensure_skip_allowed`/`begin_skip_attempt` are the
    /// durable gates.
    ///
    /// A changed-source replay fills `replay_link` so the new full attempt's
    /// event stream names the committed skip it supersedes.
    async fn try_suppression_skip(
        &self,
        session: &Session,
        wake: &PlannedWake,
        observation: &SuppressionObservation,
        decision: &PreflightDecision,
        replay_link: &mut Option<ReplayLink>,
    ) -> Option<WakeReport> {
        let session_key = session.session_key.clone();
        let audit = AtomicSkipAudit::new(self.db.clone());
        // Replay first: an unchanged crash replay returns the committed skip
        // without a second Turn; a changed replay links a new full attempt.
        let replay = match audit.replay(wake, decision).await {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(%error, %session_key, "suppression: skip replay failed; running a full Turn");
                return None;
            }
        };
        if replay.outcome() == "skipped" {
            if replay.committed_skip().is_none() {
                // Skip-eligible: allocate the durable audit fence and commit.
                if let Err(error) = AtomicSkipAudit::ensure_skip_allowed(decision) {
                    tracing::warn!(%error, %session_key, "suppression: decision is not skip-eligible");
                    return None;
                }
                let skip_attempt = match self.db.begin_skip_attempt(&session_key, &observation.state).await {
                    Ok(attempt) => attempt,
                    Err(error) => {
                        // No durable audit fence was written: a concurrent
                        // change (off/shadow/config/baseline) falls back to a
                        // full Turn.
                        tracing::warn!(%error, %session_key, "suppression: begin skip attempt rejected; running a full Turn");
                        return None;
                    }
                };
                if let Err(error) = audit
                    .commit(SkipAuditRequest {
                        wake,
                        attempt: &skip_attempt,
                        decision,
                        checked_at: chrono::Utc::now(),
                    })
                    .await
                {
                    // The durable audit fence must stay visible for every
                    // worker. Settle it to `audit_failed` within the bounded
                    // recovery; if even that fails, `audit_in_progress`
                    // remains and the alert still fires. TriggerRouter remains
                    // the only Schedule writer.
                    tracing::warn!(%error, %session_key, "suppression: atomic skip commit failed");
                    let settled = self
                        .suppression_recovery()
                        .settle_skip_failure(
                            &session.workspace_id,
                            &session_key,
                            &skip_attempt.attempt_id,
                            "audit_write_failed",
                        )
                        .await;
                    if !settled {
                        tracing::warn!(
                            attempt_id = %skip_attempt.attempt_id,
                            "suppression: audit failure settlement failed; the fence remains visible"
                        );
                    }
                    let alert = SuppressionAlert {
                        code: ErrorCode::AuditWriteFailed.as_str().to_string(),
                        workspace_id: session.workspace_id.clone(),
                        turn_id: Some(wake.attempt(skip_attempt.attempt_no).turn_id.clone()),
                        baseline_state_id: decision.baseline_state_id.clone(),
                        current_registry_version: observation
                            .captured
                            .as_ref()
                            .map(|captured| captured.registry.registry_version()),
                        baseline_registry_version: observation.state.registry_version.clone(),
                        vetoes: decision.vetoes.iter().map(|veto| veto.as_str().to_string()).collect(),
                        consecutive_failures: self.consecutive_failures(&session.workspace_id, &session_key).await,
                        metrics: json!({
                            "attemptNo": skip_attempt.attempt_no,
                            "fence": if settled { "audit_failed" } else { "audit_in_progress" },
                            "cause": "audit_commit_failed",
                        }),
                        trace_id: skip_attempt.attempt_id.clone(),
                    };
                    self.emit_suppression_alert(&session_key, &alert).await;
                    return Some(WakeReport {
                        session_key,
                        outcome: WakeOutcome::AuditFailed,
                    });
                }
                let turn_id = wake.attempt(skip_attempt.attempt_no).turn_id.clone();
                self.record_turn_observation(
                    &session.workspace_id,
                    &session_key,
                    &turn_id,
                    &skip_attempt.attempt_id,
                    observation,
                    decision,
                    TurnCounts::default(),
                )
                .await;
            }
            if let Some(committed) = replay.committed_skip() {
                self.record_turn_observation(
                    &session.workspace_id,
                    &session_key,
                    replay.turn_id(),
                    &committed.attempt_id,
                    observation,
                    decision,
                    TurnCounts::default(),
                )
                .await;
            }
            self.reconcile_committed_skip(session, decision).await;
            return Some(WakeReport {
                session_key,
                outcome: WakeOutcome::Skipped,
            });
        }
        // Changed replay: the new full attempt must name the committed skip.
        if let Some(previous) = replay.previous_attempt_id() {
            *replay_link = Some(ReplayLink {
                previous_attempt_id: previous.to_string(),
                supersedes: replay.supersedes,
            });
        }
        None
    }

    /// Reconcile the non-Schedule side effects of a committed skip.
    ///
    /// Schedule stays the exclusive responsibility of `TriggerRouter`: after
    /// its `mark_assistant_wake` succeeds it clears `SyncPending::Schedule`
    /// with the session key from this wake's report. A failed projection write
    /// keeps its fence; the committed workspace events are the SSE source of
    /// truth (clients resume from the DB high watermark).
    async fn reconcile_committed_skip(&self, session: &Session, decision: &PreflightDecision) {
        let session_key = &session.session_key;
        // Schedule stays the TriggerRouter's exclusive write. Projection and
        // notification are reconciled here; a persistent notification failure
        // keeps its barrier and alerts instead of rolling anything back.
        let alerts = self
            .suppression_recovery()
            .reconcile_committed_consumers(&session.workspace_id, session_key)
            .await;
        for alert in &alerts {
            self.emit_suppression_alert(session_key, alert).await;
        }
        tracing::info!(
            %session_key,
            baseline_state_id = decision.baseline_state_id.as_deref().unwrap_or(""),
            config_version = decision.config_version,
            alerts = alerts.len(),
            "suppression: committed skip reconciled"
        );
    }

    /// Install the baseline only for a fully successful, completely persisted
    /// Turn with every Source available; otherwise settle `full_failed`.
    async fn settle_full_attempt(
        &self,
        workspace_id: &str,
        durable: &FullAttempt,
        observation: &SuppressionObservation,
        attempt: Option<&WakeAttempt>,
        outcome: WakeOutcome,
        events_ok: bool,
    ) -> WakeOutcome {
        // Off-mode manual turns have no Source capture by design. They still
        // allocated a durable attempt for the HTTP turnId, so settle it without
        // a baseline and keep the old baseline ineligible.
        if observation.settle_without_baseline {
            let settled = match &outcome {
                WakeOutcome::Completed => "full_completed",
                WakeOutcome::Skipped => "skipped",
                WakeOutcome::Partial | WakeOutcome::Cancelled | WakeOutcome::Failed { .. } => "full_failed",
                WakeOutcome::AuditFailed => "full_failed",
            };
            if let Err(error) = self
                .db
                .settle_full_attempt_without_baseline(workspace_id, &durable.attempt_id, settled)
                .await
            {
                tracing::warn!(%error, %durable.attempt_id, "suppression: baseline-less settlement failed");
            }
            return outcome;
        }
        let captured = observation.captured.as_ref();
        let all_sources_available = captured.is_some_and(|captured| {
            !captured.decision.snapshot.is_empty()
                && captured.decision.snapshot.iter().all(|snapshot| snapshot.available)
        });
        if events_ok
            && all_sources_available
            && matches!(outcome, WakeOutcome::Completed)
            && let Some(captured) = captured
        {
            let state_id = format!("state:{}", durable.attempt_id);
            let turn_id = attempt.map_or("", |attempt| attempt.turn_id.as_str());
            let completion = FullAttemptCompletion {
                state_id: state_id.clone(),
                state_version: TURN_STATE_VERSION,
                registry_version: captured.registry.registry_version(),
                config_version: captured.decision.config_version,
                control_cursor: observation.control_cursor,
                turn_state: baseline_turn_state(&state_id, turn_id, observation, captured),
                observed_at: observation.observed_at,
            };
            match self
                .db
                .complete_full_attempt(workspace_id, &durable.attempt_id, &completion)
                .await
            {
                Ok(_) => outcome,
                Err(error) => {
                    tracing::warn!(%error, %durable.attempt_id, "suppression: baseline installation failed");
                    self.fail_full_attempt(workspace_id, durable, "baseline_write_failed")
                        .await;
                    let alert = SuppressionAlert {
                        code: ErrorCode::BaselineCorrupt.as_str().to_string(),
                        workspace_id: workspace_id.to_string(),
                        turn_id: attempt.map(|attempt| attempt.turn_id.clone()),
                        baseline_state_id: captured.decision.baseline_state_id.clone(),
                        current_registry_version: Some(captured.registry.registry_version()),
                        baseline_registry_version: observation.state.registry_version.clone(),
                        vetoes: captured
                            .decision
                            .vetoes
                            .iter()
                            .map(|veto| veto.as_str().to_string())
                            .collect(),
                        consecutive_failures: self.consecutive_failures(workspace_id, &durable.session_id).await,
                        metrics: json!({ "fence": "baseline_write_failed" }),
                        trace_id: durable.attempt_id.clone(),
                    };
                    self.emit_suppression_alert(&durable.session_id, &alert).await;
                    WakeOutcome::Failed {
                        error: format!("baseline installation failed: {error}"),
                    }
                }
            }
        } else {
            let reason = if captured.is_none() {
                observation
                    .unavailable_reason
                    .as_deref()
                    .unwrap_or("preflight_unavailable")
            } else if !events_ok {
                "event_write_failed"
            } else if !all_sources_available {
                "source_unavailable"
            } else {
                match &outcome {
                    WakeOutcome::Partial => "partial",
                    WakeOutcome::Cancelled => "cancelled",
                    WakeOutcome::Failed { .. } => "failed",
                    WakeOutcome::Completed => "unknown_outcome",
                    WakeOutcome::Skipped | WakeOutcome::AuditFailed => "unknown_outcome",
                }
            };
            self.fail_full_attempt(workspace_id, durable, reason).await;
            outcome
        }
    }

    async fn fail_full_attempt(&self, workspace_id: &str, durable: &FullAttempt, reason: &str) {
        if let Err(error) = self
            .db
            .fail_full_attempt(workspace_id, &durable.attempt_id, reason)
            .await
        {
            // The in-progress fence remains visible to every worker.
            tracing::warn!(%error, %reason, %durable.attempt_id, "suppression: settle full attempt failed");
        }
    }

    /// Next monotonic attempt number. Durable allocation happens in
    /// `begin_full_attempt`/`begin_skip_attempt`; reading the current row keeps
    /// the lifecycle identity aligned with the attempt those CAS operations
    /// will grant.
    async fn next_attempt_no(&self, workspace_id: &str, session_key: &str) -> i64 {
        match self.db.load_suppression_state(workspace_id, session_key).await {
            Ok(state) => state.map(|state| state.attempt_no + 1).unwrap_or(1),
            Err(error) => {
                tracing::warn!(
                    %error,
                    %workspace_id,
                    "failed to load suppression attempt number; using the first attempt"
                );
                1
            }
        }
    }

    /// Emit one wake lifecycle event and report whether it persisted.
    ///
    /// The boolean is event-write health evidence: a full Turn with a missing
    /// lifecycle event may not install a baseline.
    async fn emit_lifecycle(
        &self,
        context: &TurnContext,
        attempt: Option<&WakeAttempt>,
        event_type: &str,
        payload: Value,
    ) -> bool {
        let input = match attempt {
            Some(attempt) => attempt_event(context, attempt, event_type, payload),
            None => session_event(context, event_type, payload),
        };
        if let Err(error) = self.events.emit(input).await {
            tracing::warn!(%error, %event_type, "failed to persist wake session event");
            false
        } else {
            true
        }
    }
}

/// The immutable pre-turn state envelope stored with a successful baseline.
///
/// `decision.snapshot` is the same capture the skip comparator used, so the
/// baseline describes exactly the Source state this Turn started from.
fn baseline_turn_state(
    state_id: &str,
    turn_id: &str,
    observation: &SuppressionObservation,
    captured: &CapturedPreflight,
) -> Value {
    json!({
        "stateId": state_id,
        "stateVersion": TURN_STATE_VERSION,
        "turnId": turn_id,
        "observedAt": observation.observed_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "sources": captured
            .decision
            .snapshot
            .iter()
            .map(|source| {
                json!({
                    "sourceId": source.source_id,
                    "sourceVersion": source.source_version,
                    "revision": source.revision,
                    "stateHash": source.state_hash,
                    "available": source.available,
                    "empty": source.empty,
                })
            })
            .collect::<Vec<_>>(),
        "controlCursor": observation.control_cursor,
        "registryVersion": captured.registry.registry_version(),
        "mode": captured.decision.mode.as_str(),
    })
}

#[async_trait::async_trait]
impl WakeExecutor for WakeRunner {
    async fn execute(&self, schedule: AssistantSchedule, cancel: CancellationToken) -> WakeReport {
        self.run(&schedule, cancel).await
    }

    async fn execute_event(&self, wake: EventWake, cancel: CancellationToken) -> WakeReport {
        self.run_event(&wake, cancel).await
    }

    async fn recover_suppression(&self, workspace_id: &str) -> Vec<RecoveryReport> {
        WakeRunner::recover_suppression(self, workspace_id).await
    }

    async fn manual_preflight(&self, workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        WakeRunner::manual_preflight(self, workspace_id).await
    }

    async fn prepare_manual(
        &self,
        schedule: &AssistantSchedule,
        operation: ManualOperation,
    ) -> Result<PreparedManual, ManualStartError> {
        WakeRunner::prepare_manual(self, schedule, operation).await
    }

    async fn execute_manual(&self, prepared: PreparedManual, cancel: CancellationToken) -> WakeReport {
        WakeRunner::execute_manual(self, prepared, cancel).await
    }

    async fn record_confirmed_false_skip(
        &self,
        workspace_id: &str,
        turn_id: &str,
        reviewer: &str,
        root_cause: &str,
    ) -> Result<(), ManualStartError> {
        WakeRunner::record_confirmed_false_skip(self, workspace_id, turn_id, reviewer, root_cause)
            .await
            .map_err(ManualStartError::Unavailable)
    }
}

/// 构造 wake Turn 的上下文。scheduled / event 两个入口共用，确保 schedule
/// 的 `permission_mode` 一定进入 `TurnContext`（`PermissionGuardTool` 读它做
/// 工具层写拒绝）。
fn wake_context(
    origin: &SessionOrigin<'_>,
    session_key: String,
    permission_mode: PermissionMode,
    turn_id: Option<String>,
) -> TurnContext {
    let mut context = TurnContext::new(
        origin.workspace_id.to_string(),
        origin.assistant_id.to_string(),
        session_key,
        Uuid::new_v4().to_string(),
        0,
    )
    .with_permission_mode(permission_mode);
    if let Some(turn_id) = turn_id {
        context.turn_id = turn_id;
    }
    context
}

fn outcome_with_todos(outcome: TurnOutcome, todos: &[Todo]) -> WakeOutcome {
    match outcome {
        TurnOutcome::Completed { .. } if todos.iter().any(|todo| todo.status != TodoStatus::Completed) => {
            WakeOutcome::Partial
        }
        TurnOutcome::Completed { .. } => WakeOutcome::Completed,
        TurnOutcome::Cancelled { .. } => WakeOutcome::Cancelled,
        TurnOutcome::Failed { error } => WakeOutcome::Failed { error },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::prompt::PromptMemorySource;
    use crate::domains::agent::session::context::TurnContextRegistry;
    use crate::domains::agent::session::events::SessionEventSink;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::runner::SessionRunner;
    use crate::domains::agent::session::suppression::preflight::{PreflightDecision, SuppressionService};
    use crate::domains::agent::session::suppression::registry::{
        self, SnapshotProviderSlot, SourceContext, SourceMode,
    };
    use crate::domains::agent::session::todos::TodoStore;
    use crate::domains::agent::session::types::{Todo, TodoStatus};
    use chrono::TimeZone;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tinyiothub_agent::config::AgentRuntimeConfig;
    use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
    use tinyiothub_agent::port::memory::NoopMemory;
    use tinyiothub_agent::port::observer::NoopObserver;
    use tinyiothub_agent::port::provider::{ChatMessage, ChatRequest, ChatResponse, ModelProvider};
    use tinyiothub_core::harness::{ExecutionMode, HarnessEvent, HarnessEventInput};
    use tinyiothub_core::heartbeat::HeartbeatTask;
    use tinyiothub_core::memory::AgentMemory;
    use tinyiothub_storage::assistant_schedule::AssistantSchedule;
    use tinyiothub_storage::error::DbError;
    use tinyiothub_storage::harness_event::EventFilter;
    use tinyiothub_storage::suppression::{SuppressionMode, SuppressionTurnObservation};
    use tokio::sync::mpsc;

    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("巡检完成".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }

        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    #[derive(Default)]
    struct RecordingWakeState {
        seeds: Vec<Vec<ChatMessage>>,
        inputs: Vec<String>,
    }

    struct RecordingWakeLoop {
        state: Arc<tokio::sync::Mutex<RecordingWakeState>>,
    }

    #[async_trait::async_trait]
    impl tinyiothub_agent::port::runtime::AgentLoop for RecordingWakeLoop {
        async fn turn_streamed(
            &self,
            user_message: &str,
            _event_tx: mpsc::Sender<tinyiothub_agent::port::events::TurnEvent>,
            _cancel_token: Option<CancellationToken>,
        ) -> anyhow::Result<String> {
            self.state.lock().await.inputs.push(user_message.into());
            Ok("done".into())
        }

        async fn run_single(&self, message: &str) -> anyhow::Result<String> {
            self.state.lock().await.inputs.push(message.into());
            Ok("done".into())
        }

        async fn clear_history(&self) {}

        async fn seed_history(&self, messages: &[ChatMessage]) -> anyhow::Result<()> {
            self.state.lock().await.seeds.push(messages.to_vec());
            Ok(())
        }
    }

    fn task(text: &str, paused: bool) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws".into(),
            priority: "low".into(),
            text: text.into(),
            paused,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn test_prompt() -> crate::domains::agent::prompt::assistant::PromptPackage {
        crate::domains::agent::prompt::assistant::PromptPackage {
            system_prompt: "test-prompt".into(),
            turn_input: String::new(),
        }
    }

    fn schedule(prompt: &str) -> AssistantSchedule {
        AssistantSchedule {
            workspace_id: "ws".into(),
            assistant_id: assistant_agent_id("ws"),
            interval_seconds: 300,
            enabled: true,
            permission_mode: PermissionMode::Ask,
            prompt: prompt.into(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        }
    }

    async fn fixture() -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        fixture_with(Arc::new(|| Ok(Box::new(ScriptedProvider)))).await
    }

    async fn fixture_with(
        provider_factory: tinyiothub_agent::provider::ProviderFactory,
    ) -> (WakeRunner, Arc<AgentPool>, Arc<RecordingSink>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        // Each test gets its own workspace-store root; the default root is
        // shared repo state, so parallel tests would race harness migrations.
        let db = Arc::new(tinyiothub_storage::Db::new(pool).with_workspace_store_root(
            std::env::temp_dir().join(format!("tinyiothub-wake-store-{}", uuid::Uuid::new_v4())),
        ));
        tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws', 'workspace', 'tenant-default-001', 'now', 'now')")
            .execute(db.pool())
            .await
            .unwrap();
        db.create_workspace_store("ws").await.unwrap();
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                provider_factory,
            )
            .unwrap(),
        );
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink.clone(), contexts));
        let todos = Arc::new(TodoStore::default());
        let wake = WakeRunner::from_parts(db.clone(), agent_pool.clone(), sink.clone(), todos, runner);
        (wake, agent_pool, sink)
    }

    #[test]
    fn wake_message_uses_custom_prompt_first() {
        let message = build_wake_message(&schedule("检查 3 号楼"), None, &[task("离线事件", false)]);
        assert_eq!(message.as_deref(), Some("检查 3 号楼"));
    }

    #[test]
    fn wake_message_lists_active_tasks_and_skips_all_paused() {
        let message =
            build_wake_message(&schedule(""), None, &[task("离线事件", false), task("设备异常", true)]).unwrap();
        assert!(message.contains("- 离线事件"));
        assert!(!message.contains("设备异常"));
        assert!(build_wake_message(&schedule(""), None, &[task("离线事件", true)]).is_none());
        assert!(
            build_wake_message(&schedule(""), None, &[])
                .unwrap()
                .contains("默认职责")
        );
    }

    #[test]
    fn wake_message_prefers_inspection_skill_over_tasks() {
        let skill = tinyiothub_skills::LoadedSkill {
            name: "inspection".into(),
            title: "巡检".into(),
            description: String::new(),
            version: "1".into(),
            inject: tinyiothub_skills::InjectMode::Trigger,
            trigger: Some("/inspection".into()),
            body: "# 巡检\n## 执行步骤\n1. 检查离线设备".into(),
        };
        let message = build_wake_message(&schedule(""), Some(&skill), &[task("旧任务", false)]).unwrap();
        assert!(message.contains("检查离线设备"));
        assert!(!message.contains("旧任务"));
        let custom = build_wake_message(&schedule("自定义"), Some(&skill), &[task("旧任务", false)]).unwrap();
        assert_eq!(custom, "自定义");
    }

    #[test]
    fn outcome_with_todos_marks_partial() {
        let incomplete = vec![Todo {
            content: "巡检".into(),
            status: TodoStatus::InProgress,
            active_form: "巡检中".into(),
        }];
        assert_eq!(
            outcome_with_todos(
                TurnOutcome::Completed {
                    text: String::new(),
                    tool_calls: 0
                },
                &incomplete
            ),
            WakeOutcome::Partial
        );
        assert_eq!(
            outcome_with_todos(
                TurnOutcome::Completed {
                    text: String::new(),
                    tool_calls: 0
                },
                &[]
            ),
            WakeOutcome::Completed
        );
    }

    #[tokio::test]
    async fn wake_reseeds_only_current_session_history() {
        let (wake, _agent_pool, _sink) = fixture().await;
        let assistant_id = assistant_agent_id("ws");
        let session_key = format!("agent:ws:{assistant_id}/main");
        for (session, text) in [
            (session_key.as_str(), "current-session-marker"),
            ("agent:ws:other/other", "other-session-marker"),
        ] {
            let context = TurnContext::new("ws".into(), assistant_id.clone(), session.into(), "old-run".into(), 0);
            wake.db
                .append_session_event(&session_event(
                    &context,
                    "message.created",
                    json!({ "messageId": uuid::Uuid::new_v4().to_string(), "role": "user", "text": text }),
                ))
                .await
                .unwrap();
        }
        let state = Arc::new(tokio::sync::Mutex::new(RecordingWakeState::default()));
        let agent = Arc::new(tokio::sync::Mutex::new(RecordingWakeLoop { state: state.clone() })) as AgentLoopHandle;

        let report = wake
            .run_with_agent(&schedule(""), agent, "next-turn", CancellationToken::new())
            .await;

        assert_eq!(report.outcome, WakeOutcome::Completed);
        let state = state.lock().await;
        let seeded = state.seeds.iter().flatten().collect::<Vec<_>>();
        assert!(
            seeded
                .iter()
                .any(|message| message.content.contains("current-session-marker"))
        );
        assert!(
            seeded
                .iter()
                .all(|message| !message.content.contains("other-session-marker"))
        );
        assert_eq!(state.inputs, vec!["next-turn"]);
    }

    #[tokio::test]
    async fn wake_prompt_uses_injected_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("AGENTS.md"), "configured-workspace-marker").unwrap();
        let (mut wake, _agent_pool, _sink) = fixture().await;
        wake.system_prompts = Arc::new(tinyiothub_core::config::SystemPromptsConfig {
            workspace_dir: root.path().display().to_string(),
            ..Default::default()
        });

        let package = wake.wake_prompt("ws", "assistant", "turn".into(), &[]).await.unwrap();
        assert!(package.system_prompt.contains("configured-workspace-marker"));
    }

    #[tokio::test]
    async fn wake_prompt_injects_short_term_snapshot_and_semantic_events_once() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("SHORT_TERM.md"), "short-term-wake-marker").unwrap();

        let (mut wake, _agent_pool, _sink) = fixture().await;
        wake.system_prompts = Arc::new(tinyiothub_core::config::SystemPromptsConfig {
            workspace_dir: root.path().display().to_string(),
            ..Default::default()
        });
        let snapshot = tinyiothub_runtime::watch::live_state::WorkspaceSnapshot {
            v: 1,
            devices: vec![tinyiothub_runtime::watch::live_state::DeviceSnapshot {
                thing_id: "device-1".into(),
                name: "wake-device-marker".into(),
                health: tinyiothub_runtime::watch::live_state::Health::Ok,
                online: true,
                zone: "zone".into(),
                values: vec![],
                properties: vec![],
            }],
            total: 1,
        };
        wake = wake.with_snapshot_provider(Arc::new(
            crate::domains::agent::session::suppression::registry::SnapshotProviderSlot::with(snapshot),
        ));

        let context = TurnContext::new(
            "ws".into(),
            assistant_agent_id("ws"),
            format!("agent:ws:{}/main", assistant_agent_id("ws")),
            "run".into(),
            0,
        );
        wake.db
            .append_session_event(&session_event(
                &context,
                "thing.event",
                json!({ "marker": "wake-event-marker" }),
            ))
            .await
            .unwrap();

        let session_key = format!("agent:ws:{}/main", assistant_agent_id("ws"));
        let capture = registry::load_session_events(&wake.db, "ws", &session_key)
            .await
            .unwrap();
        let package = wake
            .wake_prompt("ws", &assistant_agent_id("ws"), "turn".into(), &capture.events)
            .await
            .unwrap();
        assert_eq!(package.system_prompt.matches("short-term-wake-marker").count(), 1);
        assert_eq!(package.system_prompt.matches("wake-device-marker").count(), 1);
        assert_eq!(package.system_prompt.matches("wake-event-marker").count(), 1);
        assert_eq!(package.turn_input, "turn");
    }

    #[tokio::test]
    async fn wake_run_writes_lifecycle_and_ends_session() {
        let (wake, agent_pool, sink) = fixture().await;
        let schedule = schedule("");
        let agent = agent_pool
            .create(
                &schedule.assistant_id,
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let report = wake
            .run_with_agent(&schedule, agent, "按默认职责巡检", CancellationToken::new())
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let parsed = tinyiothub_agent::session::SessionKey::parse(&report.session_key)
            .expect("wake session key must be a valid agent:<ws>:<assistant>/<session> key");
        assert_eq!(parsed.workspace_id, "ws");
        assert_eq!(parsed.agent_id, assistant_agent_id("ws"));
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "step.text",
                "message.created",
                "turn.completed",
                "session.ended",
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "scheduled");
        assert_eq!(
            events[0].payload_json["sourceId"], events[1].payload_json["wakeEventId"],
            "scheduled trigger.received.sourceId must correlate with wake.dispatched.wakeEventId"
        );
        assert_eq!(events.last().unwrap().payload_json["outcome"], "completed");
        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.kind, "scheduled");
        assert_eq!(session.status, "ended");
    }

    #[tokio::test]
    async fn subsequent_wakes_reuse_the_default_assistant_session() {
        let (wake, agent_pool, _sink) = fixture().await;
        let schedule = schedule("");
        let agent = agent_pool
            .create(
                &schedule.assistant_id,
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let first = wake
            .run_with_agent(&schedule, agent.clone(), "第一次巡检", CancellationToken::new())
            .await;
        let second = wake
            .run_with_agent(&schedule, agent, "第二次巡检", CancellationToken::new())
            .await;
        assert_eq!(first.outcome, WakeOutcome::Completed);
        assert_eq!(second.outcome, WakeOutcome::Completed);
        assert_eq!(first.session_key, second.session_key);
        assert!(first.session_key.ends_with("/main"));
        let sessions = wake.db.list_sessions(Some("ws"), None, 100, 0).await.unwrap();
        assert_eq!(sessions.len(), 1, "a workspace must not create one session per wake");
    }

    #[tokio::test]
    async fn disabled_schedule_skips_wake() {
        let (wake, agent_pool, sink) = fixture().await;
        let mut schedule = schedule("");
        schedule.enabled = false;
        let report = wake.run(&schedule, CancellationToken::new()).await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(sink.events.lock().await.is_empty());
        // schedule() uses assistant_agent_id("ws"); the wake agent must not have been created
        assert_eq!(agent_pool.pool_size(), 0);
    }

    #[tokio::test]
    async fn disabled_schedule_skips_event_wake() {
        let (wake, agent_pool, sink) = fixture().await;
        wake.db
            .ensure_assistant_schedule("ws", &assistant_agent_id("ws"), chrono::Utc::now())
            .await
            .unwrap();
        wake.db
            .update_assistant_schedule("ws", Some(false), None, None, None, None)
            .await
            .unwrap();
        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(report.session_key.is_empty());
        assert!(sink.events.lock().await.is_empty());
        assert_eq!(
            agent_pool.pool_size(),
            0,
            "no agent should be created for a skipped event wake"
        );
        let sessions = wake.db.list_sessions(Some("ws"), None, 10, 0).await.unwrap();
        assert!(sessions.is_empty(), "no session row should be written");
    }

    #[tokio::test]
    async fn event_wake_without_schedule_continues_enabled() {
        // No schedule row: event wakes default to enabled (Ask) rather than
        // silently failing.
        let (wake, _agent_pool, sink) = fixture().await;
        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        assert!(!sink.events.lock().await.is_empty());
        let row = wake
            .db
            .get_assistant_schedule("ws")
            .await
            .unwrap()
            .expect("schedule ensured");
        assert!(row.enabled);
        assert_eq!(row.permission_mode, PermissionMode::Ask);
    }

    #[tokio::test]
    async fn legacy_off_event_wake_is_skipped_and_persisted_as_paused() {
        // No schedule row yet, but the workspace still has the legacy `off`
        // policy: ensure must create a paused row and the event wake must skip.
        let (wake, agent_pool, sink) = fixture().await;
        sqlx::query("INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES ('ws', 'off')")
            .execute(wake.db.pool())
            .await
            .unwrap();

        let report = wake
            .run_event(
                &EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Skipped);
        assert!(report.session_key.is_empty());
        assert!(sink.events.lock().await.is_empty());
        assert_eq!(agent_pool.pool_size(), 0, "no agent for a paused event wake");

        let row = wake
            .db
            .get_assistant_schedule("ws")
            .await
            .unwrap()
            .expect("schedule ensured");
        assert!(!row.enabled, "legacy off must persist as paused");
    }

    #[test]
    fn wake_context_carries_schedule_permission_mode() {
        let origin = SessionOrigin {
            workspace_id: "ws",
            assistant_id: "__assistant__ws",
            kind: "scheduled",
            trigger: "schedule",
            source_id: None,
            planned_wake_at: None,
            suppression: None,
            force_full_reason: None,
            prepared: None,
        };
        let context = wake_context(&origin, "agent:ws:a/s".into(), PermissionMode::ReadOnly, None);
        assert_eq!(context.workspace_id, "ws");
        assert_eq!(context.assistant_id, "__assistant__ws");
        assert_eq!(context.session_key, "agent:ws:a/s");
        assert_eq!(context.permission_mode, PermissionMode::ReadOnly);

        let ask = wake_context(&origin, "s".into(), PermissionMode::Ask, Some("turn-attempt-2".into()));
        assert_eq!(ask.permission_mode, PermissionMode::Ask);
        assert_eq!(ask.turn_id, "turn-attempt-2");
    }

    struct SleepingProvider;

    #[async_trait::async_trait]
    impl ModelProvider for SleepingProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Ok(ChatResponse {
                text: Some("too late".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for SleepingProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }

        fn alias(&self) -> &str {
            "SleepingProvider"
        }
    }

    #[tokio::test]
    async fn cancel_during_wake_ends_session_as_cancelled() {
        let (wake, _agent_pool, sink) = fixture_with(Arc::new(|| Ok(Box::new(SleepingProvider)))).await;
        let wake = Arc::new(wake);
        let cancel = CancellationToken::new();
        let handle = {
            let wake = wake.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { wake.run(&schedule(""), cancel).await })
        };

        // Wait until the turn has actually started before cancelling.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let started = sink
                .events
                .lock()
                .await
                .iter()
                .any(|event| event.event_type == "turn.started");
            if started {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "turn did not start before the deadline"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        cancel.cancel();

        let report = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("cancelled wake must finish promptly")
            .unwrap();
        assert_eq!(report.outcome, WakeOutcome::Cancelled);

        let events = sink.events.lock().await;
        assert_eq!(
            events.last().unwrap().event_type,
            "session.ended",
            "cancelled wake must still emit session.ended"
        );
        assert_eq!(events.last().unwrap().payload_json["outcome"], "cancelled");
        let terminal = events
            .iter()
            .find(|event| event.event_type == "turn.completed")
            .expect("cancelled wake must finish its turn");
        assert_eq!(terminal.payload_json["outcome"], "cancelled");
        assert_eq!(terminal.payload_json["source"], "schedule");
        assert_eq!(terminal.payload_json["error"], "调度器停止本轮执行");
        drop(events);

        let session = wake.db.get_session(&report.session_key).await.unwrap().unwrap();
        assert_eq!(session.status, "ended");
    }

    #[tokio::test]
    async fn event_wake_writes_event_session_lifecycle() {
        let (wake, agent_pool, sink) = fixture().await;
        let agent = agent_pool
            .create(
                &assistant_agent_id("ws"),
                "ws",
                &AgentRuntimeConfig::default(),
                vec![],
                true,
                &test_prompt(),
            )
            .unwrap();
        let report = wake
            .run_event_with_agent(
                EventWake {
                    workspace_id: "ws".into(),
                    trigger: "alarm".into(),
                    source_id: "alarm-1".into(),
                    summary: "温度过高".into(),
                    dedup_key: None,
                },
                agent,
                "收到告警：温度过高",
                PermissionMode::Ask,
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "step.text",
                "message.created",
                "turn.completed",
                "session.ended"
            ]
        );
        assert_eq!(events[2].payload_json["kind"], "event");
        assert_eq!(events[0].payload_json["sourceId"], "alarm-1");
    }

    // ------------------------------------------------------------------
    // Suppression lifecycle integration (Task 6)
    // ------------------------------------------------------------------

    fn lifecycle_session_key() -> String {
        format!("agent:ws:{}/main", assistant_agent_id("ws"))
    }

    struct LifecycleSink {
        fail_on: Option<String>,
        events: tokio::sync::Mutex<Vec<HarnessEventInput>>,
    }

    impl LifecycleSink {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                fail_on: None,
                events: tokio::sync::Mutex::new(Vec::new()),
            })
        }

        fn failing_on(event_type: &str) -> Arc<Self> {
            Arc::new(Self {
                fail_on: Some(event_type.to_string()),
                events: tokio::sync::Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait::async_trait]
    impl SessionEventSink for LifecycleSink {
        async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError> {
            if self.fail_on.as_deref() == Some(input.event_type.as_str()) {
                return Err(DbError::Internal("injected session event write failure".into()));
            }
            let mut events = self.events.lock().await;
            let event = HarnessEvent {
                seq: events.len() as i64 + 1,
                event_id: input.event_id.clone(),
                workspace_id: input.workspace_id.clone(),
                session_id: input.session_id.clone(),
                execution_mode: input.execution_mode,
                run_id: input.run_id.clone(),
                task_id: input.task_id.clone(),
                event_type: input.event_type.clone(),
                occurred_at: input.occurred_at,
                cause_id: input.cause_id,
                schema_v: input.schema_v,
                payload_digest: input.payload_digest.clone(),
                payload_json: input.payload_json.clone(),
            };
            events.push(input);
            Ok(event)
        }
    }

    struct EmptyMemory;

    #[async_trait::async_trait]
    impl PromptMemorySource for EmptyMemory {
        async fn list_active(&self, _workspace_id: &str, _agent_id: &str) -> anyhow::Result<Vec<AgentMemory>> {
            Ok(Vec::new())
        }

        async fn record_load(&self, _id: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct RecordingProvider {
        requests: Arc<tokio::sync::Mutex<Vec<Vec<u8>>>>,
        fail: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl ModelProvider for RecordingProvider {
        async fn chat(
            &self,
            request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            self.requests
                .lock()
                .await
                .push(serde_json::to_vec(request.messages).unwrap_or_default());
            if self.fail.load(Ordering::SeqCst) {
                anyhow::bail!("provider unavailable");
            }
            Ok(ChatResponse {
                text: Some("巡检完成".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for RecordingProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
        }

        fn alias(&self) -> &str {
            "RecordingProvider"
        }
    }

    fn empty_workspace_snapshot_provider() -> Arc<dyn WorkspaceSnapshotProvider> {
        Arc::new(SnapshotProviderSlot::with(
            tinyiothub_runtime::watch::live_state::WorkspaceSnapshot {
                v: 1,
                devices: vec![],
                total: 0,
            },
        ))
    }

    struct LifecycleFixture {
        _root: tempfile::TempDir,
        _prompts: tempfile::TempDir,
        db: Arc<Db>,
        pool: Arc<AgentPool>,
        todos: Arc<TodoStore>,
        sink: Arc<LifecycleSink>,
        wake: Arc<WakeRunner>,
        schedule: AssistantSchedule,
        requests: Arc<tokio::sync::Mutex<Vec<Vec<u8>>>>,
        fail_provider: Arc<AtomicBool>,
        snapshot_provider: Option<Arc<dyn WorkspaceSnapshotProvider>>,
    }

    async fn lifecycle_fixture(mode: SuppressionMode, fail_on: Option<&str>, with_snapshot: bool) -> LifecycleFixture {
        lifecycle_fixture_with_factory(mode, fail_on, with_snapshot, None).await
    }

    async fn lifecycle_fixture_with_factory(
        mode: SuppressionMode,
        fail_on: Option<&str>,
        with_snapshot: bool,
        provider_factory: Option<tinyiothub_agent::provider::ProviderFactory>,
    ) -> LifecycleFixture {
        let root = tempfile::tempdir().expect("fixture root");
        let prompts = tempfile::tempdir().expect("prompt root");
        std::fs::create_dir_all(prompts.path().join("ws")).expect("workspace prompt dir");
        let db = Arc::new(
            Db::new(tinyiothub_storage::test_helpers::test_pool().await)
                .with_workspace_store_root(root.path().join("stores")),
        );
        tinyiothub_storage::seed::seed_system(&db).await.expect("seed system");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws', 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .execute(db.pool())
        .await
        .expect("insert workspace");
        db.create_workspace_store("ws").await.expect("workspace store");
        let now = Utc.timestamp_opt(1_800_000_000, 0).single().expect("timestamp");
        db.ensure_assistant_schedule("ws", &assistant_agent_id("ws"), now)
            .await
            .expect("assistant schedule");
        db.update_suppression_config("ws", Some(mode), None)
            .await
            .expect("suppression config")
            .expect("schedule row");

        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let fail_provider = Arc::new(AtomicBool::new(false));
        let factory = provider_factory.unwrap_or_else(|| {
            let requests = requests.clone();
            let fail = fail_provider.clone();
            Arc::new(move || {
                Ok(Box::new(RecordingProvider {
                    requests: requests.clone(),
                    fail: fail.clone(),
                }))
            })
        });
        let sink = match fail_on {
            Some(event_type) => LifecycleSink::failing_on(event_type),
            None => LifecycleSink::new(),
        };
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                factory,
            )
            .expect("agent pool"),
        );
        let todos = Arc::new(TodoStore::default());
        let sessions: Arc<dyn SessionEventSink> = sink.clone();
        let runner = Arc::new(SessionRunner::new(
            sessions.clone(),
            Arc::new(TurnContextRegistry::default()),
        ));
        let mut wake = WakeRunner::from_parts(db.clone(), agent_pool.clone(), sessions, todos.clone(), runner)
            .with_memory_source(Arc::new(EmptyMemory));
        let snapshot_provider = with_snapshot.then(empty_workspace_snapshot_provider);
        if let Some(provider) = &snapshot_provider {
            wake = wake.with_snapshot_provider(provider.clone());
        }
        wake.system_prompts = Arc::new(tinyiothub_core::config::SystemPromptsConfig {
            workspace_dir: prompts.path().display().to_string(),
            ..Default::default()
        });
        let schedule = AssistantSchedule {
            workspace_id: "ws".into(),
            assistant_id: assistant_agent_id("ws"),
            interval_seconds: 300,
            enabled: true,
            permission_mode: PermissionMode::Ask,
            prompt: "巡检".into(),
            next_wake_at: Some(now),
            last_wake_at: None,
            last_outcome: None,
        };
        LifecycleFixture {
            _root: root,
            _prompts: prompts,
            db,
            pool: agent_pool,
            todos,
            sink,
            wake: Arc::new(wake),
            schedule,
            requests,
            fail_provider,
            snapshot_provider,
        }
    }

    impl LifecycleFixture {
        async fn run(&self) -> WakeReport {
            self.wake.run(&self.schedule, CancellationToken::new()).await
        }

        async fn reconcile(&self) -> WakeReport {
            self.wake.run_reconcile(&self.schedule, CancellationToken::new()).await
        }

        async fn state(&self) -> tinyiothub_storage::suppression::SuppressionState {
            self.db
                .load_suppression_state("ws", &lifecycle_session_key())
                .await
                .expect("suppression state")
                .expect("suppression row")
        }

        async fn preflight(&self) -> PreflightDecision {
            self.preflight_with(&self.wake).await
        }

        async fn preflight_with(&self, wake: &WakeRunner) -> PreflightDecision {
            let session_key = lifecycle_session_key();
            let config = self
                .db
                .get_suppression_config("ws")
                .await
                .expect("config")
                .expect("schedule row");
            let state = self
                .db
                .load_suppression_state("ws", &session_key)
                .await
                .expect("suppression state");
            let cursor = registry::load_session_events(&self.db, "ws", &session_key)
                .await
                .expect("session events")
                .control_cursor;
            let registry = registry::default_sources(
                wake.system_prompts.clone(),
                self.db.clone(),
                wake.memory_source.clone(),
                wake.snapshot_provider.clone(),
            )
            .expect("source registry");
            SuppressionService::new(Arc::new(registry))
                .evaluate(
                    SourceContext {
                        workspace_id: "ws".into(),
                        session_id: session_key,
                        observed_at: Utc::now(),
                        mode: SourceMode::Off,
                    },
                    state.as_ref(),
                    &config,
                    cursor,
                )
                .await
                .expect("preflight")
        }

        async fn session_events(&self) -> Vec<HarnessEvent> {
            let filter = EventFilter {
                session_id: Some(lifecycle_session_key()),
                run_id: None,
                task_id: None,
                execution_mode: ExecutionMode::Production,
            };
            self.db
                .session_event_page("ws", &filter, 0, 256)
                .await
                .expect("session events")
                .events
        }

        async fn skipped_turn_count(&self) -> usize {
            self.session_events()
                .await
                .iter()
                .filter(|event| event.event_type == "turn.completed" && event.payload_json["outcome"] == "skipped")
                .count()
        }

        fn write_workspace_source(&self, name: &str, content: &str) {
            let dir = self._prompts.path().join("ws");
            std::fs::create_dir_all(&dir).expect("workspace source dir");
            std::fs::write(dir.join(name), content).expect("workspace source write");
        }

        fn restarted_wake(&self) -> Arc<WakeRunner> {
            let sessions: Arc<dyn SessionEventSink> = self.sink.clone();
            let runner = Arc::new(SessionRunner::new(
                sessions.clone(),
                Arc::new(TurnContextRegistry::default()),
            ));
            let mut wake =
                WakeRunner::from_parts(self.db.clone(), self.pool.clone(), sessions, self.todos.clone(), runner)
                    .with_memory_source(Arc::new(EmptyMemory));
            if let Some(provider) = &self.snapshot_provider {
                wake = wake.with_snapshot_provider(provider.clone());
            }
            wake.system_prompts = self.wake.system_prompts.clone();
            Arc::new(wake)
        }
    }

    #[tokio::test]
    async fn successful_full_turn_installs_baseline_and_the_next_wake_skips() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        let first = fixture.run().await;
        assert_eq!(first.outcome, WakeOutcome::Completed);
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_completed");
        assert!(
            state.baseline_is_eligible(),
            "a successful full Turn installs a baseline"
        );
        assert!(state.baseline_state_id.is_some());
        assert!(state.baseline_turn_state.is_some());
        assert_eq!(fixture.requests.lock().await.len(), 1);

        let decision = fixture.preflight().await;
        assert!(
            decision.would_skip,
            "unchanged state must be skip-eligible: vetoes={:?} fence={}",
            decision.vetoes, decision.fence
        );

        let second = fixture.run().await;
        assert_eq!(second.outcome, WakeOutcome::Skipped);
        assert_eq!(
            fixture.requests.lock().await.len(),
            1,
            "a skip must make zero model calls"
        );
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "skipped");
        assert_eq!(fixture.skipped_turn_count().await, 1);
        assert!(
            state.schedule_sync_pending,
            "TriggerRouter owns the Schedule write and its pending barrier"
        );
        assert!(!state.projection_sync_pending, "session projection reconciled");
        assert!(
            !state.notification_pending,
            "committed events are the SSE source of truth"
        );
        assert!(!state.baseline_is_eligible(), "Schedule is still pending");

        let settled = fixture
            .db
            .clear_sync_pending(
                "ws",
                &lifecycle_session_key(),
                tinyiothub_storage::suppression::SyncPending::Schedule,
            )
            .await
            .expect("schedule reconcile");
        assert!(settled.baseline_is_eligible());
    }

    #[tokio::test]
    async fn failed_reconcile_cannot_reuse_previous_baseline() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        fixture.fail_provider.store(true, Ordering::SeqCst);

        let report = fixture.reconcile().await;
        assert!(matches!(report.outcome, WakeOutcome::Failed { .. }));

        let decision = fixture.preflight().await;
        assert!(!decision.would_skip, "a failed full attempt may never be skipped over");
        assert_eq!(decision.fence, "full_failed");
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_failed");
        assert!(!state.baseline_is_eligible());
    }

    #[tokio::test]
    async fn restart_keeps_full_failed_fence_until_a_successful_full_turn() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        fixture.fail_provider.store(true, Ordering::SeqCst);
        assert!(matches!(fixture.reconcile().await.outcome, WakeOutcome::Failed { .. }));

        // A fresh worker over the same durable state (process restart) still
        // sees the failure fence, not the old baseline.
        let restarted = fixture.restarted_wake();
        let fenced = fixture.preflight_with(&restarted).await;
        assert!(!fenced.would_skip);
        assert_eq!(fenced.fence, "full_failed");

        fixture.fail_provider.store(false, Ordering::SeqCst);
        let report = restarted.run(&fixture.schedule, CancellationToken::new()).await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        let recovered = fixture.preflight().await;
        assert!(recovered.would_skip);
        assert_eq!(recovered.fence, "");
        assert!(fixture.state().await.baseline_is_eligible());
    }

    #[tokio::test]
    async fn partial_todos_fail_the_full_attempt() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        fixture.todos.replace(
            &lifecycle_session_key(),
            vec![Todo {
                content: "巡检未完成".into(),
                status: TodoStatus::InProgress,
                active_form: "巡检中".into(),
            }],
        );

        let report = fixture.run().await;
        assert_eq!(report.outcome, WakeOutcome::Partial);
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_failed");
        assert!(state.baseline_state_id.is_none());
        assert!(!state.baseline_is_eligible());
        assert!(!fixture.preflight().await.would_skip);
    }

    #[tokio::test]
    async fn cancelled_full_turn_leaves_full_failed_fence() {
        let fixture = lifecycle_fixture_with_factory(
            SuppressionMode::Enforce,
            None,
            true,
            Some(Arc::new(|| Ok(Box::new(SleepingProvider)))),
        )
        .await;
        let wake = fixture.wake.clone();
        let schedule = fixture.schedule.clone();
        let cancel = CancellationToken::new();
        let handle = {
            let cancel = cancel.clone();
            tokio::spawn(async move { wake.run(&schedule, cancel).await })
        };

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let started = fixture
                .sink
                .events
                .lock()
                .await
                .iter()
                .any(|event| event.event_type == "turn.started");
            if started {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "turn did not start");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        cancel.cancel();

        let report = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("cancelled wake must finish")
            .unwrap();
        assert_eq!(report.outcome, WakeOutcome::Cancelled);

        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_failed");
        assert!(state.baseline_state_id.is_none());
        assert!(!state.full_attempt_in_progress, "the fence must be settled");
        assert!(!fixture.preflight().await.would_skip);
    }

    #[tokio::test]
    async fn degraded_source_prevents_baseline_installation() {
        // No workspace snapshot provider: that Source is unavailable, so the
        // full Turn still runs but may not install a baseline.
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, false).await;
        let report = fixture.run().await;
        assert_eq!(report.outcome, WakeOutcome::Completed);

        let decision = fixture.preflight().await;
        assert!(!decision.would_skip);
        assert_eq!(decision.fence, "full_failed");
        let state = fixture.state().await;
        assert!(state.baseline_state_id.is_none());
        assert!(state.baseline_turn_state.is_none());
        assert!(!state.baseline_is_eligible());
    }

    #[tokio::test]
    async fn missing_terminal_session_event_prevents_baseline_installation() {
        for failed_event in ["turn.completed", "session.ended"] {
            let fixture = lifecycle_fixture(SuppressionMode::Enforce, Some(failed_event), true).await;
            let report = fixture.run().await;
            assert!(
                matches!(report.outcome, WakeOutcome::Failed { .. }),
                "missing {failed_event} must surface as a failed Turn"
            );

            let decision = fixture.preflight().await;
            assert!(!decision.would_skip, "case={failed_event}");
            assert_eq!(decision.fence, "full_failed", "case={failed_event}");
            let state = fixture.state().await;
            assert!(state.baseline_state_id.is_none(), "case={failed_event}");
            assert!(!state.full_attempt_in_progress, "case={failed_event}");
        }
    }

    #[tokio::test]
    async fn wake_and_reconcile_emit_identical_model_requests_for_the_same_captured_state() {
        let wake_fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        let reconcile_fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;

        assert_eq!(wake_fixture.run().await.outcome, WakeOutcome::Completed);
        assert_eq!(reconcile_fixture.reconcile().await.outcome, WakeOutcome::Completed);

        let wake_requests = wake_fixture.requests.lock().await.clone();
        let reconcile_requests = reconcile_fixture.requests.lock().await.clone();
        assert_eq!(wake_requests.len(), 1);
        assert_eq!(reconcile_requests.len(), 1);
        assert_eq!(
            wake_requests, reconcile_requests,
            "wake and reconcile must share one full-Turn request path"
        );
    }

    #[tokio::test]
    async fn reconcile_forces_a_full_turn_even_when_the_state_is_unchanged() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert!(fixture.preflight().await.would_skip);

        let report = fixture.reconcile().await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        assert_eq!(
            fixture.requests.lock().await.len(),
            2,
            "reconcile must not take the skip path"
        );
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_completed");
        assert!(state.baseline_is_eligible());
    }

    #[tokio::test]
    async fn shadow_mode_computes_would_skip_but_always_runs_the_full_turn() {
        let fixture = lifecycle_fixture(SuppressionMode::Shadow, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        let decision = fixture.preflight().await;
        assert!(decision.would_skip);
        assert!(!decision.mode.allows_skip());

        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert_eq!(fixture.requests.lock().await.len(), 2);
        assert_eq!(fixture.skipped_turn_count().await, 0);
        assert_eq!(fixture.state().await.latest_outcome, "full_completed");
    }

    #[tokio::test]
    async fn preflight_read_failure_fences_a_failed_full_turn() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert!(fixture.state().await.baseline_is_eligible());

        // The wake-level Session Events read fails, so no decision/cursor can
        // be built. The Turn must still be durably fenced and settled.
        fixture.wake.fail_session_events_read.store(true, Ordering::SeqCst);
        fixture.fail_provider.store(true, Ordering::SeqCst);
        let report = fixture.run().await;
        assert!(matches!(report.outcome, WakeOutcome::Failed { .. }));

        fixture.wake.fail_session_events_read.store(false, Ordering::SeqCst);
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_failed");
        assert!(!state.full_attempt_in_progress);
        assert!(!state.baseline_is_eligible());

        let decision = fixture.preflight().await;
        assert!(!decision.would_skip);
        assert_eq!(decision.fence, "full_failed");
    }

    #[tokio::test]
    async fn successful_turn_with_preflight_read_failure_installs_a_durable_baseline() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert_eq!(fixture.state().await.attempt_no, 1);

        // Session-events read failure only affects the wake-level cursor, not
        // the `session.events` Source capture; the pre-turn snapshot is still
        // valid baseline evidence.
        fixture.wake.fail_session_events_read.store(true, Ordering::SeqCst);
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        fixture.wake.fail_session_events_read.store(false, Ordering::SeqCst);

        let state = fixture.state().await;
        assert_eq!(
            state.attempt_no, 2,
            "a configured workspace must use the durable allocation, never a predicted attempt"
        );
        assert_eq!(state.latest_outcome, "full_completed");
        assert!(state.baseline_state_id.is_some());
        assert!(state.baseline_is_eligible());
    }

    #[tokio::test]
    async fn fencing_only_observation_settles_the_full_attempt_without_a_baseline() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        let session_key = lifecycle_session_key();
        let baseline = fixture
            .db
            .load_suppression_state("ws", &session_key)
            .await
            .expect("state")
            .unwrap_or_else(|| tinyiothub_storage::suppression::SuppressionState::initial("ws", &session_key));
        let durable = fixture
            .db
            .begin_full_attempt(&session_key, &baseline)
            .await
            .expect("full attempt");

        let observation = super::fencing_observation(baseline, chrono::Utc::now(), "preflight_capture_failed");
        let outcome = fixture
            .wake
            .settle_full_attempt("ws", &durable, &observation, None, WakeOutcome::Completed, true)
            .await;

        assert_eq!(outcome, WakeOutcome::Completed, "the model Turn itself completed");
        let state = fixture.state().await;
        assert_eq!(state.latest_outcome, "full_failed");
        assert_eq!(state.last_error.as_deref(), Some("preflight_capture_failed"));
        assert!(state.baseline_state_id.is_none());
        assert!(!state.full_attempt_in_progress);
        assert!(!state.baseline_is_eligible());
    }

    #[tokio::test]
    async fn changed_replay_links_the_new_full_attempt_to_the_committed_skip() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Skipped);
        let skip_attempt_id = fixture
            .state()
            .await
            .latest_attempt_id
            .expect("committed skip attempt id");
        assert_eq!(fixture.skipped_turn_count().await, 1);

        // Change a Source after the committed skip: replay must run a full
        // Turn linked to the same planned wake.
        fixture.write_workspace_source("TURN_BRIEF.md", "changed after the committed skip");
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);

        let events = fixture.sink.events.lock().await;
        let started = events
            .iter()
            .rev()
            .find(|event| event.event_type == "turn.started")
            .expect("turn.started");
        assert_eq!(
            started.payload_json["previousAttemptId"],
            serde_json::json!(skip_attempt_id)
        );
        assert_eq!(started.payload_json["supersedes"], serde_json::json!(true));

        let dispatched = events
            .iter()
            .rev()
            .find(|event| event.event_type == "wake.dispatched")
            .expect("wake.dispatched");
        assert_eq!(
            dispatched.payload_json["previousAttemptId"],
            serde_json::json!(skip_attempt_id)
        );
        assert_eq!(dispatched.payload_json["supersedes"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn manual_operation_allocates_the_durable_attempt_before_execution() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);

        let prepared = fixture
            .wake
            .prepare_manual(&fixture.schedule, ManualOperation::Reconcile)
            .await
            .expect("manual reconcile prepared");
        let turn_id = prepared.turn_id().to_string();
        assert!(turn_id.starts_with("turn:"));
        assert_eq!(prepared.session_key(), lifecycle_session_key().as_str());
        let attempt_id = prepared.attempt_id().to_string();
        assert_ne!(attempt_id, turn_id, "the durable attempt id is not the turn id");
        // The 202 may only be sent after this durable fence exists, and the
        // idempotency record must be joinable against this exact attempt id.
        let state = fixture.state().await;
        assert!(state.full_attempt_in_progress);
        assert_eq!(
            state.latest_attempt_id.as_deref(),
            Some(attempt_id.as_str()),
            "prepared attempt id must match suppression_state.latest_attempt_id"
        );

        let report = fixture.wake.execute_manual(prepared, CancellationToken::new()).await;
        assert_eq!(report.outcome, WakeOutcome::Completed);
        assert_eq!(report.session_key, lifecycle_session_key());

        // The executed Turn's lifecycle events use the exact preallocated
        // turnId (the fixture sink records them in memory; committed skip
        // events go directly to the workspace DB).
        let events = fixture.sink.events.lock().await;
        assert!(
            events.iter().any(|event| {
                event.event_type == "turn.completed" && event.run_id.as_deref() == Some(turn_id.as_str())
            }),
            "the executed Turn must carry the preallocated turnId {turn_id}"
        );
        let state = fixture.state().await;
        assert!(!state.full_attempt_in_progress);
        assert!(
            state.baseline_is_eligible(),
            "a successful reconcile installs a baseline"
        );
    }

    #[tokio::test]
    async fn manual_wake_uses_the_shared_skip_path_and_reconcile_forces_full() {
        let fixture = lifecycle_fixture(SuppressionMode::Enforce, None, true).await;
        assert_eq!(fixture.run().await.outcome, WakeOutcome::Completed);
        assert_eq!(fixture.skipped_turn_count().await, 0);

        let wake = fixture
            .wake
            .prepare_manual(&fixture.schedule, ManualOperation::Wake)
            .await
            .expect("manual wake prepared");
        assert_eq!(
            fixture
                .wake
                .execute_manual(wake, CancellationToken::new())
                .await
                .outcome,
            WakeOutcome::Skipped,
            "unchanged Sources in enforce mode must skip"
        );
        assert_eq!(fixture.skipped_turn_count().await, 1);

        // Reconcile ignores wouldSkip and runs the same full-Turn function.
        let reconcile = fixture
            .wake
            .prepare_manual(&fixture.schedule, ManualOperation::Reconcile)
            .await
            .expect("manual reconcile prepared");
        assert_eq!(
            fixture
                .wake
                .execute_manual(reconcile, CancellationToken::new())
                .await
                .outcome,
            WakeOutcome::Completed
        );
        assert_eq!(fixture.skipped_turn_count().await, 1, "reconcile must never skip");
    }

    #[tokio::test]
    async fn manual_reconcile_in_off_mode_settles_without_a_baseline() {
        let fixture = lifecycle_fixture(SuppressionMode::Off, None, true).await;
        let prepared = fixture
            .wake
            .prepare_manual(&fixture.schedule, ManualOperation::Reconcile)
            .await
            .expect("manual reconcile prepared");
        assert!(fixture.state().await.full_attempt_in_progress);
        assert_eq!(
            fixture
                .wake
                .execute_manual(prepared, CancellationToken::new())
                .await
                .outcome,
            WakeOutcome::Completed
        );
        let state = fixture.state().await;
        assert!(!state.full_attempt_in_progress);
        assert!(
            state.baseline_state_id.is_none(),
            "off mode must not install a baseline"
        );
        assert!(
            state.full_failed,
            "the old baseline stays ineligible until a full Turn installs one"
        );
    }

    #[tokio::test]
    async fn scheduled_shadow_turns_record_one_observation_each() {
        let fixture = lifecycle_fixture(SuppressionMode::Shadow, None, true).await;
        // First Turn installs the baseline; the second computes wouldSkip but
        // shadow still executes the full Turn.
        let _ = fixture.run().await;
        let _ = fixture.run().await;

        let pool = fixture
            .db
            .workspace_pool_for_testing("ws")
            .await
            .expect("workspace pool");
        let rows: Vec<(String, String, i64, i64, i64)> = sqlx::query_as(
            "SELECT turn_id, mode, would_skip, model_calls, tool_calls \
             FROM suppression_observations WHERE workspace_id = 'ws' ORDER BY observed_at ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("observations");
        assert_eq!(rows.len(), 2, "exactly one row per completed Turn");
        assert!(rows.iter().all(|(_, mode, _, _, _)| mode == "shadow"));
        assert_eq!(rows[0].2, 0, "first Turn has no baseline");
        assert_eq!(
            rows[1].2, 1,
            "shadow records the second Turn's counterfactual wouldSkip"
        );
        assert!(
            rows.iter().all(|(turn_id, _, _, _, _)| !turn_id.is_empty()),
            "recorded turn ids are real"
        );
    }

    #[tokio::test]
    async fn confirmed_false_skip_uses_the_real_alert_producer() {
        let fixture = lifecycle_fixture(SuppressionMode::Canary, None, true).await;
        let session_key = lifecycle_session_key();
        fixture
            .db
            .record_suppression_turn_observation(&SuppressionTurnObservation {
                workspace_id: "ws".to_string(),
                session_id: session_key,
                turn_id: "turn-false-skip".to_string(),
                attempt_id: "attempt-false-skip".to_string(),
                mode: SuppressionMode::Canary,
                observed_at: Utc::now(),
                would_skip: true,
                vetoes: Vec::new(),
                model_calls: 0,
                tool_calls: 0,
                preflight_latency_ms: 10,
                registry_version: "sha256:test-registry".to_string(),
            })
            .await
            .expect("observation");

        let db = fixture.db.clone();
        let sink = Arc::new(crate::domains::agent::session::events::DbSessionEventSink::new(
            db.clone(),
            Arc::new(crate::domains::agent::runtime::session_stream::SessionStreamHub::default()),
        ));
        let runner = Arc::new(SessionRunner::new(
            sink.clone(),
            Arc::new(TurnContextRegistry::default()),
        ));
        let wake = WakeRunner::from_parts(
            db.clone(),
            fixture.pool.clone(),
            sink,
            Arc::new(TodoStore::default()),
            runner,
        );
        wake.record_confirmed_false_skip("ws", "turn-false-skip", "reviewer-1", "fixed the tombstone")
            .await
            .expect("emit the real alert");

        let recorded = db
            .latest_confirmed_false_skip_at("ws")
            .await
            .expect("false-skip read")
            .expect("confirmed false skip must be persisted");
        assert!(recorded <= Utc::now());
        let events = db.recent_session_events("ws", 20).await.expect("recent events");
        assert!(
            events.iter().any(|event| {
                event.event_type == ALERT_EVENT_TYPE
                    && event.payload_json["code"] == "false_skip_detected"
                    && event.payload_json["turnId"] == "turn-false-skip"
            }),
            "the producer must persist an assistant.suppression.alert event"
        );
    }
}
