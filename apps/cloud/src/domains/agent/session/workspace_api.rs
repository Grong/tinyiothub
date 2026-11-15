//! Workspace 级的 Assistant Schedule、Global HUD 与 suppression 操作端点。
//!
//! 只读聚合：设备/告警/待答/待批/活动会话/最近事件都来自既有存储，不新增状态表。
//! 所有端点先做 workspace 归属校验。Suppression 的决策/执行仍住 session
//! 模块；本文件只做 HTTP DTO、权限、problem details 和幂等投影。

use std::collections::HashSet;

use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tinyiothub_storage::alarm::{AlarmLevel, AlarmQueryCriteria, AlarmStatus};
use tinyiothub_storage::assistant_schedule::{AssistantSchedule, SuppressionConfig};
use tinyiothub_storage::suppression::{
    DuplicateReportLabels, FalseSkipReview, MAX_RECONCILIATION_SECONDS, MIN_RECONCILIATION_SECONDS,
    SuppressionGateMode, SuppressionMode, SuppressionTurnTerminal,
};
use tinyiothub_web::security::Claims;
use uuid::Uuid;

use crate::domains::agent::AgentState;
use crate::domains::agent::session::handlers::is_admin;
use crate::domains::agent::session::hud::{
    HudActivity, HudAlarms, HudAssistant, HudDevices, HudPending, HudResponse, HudSchedule, HudStatusInputs,
    MAX_RECENT_EVENTS, assistant_status,
};
use crate::domains::agent::session::suppression::contract::{SourceDescriptor, SourceSnapshot};
use crate::domains::agent::session::suppression::preflight::{
    GateValidationContext, PreflightDecision, derive_canary_gate_artifact, derive_shadow_gate_artifact,
    seal_gate_artifact, validate_transition, verify_artifact_evidence,
};
use crate::domains::agent::session::wake::{ManualOperation, ManualPreflight, ManualStartError, assistant_agent_id};

pub fn create_workspace_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{workspace_id}/agent/hud", get(get_hud))
        .route("/{workspace_id}/agent/schedule", get(get_schedule).put(update_schedule))
        .route(
            "/{workspace_id}/agent/suppression/preflight",
            get(get_suppression_preflight),
        )
        .route("/{workspace_id}/agent/suppression/report", get(get_suppression_report))
        .route("/{workspace_id}/agent/suppression/wake", post(post_suppression_wake))
        .route(
            "/{workspace_id}/agent/suppression/reconcile",
            post(post_suppression_reconcile),
        )
        .route(
            "/{workspace_id}/agent/suppression/gate-artifacts",
            post(post_seal_gate_artifact),
        )
        .route(
            "/{workspace_id}/agent/suppression/false-skip",
            post(post_confirmed_false_skip),
        )
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// RFC 9457 `application/problem+json` with a stable snake_case `code`.
fn problem(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    let body = json!({
        "type": "about:blank",
        "title": status.canonical_reason().unwrap_or("Error"),
        "status": status.as_u16(),
        "code": code,
        "detail": detail.into(),
    });
    (status, [(header::CONTENT_TYPE, "application/problem+json")], Json(body)).into_response()
}

async fn verify_workspace_read(state: &AgentState, claims: &Claims, workspace_id: &str) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(error_response(StatusCode::FORBIDDEN, "无权访问此工作空间")),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "工作空间不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check workspace failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败"))
        }
    }
}

/// Same tenant/workspace gate as [`verify_workspace_read`], expressed as an
/// RFC 9457 problem for the new suppression endpoints.
async fn verify_workspace_read_problem(
    state: &AgentState,
    claims: &Claims,
    workspace_id: &str,
) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(problem(
            StatusCode::FORBIDDEN,
            "workspace_forbidden",
            "无权访问此工作空间",
        )),
        Ok(None) => Err(problem(StatusCode::NOT_FOUND, "workspace_not_found", "工作空间不存在")),
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check workspace failed");
            Err(problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "workspace_lookup_failed",
                "获取工作空间失败",
            ))
        }
    }
}

/// Workspace write permission: tenant ownership plus an administrator grant.
/// R4 tightens PUT schedule and the manual POST endpoints to this gate.
async fn verify_workspace_write_problem(
    state: &AgentState,
    claims: &Claims,
    workspace_id: &str,
) -> Result<(), Response> {
    verify_workspace_read_problem(state, claims, workspace_id).await?;
    if !is_admin(&state.db, &claims.user_id).await {
        return Err(problem(
            StatusCode::FORBIDDEN,
            "workspace_write_required",
            "需要管理员权限",
        ));
    }
    Ok(())
}

fn validate_interval(interval_seconds: u32) -> Result<u32, String> {
    if (tinyiothub_storage::assistant_schedule::MIN_ASSISTANT_INTERVAL_SECONDS
        ..=tinyiothub_storage::assistant_schedule::MAX_ASSISTANT_INTERVAL_SECONDS)
        .contains(&interval_seconds)
    {
        Ok(interval_seconds)
    } else {
        Err("intervalSeconds 必须在 60 到 86400 之间".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    pub enabled: bool,
    pub interval_seconds: u32,
    /// Wire value of [`tinyiothub_core::policy::PermissionMode`].
    pub permission_mode: String,
    pub prompt: String,
    pub next_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_wake_at: Option<chrono::DateTime<Utc>>,
    pub last_outcome: Option<String>,
    /// `off` | `shadow` | `canary` | `enforce`.
    pub suppression_mode: String,
    pub reconciliation_seconds: u32,
    pub suppression_config_version: i64,
}

impl ScheduleDto {
    fn new(schedule: &AssistantSchedule, config: &SuppressionConfig) -> Self {
        Self {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            permission_mode: schedule.permission_mode.as_str().to_string(),
            prompt: schedule.prompt.clone(),
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome.clone(),
            suppression_mode: config.mode.as_str().to_string(),
            reconciliation_seconds: config.reconciliation_seconds,
            suppression_config_version: config.config_version,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScheduleRequest {
    pub enabled: Option<bool>,
    pub interval_seconds: Option<u32>,
    pub prompt: Option<String>,
    /// Validated as one of `off|shadow|canary|enforce`; invalid values return
    /// a 400 problem without any partial write.
    pub suppression_mode: Option<String>,
    pub reconciliation_seconds: Option<u32>,
}

/// Parse and validate the suppression fields before any write.
#[allow(clippy::result_large_err)] // axum response is the error channel by design
fn parse_suppression_update(
    request: &UpdateScheduleRequest,
) -> Result<(Option<SuppressionMode>, Option<u32>), Response> {
    let mode = match request.suppression_mode.as_deref() {
        None => None,
        Some(value) => Some(SuppressionMode::parse(value.trim()).ok_or_else(|| {
            problem(
                StatusCode::BAD_REQUEST,
                "suppression_config_invalid",
                "suppressionMode 必须是 off、shadow、canary 或 enforce",
            )
        })?),
    };
    if let Some(seconds) = request.reconciliation_seconds
        && !(MIN_RECONCILIATION_SECONDS..=MAX_RECONCILIATION_SECONDS).contains(&seconds)
    {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            format!("reconciliationSeconds 必须在 {MIN_RECONCILIATION_SECONDS} 到 {MAX_RECONCILIATION_SECONDS} 之间"),
        ));
    }
    Ok((mode, request.reconciliation_seconds))
}

fn gate_problem(detail: impl Into<String>) -> Response {
    problem(StatusCode::CONFLICT, "shadow_gate_not_met", detail)
}

/// Re-derive every non-human metric from the server-recorded observations.
/// Persisting an artifact row is not evidence; the observations must support
/// it.
#[allow(clippy::result_large_err)]
async fn verify_gate_artifact_evidence(
    state: &AgentState,
    workspace_id: &str,
    artifact: &tinyiothub_storage::suppression::SuppressionGateArtifact,
) -> Result<(), Response> {
    let sample = state
        .db
        .load_suppression_turn_observations(workspace_id, &artifact.sample.turn_ids)
        .await
        .map_err(internal)?;
    let baseline = if artifact.mode == SuppressionGateMode::Canary {
        state
            .db
            .load_suppression_turn_observations(workspace_id, &artifact.sample.baseline_turn_ids)
            .await
            .map_err(internal)?
    } else {
        Vec::new()
    };
    verify_artifact_evidence(artifact, &sample, &baseline).map_err(|error| gate_problem(error.what))
}

/// Task 10: `canary` requires an approved shadow artifact and `enforce`
/// requires an approved canary-measured artifact. `off` is the kill switch and
/// `shadow` is observational, so both are always allowed. There is no bypass
/// parameter, and a stale/absent artifact fails closed with a 409
/// `shadow_gate_not_met` problem.
#[allow(clippy::result_large_err)] // axum response is the error channel by design
async fn validate_gate_artifact_seam(
    state: &AgentState,
    workspace_id: &str,
    mode: SuppressionMode,
) -> Result<(), Response> {
    let artifact_mode = match mode {
        SuppressionMode::Off | SuppressionMode::Shadow => return Ok(()),
        SuppressionMode::Canary => SuppressionGateMode::Shadow,
        SuppressionMode::Enforce => SuppressionGateMode::Canary,
    };
    let Some(ops) = state.suppression_ops.as_ref() else {
        return Err(problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "suppression operations are not wired; gate evidence cannot be verified",
        ));
    };
    let current_registry_version = ops.registry_version(workspace_id).await.map_err(|error| {
        tracing::warn!(%workspace_id, ?error, "gate validation: registry version unavailable");
        problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "无法验证当前 registryVersion",
        )
    })?;
    let artifact = match state
        .db
        .latest_suppression_gate_artifact(workspace_id, artifact_mode)
        .await
    {
        Ok(Some(artifact)) => artifact,
        Ok(None) => {
            return Err(gate_problem(match artifact_mode {
                SuppressionGateMode::Shadow => "canary 需要已批准的 shadow gate artifact",
                SuppressionGateMode::Canary => "enforce 需要已批准的 canary 实测 gate artifact",
            }));
        }
        Err(error) => return Err(internal(error)),
    };
    verify_gate_artifact_evidence(state, workspace_id, &artifact).await?;
    let latest_confirmed_false_skip_at = match state.db.latest_confirmed_false_skip_at(workspace_id).await {
        Ok(value) => value,
        Err(error) => return Err(internal(error)),
    };
    let context = GateValidationContext {
        workspace_id,
        current_registry_version: &current_registry_version,
        latest_confirmed_false_skip_at,
        allow_synthetic: state.suppression_gate_allow_synthetic,
    };
    validate_transition(mode, &artifact, &context).map_err(|error| {
        tracing::warn!(
            %workspace_id,
            code = %error.code,
            reason = %error.what,
            "gate validation rejected a mode change"
        );
        gate_problem(error.what)
    })
}

pub async fn get_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => {
            let config = state
                .db
                .get_suppression_config(&workspace_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| SuppressionConfig::off(workspace_id.clone()));
            Json(ScheduleDto::new(&schedule, &config)).into_response()
        }
        Err(error) => internal(error),
    }
}

pub async fn update_schedule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Response {
    // R4: schedule mutations require workspace write permission, not read.
    if let Err(response) = verify_workspace_write_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    // Fail before any write: an invalid config must leave the row untouched.
    let (mode, reconciliation_seconds) = match parse_suppression_update(&request) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    if let Some(mode) = mode
        && let Err(response) = validate_gate_artifact_seam(&state, &workspace_id, mode).await
    {
        return response;
    }
    let interval_seconds = match request.interval_seconds {
        Some(value) => match validate_interval(value) {
            Ok(value) => value,
            Err(_) => {
                return problem(
                    StatusCode::BAD_REQUEST,
                    "suppression_config_invalid",
                    format!(
                        "intervalSeconds 必须在 {} 到 {} 之间",
                        tinyiothub_storage::assistant_schedule::MIN_ASSISTANT_INTERVAL_SECONDS,
                        tinyiothub_storage::assistant_schedule::MAX_ASSISTANT_INTERVAL_SECONDS,
                    ),
                );
            }
        },
        None => 0,
    };
    let now = Utc::now();
    let current = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let interval_seconds = if request.interval_seconds.is_some() {
        interval_seconds
    } else {
        current.interval_seconds
    };

    // Kill switch first: once `off` is committed, later ticks must not skip.
    // `assistant_schedules` lives in the platform DB while a skip commit runs
    // in the workspace DB. A skip whose commit is already in flight cannot be
    // rolled back by this write; Task 8/9 own the alert/reconciliation for
    // that residual window. This handler does not claim cross-DB atomicity.
    if (mode.is_some() || reconciliation_seconds.is_some())
        && let Err(error) = state
            .db
            .update_suppression_config(&workspace_id, mode, reconciliation_seconds)
            .await
    {
        return internal(error);
    }

    let recompute = request.enabled == Some(true) || request.interval_seconds.is_some();
    let next_wake_at = recompute.then(|| now + chrono::Duration::seconds(i64::from(interval_seconds)));

    let schedule = match state
        .db
        .update_assistant_schedule(
            &workspace_id,
            request.enabled,
            Some(interval_seconds),
            request.prompt.as_deref(),
            next_wake_at,
            None,
        )
        .await
    {
        Ok(Some(schedule)) => schedule,
        Ok(None) => {
            return problem(StatusCode::NOT_FOUND, "schedule_not_found", "Schedule 不存在");
        }
        Err(error) => return internal(error),
    };
    let config = match state.db.get_suppression_config(&workspace_id).await {
        Ok(Some(config)) => config,
        Ok(None) => SuppressionConfig::off(workspace_id.clone()),
        Err(error) => return internal(error),
    };
    Json(ScheduleDto::new(&schedule, &config)).into_response()
}

pub async fn get_hud(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read(&state, &claims, &workspace_id).await {
        return response;
    }
    let now = Utc::now();
    let schedule = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let paused = !schedule.enabled;

    let total = match state.db.count_things_total(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let online = match state.db.count_online_things(Some(workspace_id.as_str())).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let criteria = AlarmQueryCriteria {
        workspace_id: Some(workspace_id.clone()),
        alarm_levels: Some(vec![AlarmLevel::Critical, AlarmLevel::Error]),
        statuses: Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]),
        ..Default::default()
    };
    let alarms = match state.db.count_alarms_by_criteria(&criteria).await {
        Ok(value) => value as i64,
        Err(error) => return internal(error),
    };
    let harness_approvals = match state.db.count_pending_tool_approvals(&workspace_id).await {
        Ok(value) => value,
        Err(error) => return internal(error),
    };
    let broker_approvals = state
        .session_runtime
        .approvals
        .count_pending_for_workspace(&workspace_id) as i64;
    let approvals = harness_approvals + broker_approvals;
    let questions = state
        .session_runtime
        .questions
        .count_pending_for_workspace(&workspace_id);
    let active_sessions = state.session_runtime.contexts.count_for_workspace(&workspace_id);
    let recent_events = match state.db.recent_session_events(&workspace_id, MAX_RECENT_EVENTS).await {
        // Raw turn.state envelopes are backend-only; filter server-side.
        Ok(events) => events
            .into_iter()
            .filter(|event| crate::domains::agent::session::is_public_session_event(&event.event_type))
            .collect(),
        Err(error) => return internal(error),
    };

    let last_outcome = schedule.last_outcome.clone();
    let status = assistant_status(HudStatusInputs {
        paused,
        active_sessions,
        pending_questions: questions,
        pending_approvals: approvals,
        last_outcome: last_outcome.as_deref(),
    });

    Json(HudResponse {
        assistant: HudAssistant { status, last_outcome },
        devices: HudDevices {
            total,
            online,
            offline: (total - online).max(0),
        },
        alarms: HudAlarms { high_priority: alarms },
        pending: HudPending { questions, approvals },
        activity: HudActivity { active_sessions },
        schedule: HudSchedule {
            enabled: schedule.enabled,
            interval_seconds: schedule.interval_seconds,
            next_wake_at: schedule.next_wake_at,
            last_wake_at: schedule.last_wake_at,
            last_outcome: schedule.last_outcome,
        },
        recent_events,
    })
    .into_response()
}

fn internal(error: impl std::fmt::Display) -> Response {
    tracing::warn!(%error, "assistant HUD/schedule query failed");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取 Assistant 状态失败")
}

// ── Suppression public projections ──

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionCountsDto {
    pub active: usize,
    pub waiting: usize,
    pub closed: usize,
}

/// Public Source projection. It carries IDs, compatibility, availability,
/// disposition counts and ref-validation status — never refs, workKeys,
/// revisions, payloads or user text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSourceDto {
    pub source_id: String,
    pub source_version: u32,
    pub compatible: bool,
    pub available: bool,
    pub empty: bool,
    pub counts: SuppressionCountsDto,
    pub ref_validation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

fn public_source(descriptor: &SourceDescriptor, snapshot: Option<&SourceSnapshot>) -> PublicSourceDto {
    let Some(snapshot) = snapshot else {
        return PublicSourceDto {
            source_id: descriptor.source_id.clone(),
            source_version: descriptor.source_version,
            compatible: true,
            available: false,
            empty: false,
            counts: SuppressionCountsDto {
                active: 0,
                waiting: 0,
                closed: 0,
            },
            ref_validation: "unavailable".to_string(),
            error_code: None,
        };
    };
    let mut counts = SuppressionCountsDto {
        active: 0,
        waiting: 0,
        closed: 0,
    };
    for item in &snapshot.work_items {
        match item.disposition {
            crate::domains::agent::session::suppression::contract::WorkItemDisposition::Active => counts.active += 1,
            crate::domains::agent::session::suppression::contract::WorkItemDisposition::Waiting => counts.waiting += 1,
            crate::domains::agent::session::suppression::contract::WorkItemDisposition::Closed => counts.closed += 1,
        }
    }
    let ref_validation = if !snapshot.available {
        "unavailable"
    } else if snapshot.state_ref.is_some() {
        "valid"
    } else if snapshot.empty {
        "empty"
    } else {
        "missing"
    };
    PublicSourceDto {
        source_id: snapshot.source_id.clone(),
        source_version: snapshot.source_version,
        compatible: snapshot.source_version == descriptor.source_version,
        available: snapshot.available,
        empty: snapshot.empty,
        counts,
        ref_validation: ref_validation.to_string(),
        error_code: snapshot.error.as_ref().map(|error| error.code.clone()),
    }
}

fn public_sources(descriptors: &[SourceDescriptor], decision: &PreflightDecision) -> Vec<PublicSourceDto> {
    descriptors
        .iter()
        .map(|descriptor| {
            let snapshot = decision
                .snapshot
                .iter()
                .find(|snapshot| snapshot.source_id == descriptor.source_id);
            public_source(descriptor, snapshot)
        })
        .collect()
}

fn veto_names(decision: &PreflightDecision) -> Vec<String> {
    decision.vetoes.iter().map(|veto| veto.as_str().to_string()).collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionPreflightDto {
    pub workspace_id: String,
    pub session_key: String,
    pub mode: String,
    pub reconciliation_seconds: u32,
    pub config_version: i64,
    pub registry_version: String,
    pub would_skip: bool,
    pub vetoes: Vec<String>,
    pub fence: String,
    pub sources: Vec<PublicSourceDto>,
}

fn preflight_dto(workspace_id: &str, preflight: &ManualPreflight) -> SuppressionPreflightDto {
    SuppressionPreflightDto {
        workspace_id: workspace_id.to_string(),
        session_key: preflight.session_key.clone(),
        mode: preflight.mode.as_str().to_string(),
        reconciliation_seconds: preflight.reconciliation_seconds,
        config_version: preflight.config_version,
        registry_version: preflight.registry_version.clone(),
        would_skip: preflight.decision.would_skip,
        vetoes: veto_names(&preflight.decision),
        fence: preflight.decision.fence.clone(),
        sources: public_sources(&preflight.descriptors, &preflight.decision),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionReportQuery {
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionReportStateDto {
    pub latest_outcome: String,
    pub fences: Vec<String>,
    pub baseline_installed_at: Option<chrono::DateTime<Utc>>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionRecentOutcomeDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub outcome: String,
    pub occurred_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionTurnDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_calls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<chrono::DateTime<Utc>>,
}

fn report_turn(turn_id: String, terminal: Option<SuppressionTurnTerminal>) -> SuppressionTurnDto {
    match terminal {
        Some(terminal) => SuppressionTurnDto {
            turn_id: Some(turn_id),
            status: terminal.outcome.clone(),
            outcome: Some(terminal.outcome),
            tool_calls: terminal.tool_calls,
            model_calls: terminal.model_calls,
            occurred_at: terminal.occurred_at,
        },
        // Unknown or not-yet-terminal: report the status only, never invented
        // final counts.
        None => SuppressionTurnDto {
            turn_id: Some(turn_id),
            status: "unknown".to_string(),
            outcome: None,
            tool_calls: None,
            model_calls: None,
            occurred_at: None,
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionMetricsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn: Option<SuppressionTurnDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionAlertDto {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionNotificationDto {
    pub pending: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionReportDto {
    pub workspace_id: String,
    pub session_key: String,
    pub mode: String,
    pub reconciliation_seconds: u32,
    pub config_version: i64,
    pub registry_version: String,
    pub state: SuppressionReportStateDto,
    pub sources: Vec<PublicSourceDto>,
    pub vetoes: Vec<String>,
    pub recent_outcomes: Vec<SuppressionRecentOutcomeDto>,
    pub metrics: SuppressionMetricsDto,
    pub alerts: SuppressionAlertDto,
    pub notifications: SuppressionNotificationDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<SuppressionTurnDto>,
}

pub async fn get_suppression_preflight(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    if let Err(response) = verify_workspace_read_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(ops) = state.suppression_ops.as_ref() else {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "suppression operations are not wired",
        );
    };
    match ops.manual_preflight(&workspace_id).await {
        Ok(preflight) => Json(preflight_dto(&workspace_id, &preflight)).into_response(),
        Err(error) => manual_error(error),
    }
}

pub async fn get_suppression_report(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(query): Query<SuppressionReportQuery>,
) -> Response {
    if let Err(response) = verify_workspace_read_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(ops) = state.suppression_ops.as_ref() else {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "suppression operations are not wired",
        );
    };
    let preflight = match ops.manual_preflight(&workspace_id).await {
        Ok(preflight) => preflight,
        Err(error) => return manual_error(error),
    };
    let state_row = match state
        .db
        .load_suppression_state(&workspace_id, &preflight.session_key)
        .await
    {
        Ok(state) => state,
        Err(error) => return internal(error),
    };
    let recent = match state
        .db
        .recent_suppression_outcomes(&workspace_id, &preflight.session_key, 5)
        .await
    {
        Ok(recent) => recent,
        Err(error) => return internal(error),
    };
    let turn = match query.turn_id.as_deref() {
        Some(turn_id) => {
            let terminal = match state
                .db
                .suppression_turn_terminal(&workspace_id, &preflight.session_key, turn_id)
                .await
            {
                Ok(terminal) => terminal,
                Err(error) => return internal(error),
            };
            Some(report_turn(turn_id.to_string(), terminal))
        }
        None => None,
    };
    let last_turn = match recent.first().and_then(|outcome| outcome.turn_id.clone()) {
        Some(turn_id) => {
            let terminal = match state
                .db
                .suppression_turn_terminal(&workspace_id, &preflight.session_key, &turn_id)
                .await
            {
                Ok(terminal) => terminal,
                Err(error) => return internal(error),
            };
            Some(report_turn(turn_id, terminal))
        }
        None => None,
    };
    // A workspace that has never run a suppression Turn still has a readable
    // report: project the same fail-safe initial shape without writing it.
    let state_row = state_row.unwrap_or_else(|| {
        tinyiothub_storage::suppression::SuppressionState::initial(&workspace_id, &preflight.session_key)
    });
    let mut fences = Vec::new();
    if state_row.full_attempt_in_progress {
        fences.push("full_in_progress".to_string());
    }
    if state_row.full_failed {
        fences.push("full_failed".to_string());
    }
    if state_row.audit_in_progress {
        fences.push("audit_in_progress".to_string());
    }
    if state_row.schedule_sync_pending || state_row.projection_sync_pending || state_row.notification_pending {
        fences.push("sync_pending".to_string());
    }
    Json(SuppressionReportDto {
        workspace_id: workspace_id.clone(),
        session_key: preflight.session_key.clone(),
        mode: preflight.mode.as_str().to_string(),
        reconciliation_seconds: preflight.reconciliation_seconds,
        config_version: preflight.config_version,
        registry_version: preflight.registry_version.clone(),
        state: SuppressionReportStateDto {
            latest_outcome: state_row.latest_outcome.clone(),
            fences,
            baseline_installed_at: state_row.baseline_installed_at,
            updated_at: state_row.updated_at,
        },
        sources: public_sources(&preflight.descriptors, &preflight.decision),
        vetoes: veto_names(&preflight.decision),
        recent_outcomes: recent
            .into_iter()
            .map(|outcome| SuppressionRecentOutcomeDto {
                turn_id: outcome.turn_id,
                outcome: outcome.outcome,
                occurred_at: outcome.occurred_at,
            })
            .collect(),
        metrics: SuppressionMetricsDto { last_turn },
        alerts: {
            // Real errors and audit fences only: `full_failed` alone is the
            // baseline-ineligibility marker, not an alert (a deliberate
            // off-mode reconcile keeps it set with no active alert).
            let activity = crate::domains::agent::session::suppression::error::alert_activity(&state_row);
            SuppressionAlertDto {
                active: activity.active,
                code: activity.code,
            }
        },
        notifications: SuppressionNotificationDto {
            pending: state_row.notification_pending,
        },
        turn,
    })
    .into_response()
}

pub async fn post_suppression_wake(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    manual_operation(state, claims, workspace_id, headers, ManualOperation::Wake).await
}

pub async fn post_suppression_reconcile(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    manual_operation(state, claims, workspace_id, headers, ManualOperation::Reconcile).await
}

#[allow(clippy::result_large_err)] // axum response is the error channel by design
fn idempotency_key(headers: &HeaderMap) -> Result<String, Response> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            problem(
                StatusCode::BAD_REQUEST,
                "idempotency_key_required",
                "POST suppression 操作必须提供 Idempotency-Key 请求头",
            )
        })
}

/// Request-derived intent fingerprint.
///
/// POST wake/reconcile carry no body, so the whole intent is operation +
/// workspace. Mutable server state (for example the schedule's `nextWakeAt`)
/// is stored for diagnostics but is never a conflict key: a scheduled wake
/// ticking between a request and its retry must still replay the recorded
/// result. Source contents are likewise excluded.
pub(crate) fn request_fingerprint(operation: ManualOperation, workspace_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"tinyiothub:suppression-http-idempotency:v1\0");
    hasher.update(operation.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(workspace_id.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn report_url(workspace_id: &str, turn_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace_id}/agent/suppression/report?turnId={turn_id}")
}

fn accepted_body(session_key: &str, turn_id: &str, workspace_id: &str) -> Value {
    json!({
        "sessionKey": session_key,
        "turnId": turn_id,
        "outcome": "accepted",
        "reportUrl": report_url(workspace_id, turn_id),
    })
}

/// Reservation preparation deadline (server-owned timestamp). A reservation
/// that never recorded a result older than this is reclaimable by a retry, so
/// a crash or a failed result write cannot wedge the key permanently.
const IDEMPOTENCY_PREPARATION_DEADLINE_SECS: i64 = 30;

fn replay_or_conflict(
    record: tinyiothub_storage::suppression::SuppressionIdempotency,
    operation: ManualOperation,
    fingerprint: &str,
) -> Response {
    if record.operation != operation.as_str() || record.request_fingerprint != fingerprint {
        return problem(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "同一 Idempotency-Key 不能用于不同 operation 或请求指纹；请使用新的 key",
        );
    }
    match record.result {
        Some(result) => (StatusCode::ACCEPTED, Json(result)).into_response(),
        None => problem(
            StatusCode::CONFLICT,
            "wake_in_flight",
            "相同请求正在处理中，请稍后使用同一 Idempotency-Key 重试",
        ),
    }
}

async fn manual_operation(
    state: AgentState,
    claims: Claims,
    workspace_id: String,
    headers: HeaderMap,
    operation: ManualOperation,
) -> Response {
    if let Err(response) = verify_workspace_write_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    let key = match idempotency_key(&headers) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let Some(ops) = state.suppression_ops.as_ref().cloned() else {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "suppression operations are not wired",
        );
    };
    let now = Utc::now();
    let schedule = match state
        .db
        .ensure_assistant_schedule(&workspace_id, &assistant_agent_id(&workspace_id), now)
        .await
    {
        Ok(schedule) => schedule,
        Err(error) => return internal(error),
    };
    let fingerprint = request_fingerprint(operation, &workspace_id);
    let stale_before = Utc::now() - chrono::Duration::seconds(IDEMPOTENCY_PREPARATION_DEADLINE_SECS);

    match state.db.find_suppression_idempotency(&workspace_id, &key).await {
        Ok(Some(record)) => {
            // A key already attached to a different intent keeps its 409.
            if record.operation != operation.as_str() || record.request_fingerprint != fingerprint {
                return replay_or_conflict(record, operation, &fingerprint);
            }
            if record.result.is_some() {
                return replay_or_conflict(record, operation, &fingerprint);
            }
            // Preparation crashed or its result write failed. Once the
            // server-owned timestamp passes the deadline, this retry may
            // reclaim the reservation instead of returning 409 forever.
            if record.updated_at <= stale_before {
                if let Err(error) = state
                    .db
                    .reclaim_stale_suppression_idempotency(&workspace_id, operation.as_str(), &key, stale_before)
                    .await
                {
                    return internal(error);
                }
            } else {
                return problem(
                    StatusCode::CONFLICT,
                    "wake_in_flight",
                    "相同请求正在准备中，请稍后使用同一 Idempotency-Key 重试",
                );
            }
        }
        Ok(None) => {}
        Err(error) => return internal(error),
    }

    match state
        .db
        .try_insert_suppression_idempotency(
            &workspace_id,
            operation.as_str(),
            &key,
            &fingerprint,
            schedule.next_wake_at,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            // A concurrent identical request won the unique constraint. Read
            // its recorded result. The winner records it after the synchronous
            // attempt allocation (bounded by the 4s preflight deadline, so
            // wait up to 10s). A winner that crashed leaves a null-result row;
            // that row is reclaimed by a later retry once the server-owned
            // preparation deadline passes.
            for _ in 0..500 {
                match state.db.find_suppression_idempotency(&workspace_id, &key).await {
                    Ok(Some(record)) => {
                        if record.result.is_some() {
                            return replay_or_conflict(record, operation, &fingerprint);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => return internal(error),
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            return problem(
                StatusCode::CONFLICT,
                "wake_in_flight",
                "相同请求正在处理中，请稍后使用同一 Idempotency-Key 重试",
            );
        }
        Err(error) => return internal(error),
    }

    match ops.start_manual(&workspace_id, operation).await {
        Ok(start) => {
            let result = accepted_body(&start.session_key, &start.turn_id, &workspace_id);
            // Persist the durable attempt id (recovery joins this against
            // suppression_state.latest_attempt_id); the executed turn id stays
            // in the recorded result. Retry transient write failures; if all
            // attempts fail the null-result reservation is reclaimable after
            // the preparation deadline instead of wedging the key forever.
            let mut recorded = false;
            for attempt_no in 0..3 {
                match state
                    .db
                    .record_suppression_idempotency(&workspace_id, operation.as_str(), &key, &start.attempt_id, &result)
                    .await
                {
                    Ok(()) => {
                        recorded = true;
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(
                            %workspace_id,
                            attempt_no,
                            %error,
                            "record suppression idempotency failed"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                }
            }
            if !recorded {
                tracing::error!(
                    %workspace_id,
                    turn_id = %start.turn_id,
                    attempt_id = %start.attempt_id,
                    "suppression idempotency result could not be recorded; stale reservation is reclaimable after the preparation deadline"
                );
            }
            (StatusCode::ACCEPTED, Json(result)).into_response()
        }
        Err(error) => {
            let _ = state
                .db
                .delete_suppression_idempotency(&workspace_id, operation.as_str(), &key)
                .await;
            manual_error(error)
        }
    }
}

fn manual_error(error: ManualStartError) -> Response {
    match error {
        ManualStartError::InFlight => problem(
            StatusCode::CONFLICT,
            "wake_in_flight",
            "该工作空间已有 Turn 在途；请等待完成并使用同一 Idempotency-Key 重试",
        ),
        ManualStartError::Backpressure { retry_after_ms } => {
            let body = json!({
                "type": "about:blank",
                "title": StatusCode::TOO_MANY_REQUESTS.canonical_reason().unwrap_or("Too Many Requests"),
                "status": StatusCode::TOO_MANY_REQUESTS.as_u16(),
                "code": "wake_backpressure",
                "detail": "preflight 容量已满；请按 Retry-After 使用同一 Idempotency-Key 重试",
                "retryAfterMs": retry_after_ms,
            });
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::CONTENT_TYPE, "application/problem+json")],
                Json(body),
            )
                .into_response();
            let seconds = retry_after_ms.div_ceil(1000).max(1);
            if let Ok(value) = axum::http::HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
            response
        }
        ManualStartError::Unavailable(detail) => {
            tracing::warn!(%detail, "manual suppression operation unavailable");
            problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "suppression_unavailable",
                "suppression 操作暂不可用",
            )
        }
    }
}

// ── Gate evidence authoring (Task 10 / R20) ──
//
// These endpoints only append evidence: they never change suppression mode and
// never bypass PUT gate validation. Metrics are derived from server-recorded
// observations; the caller supplies references plus human provenance only.

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealGateArtifactRequest {
    /// `shadow` (unlocks canary) or `canary` (unlocks enforce).
    pub mode: String,
    pub sample_turn_ids: Vec<String>,
    #[serde(default)]
    pub baseline_turn_ids: Vec<String>,
    pub from: chrono::DateTime<Utc>,
    pub to: chrono::DateTime<Utc>,
    pub approver: String,
    #[serde(default)]
    pub duplicate_report_review: Option<DuplicateReportLabels>,
    #[serde(default)]
    pub false_skip_review: Option<FalseSkipReview>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordFalseSkipRequest {
    pub turn_id: String,
    pub reviewer: String,
    #[serde(default)]
    pub root_cause: String,
}

fn json_request_fingerprint(operation: &str, workspace_id: &str, request: &impl Serialize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"tinyiothub:suppression-gate-http:v1\0");
    hasher.update(operation.as_bytes());
    hasher.update(b"\0");
    hasher.update(workspace_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(request).unwrap_or_default());
    format!("sha256:{:x}", hasher.finalize())
}

/// Reserve an admin operation key. A recorded result is replayed; a
/// concurrent or conflicting reservation is a 409.
#[allow(clippy::result_large_err)]
async fn reserve_admin_idempotency(
    state: &AgentState,
    workspace_id: &str,
    operation: &str,
    key: &str,
    fingerprint: &str,
) -> Result<(), Response> {
    match state.db.find_suppression_idempotency(workspace_id, key).await {
        Ok(Some(record)) => {
            if record.operation != operation || record.request_fingerprint != fingerprint {
                return Err(problem(
                    StatusCode::CONFLICT,
                    "idempotency_conflict",
                    "同一 Idempotency-Key 不能用于不同 operation 或请求指纹；请使用新的 key",
                ));
            }
            if let Some(result) = record.result {
                return Err((StatusCode::CREATED, Json(result)).into_response());
            }
            return Err(problem(
                StatusCode::CONFLICT,
                "wake_in_flight",
                "相同请求正在处理中，请稍后使用同一 Idempotency-Key 重试",
            ));
        }
        Ok(None) => {}
        Err(error) => return Err(internal(error)),
    }
    match state
        .db
        .try_insert_suppression_idempotency(workspace_id, operation, key, fingerprint, None)
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err(problem(
            StatusCode::CONFLICT,
            "wake_in_flight",
            "相同请求正在处理中，请稍后使用同一 Idempotency-Key 重试",
        )),
        Err(error) => Err(internal(error)),
    }
}

async fn complete_admin_idempotency(
    state: &AgentState,
    workspace_id: &str,
    operation: &str,
    key: &str,
    id: &str,
    result: Value,
) -> Response {
    if let Err(error) = state
        .db
        .record_suppression_idempotency(workspace_id, operation, key, id, &result)
        .await
    {
        tracing::error!(%workspace_id, %operation, %error, "record admin idempotency result failed");
    }
    (StatusCode::CREATED, Json(result)).into_response()
}

#[allow(clippy::result_large_err)]
fn validate_unique_turn_ids(ids: &[String], field: &str) -> Result<(), Response> {
    if ids.is_empty() {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            format!("{field} 不能为空"),
        ));
    }
    let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if unique.len() != ids.len() {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            format!("{field} 不能包含重复 turnId"),
        ));
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
async fn seal_gate_artifact_inner(
    state: &AgentState,
    workspace_id: &str,
    request: SealGateArtifactRequest,
) -> Result<(String, Value), Response> {
    let mode = match request.mode.trim() {
        "shadow" => SuppressionGateMode::Shadow,
        "canary" => SuppressionGateMode::Canary,
        _ => {
            return Err(problem(
                StatusCode::BAD_REQUEST,
                "suppression_config_invalid",
                "mode 必须是 shadow 或 canary",
            ));
        }
    };
    if request.approver.trim().is_empty() {
        return Err(gate_problem("gate artifact 需要 approver"));
    }
    if request.from > request.to {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            "from 不能晚于 to",
        ));
    }
    validate_unique_turn_ids(&request.sample_turn_ids, "sampleTurnIds")?;

    let sample = state
        .db
        .load_suppression_turn_observations(workspace_id, &request.sample_turn_ids)
        .await
        .map_err(internal)?;
    if sample.len() != request.sample_turn_ids.len() {
        return Err(gate_problem("sampleTurnIds 引用了缺失或不属于本工作空间的 Turn"));
    }
    let expected_mode = match mode {
        SuppressionGateMode::Shadow => SuppressionMode::Shadow,
        SuppressionGateMode::Canary => SuppressionMode::Canary,
    };
    if sample.iter().any(|observation| observation.mode != expected_mode) {
        return Err(gate_problem("sampleTurnIds 必须全部来自对应 mode 的已记录观测"));
    }
    if sample
        .iter()
        .any(|observation| observation.observed_at < request.from || observation.observed_at > request.to)
    {
        return Err(gate_problem("sampleTurnIds 超出 from/to 范围"));
    }

    let baseline = match mode {
        SuppressionGateMode::Shadow => {
            if !request.baseline_turn_ids.is_empty() {
                return Err(gate_problem("shadow artifact 不接受 baselineTurnIds"));
            }
            Vec::new()
        }
        SuppressionGateMode::Canary => {
            validate_unique_turn_ids(&request.baseline_turn_ids, "baselineTurnIds")?;
            let baseline = state
                .db
                .load_suppression_turn_observations(workspace_id, &request.baseline_turn_ids)
                .await
                .map_err(internal)?;
            if baseline.len() != request.baseline_turn_ids.len() {
                return Err(gate_problem("baselineTurnIds 引用了缺失或不属于本工作空间的 Turn"));
            }
            if baseline
                .iter()
                .any(|observation| observation.mode != SuppressionMode::Shadow)
            {
                return Err(gate_problem("baselineTurnIds 必须全部来自 mode=shadow 观测"));
            }
            if baseline
                .iter()
                .any(|observation| observation.observed_at < request.from || observation.observed_at > request.to)
            {
                return Err(gate_problem("baselineTurnIds 超出 from/to 范围"));
            }
            baseline
        }
    };

    let now = Utc::now();
    let review = match mode {
        SuppressionGateMode::Shadow => FalseSkipReview {
            reviewer: String::new(),
            reviewed_at: now,
            conclusion: tinyiothub_storage::suppression::FALSE_SKIP_CONCLUSION_NONE.to_string(),
            confirmed_false_skips: 0,
            samples: Vec::new(),
        },
        SuppressionGateMode::Canary => {
            let review = request
                .false_skip_review
                .ok_or_else(|| gate_problem("canary artifact 需要 falseSkipReview（reviewer、conclusion、samples）"))?;
            if review.reviewer.trim().is_empty() {
                return Err(gate_problem("falseSkipReview.reviewer 不能为空"));
            }
            let would_skip: HashSet<&str> = sample
                .iter()
                .filter(|observation| observation.would_skip)
                .map(|observation| observation.turn_id.as_str())
                .collect();
            for review_sample in &review.samples {
                if !would_skip.contains(review_sample.turn_id.as_str()) {
                    return Err(gate_problem(format!(
                        "falseSkipReview sample {} 不是样本内的 wouldSkip Turn",
                        review_sample.turn_id
                    )));
                }
            }
            review
        }
    };

    let labels = match mode {
        SuppressionGateMode::Shadow => None,
        SuppressionGateMode::Canary => Some(
            request
                .duplicate_report_review
                .ok_or_else(|| gate_problem("canary artifact 需要 duplicateReportReview（labeler 与人工计数）"))?,
        ),
    };

    let artifact_id = format!("gate:{}:{}:{}", mode.as_str(), workspace_id, Uuid::new_v4());
    let synthetic = state.suppression_gate_allow_synthetic;
    let artifact = match mode {
        SuppressionGateMode::Shadow => derive_shadow_gate_artifact(
            workspace_id,
            artifact_id.clone(),
            &request.approver,
            now,
            synthetic,
            &sample,
        ),
        SuppressionGateMode::Canary => derive_canary_gate_artifact(
            workspace_id,
            artifact_id.clone(),
            &request.approver,
            now,
            synthetic,
            &sample,
            &baseline,
            labels.expect("canary labels"),
            review,
        ),
    }
    .map_err(|error| gate_problem(error.what))?;
    verify_artifact_evidence(&artifact, &sample, &baseline).map_err(|error| gate_problem(error.what))?;
    let artifact = seal_gate_artifact(artifact);
    state
        .db
        .insert_suppression_gate_artifact(&artifact)
        .await
        .map_err(internal)?;

    let result = json!({
        "artifactId": artifact.artifact_id,
        "mode": artifact.mode.as_str(),
        "workspaceId": workspace_id,
        "approvedAt": artifact.approved_at,
        "contentSha256": artifact.content_sha256,
        "synthetic": artifact.sample.synthetic,
        "sampleTurnCount": artifact.sample.turn_ids.len(),
        "baselineTurnCount": artifact.sample.baseline_turn_ids.len(),
    });
    Ok((artifact.artifact_id, result))
}

#[allow(clippy::result_large_err)]
async fn confirmed_false_skip_inner(
    state: &AgentState,
    workspace_id: &str,
    request: &RecordFalseSkipRequest,
) -> Result<(String, Value), Response> {
    if request.turn_id.trim().is_empty() || request.reviewer.trim().is_empty() {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            "turnId 与 reviewer 不能为空",
        ));
    }
    let Some(observation) = state
        .db
        .suppression_turn_observation(workspace_id, &request.turn_id)
        .await
        .map_err(internal)?
    else {
        return Err(problem(
            StatusCode::NOT_FOUND,
            "suppression_config_invalid",
            "turnId 没有对应的 suppression 观测",
        ));
    };
    if !observation.would_skip || !matches!(observation.mode, SuppressionMode::Canary | SuppressionMode::Enforce) {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "suppression_config_invalid",
            "只有 canary/enforce 的 wouldSkip Turn 才能确认 false skip",
        ));
    }
    let Some(ops) = state.suppression_ops.as_ref() else {
        return Err(problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "suppression_unavailable",
            "suppression operations are not wired",
        ));
    };
    ops.record_confirmed_false_skip(workspace_id, &request.turn_id, &request.reviewer, &request.root_cause)
        .await
        .map_err(manual_error)?;
    Ok((
        request.turn_id.clone(),
        json!({
            "code": "false_skip_detected",
            "turnId": request.turn_id,
            "reviewer": request.reviewer,
        }),
    ))
}

pub async fn post_seal_gate_artifact(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<SealGateArtifactRequest>,
) -> Response {
    if let Err(response) = verify_workspace_write_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    let key = match idempotency_key(&headers) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let fingerprint = json_request_fingerprint("gate_seal", &workspace_id, &request);
    if let Err(response) = reserve_admin_idempotency(&state, &workspace_id, "gate_seal", &key, &fingerprint).await {
        return response;
    }
    match seal_gate_artifact_inner(&state, &workspace_id, request).await {
        Ok((artifact_id, result)) => {
            complete_admin_idempotency(&state, &workspace_id, "gate_seal", &key, &artifact_id, result).await
        }
        Err(response) => {
            let _ = state
                .db
                .delete_suppression_idempotency(&workspace_id, "gate_seal", &key)
                .await;
            response
        }
    }
}

pub async fn post_confirmed_false_skip(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<RecordFalseSkipRequest>,
) -> Response {
    if let Err(response) = verify_workspace_write_problem(&state, &claims, &workspace_id).await {
        return response;
    }
    let key = match idempotency_key(&headers) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let fingerprint = json_request_fingerprint("gate_false_skip", &workspace_id, &request);
    if let Err(response) = reserve_admin_idempotency(&state, &workspace_id, "gate_false_skip", &key, &fingerprint).await
    {
        return response;
    }
    match confirmed_false_skip_inner(&state, &workspace_id, &request).await {
        Ok((turn_id, result)) => {
            complete_admin_idempotency(&state, &workspace_id, "gate_false_skip", &key, &turn_id, result).await
        }
        Err(response) => {
            let _ = state
                .db
                .delete_suppression_idempotency(&workspace_id, "gate_false_skip", &key)
                .await;
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_bounds_are_enforced() {
        assert!(validate_interval(60).is_ok());
        assert!(validate_interval(300).is_ok());
        assert!(validate_interval(86_400).is_ok());
        assert!(validate_interval(59).is_err());
        assert!(validate_interval(86_401).is_err());
    }

    #[test]
    fn schedule_dto_serializes_camel_case() {
        let config = SuppressionConfig {
            workspace_id: "ws".to_string(),
            mode: SuppressionMode::Shadow,
            reconciliation_seconds: 1800,
            config_version: 2,
        };
        let schedule = AssistantSchedule {
            workspace_id: "ws".to_string(),
            assistant_id: "__assistant__ws".to_string(),
            interval_seconds: 300,
            enabled: true,
            permission_mode: tinyiothub_storage::policy::PermissionMode::Ask,
            prompt: String::new(),
            next_wake_at: None,
            last_wake_at: None,
            last_outcome: None,
        };
        let dto = ScheduleDto::new(&schedule, &config);
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["intervalSeconds"], 300);
        assert!(value.get("nextWakeAt").is_some());
        assert_eq!(value["suppressionMode"], "shadow");
        assert_eq!(value["reconciliationSeconds"], 1800);
        assert_eq!(value["suppressionConfigVersion"], 2);
    }

    #[test]
    fn invalid_suppression_updates_are_rejected_before_any_write() {
        let request = UpdateScheduleRequest {
            enabled: None,
            interval_seconds: None,
            prompt: None,
            suppression_mode: Some("impossible".to_string()),
            reconciliation_seconds: None,
        };
        assert!(parse_suppression_update(&request).is_err());

        let request = UpdateScheduleRequest {
            enabled: None,
            interval_seconds: None,
            prompt: None,
            suppression_mode: Some("enforce".to_string()),
            reconciliation_seconds: Some(299),
        };
        assert!(parse_suppression_update(&request).is_err());

        let request = UpdateScheduleRequest {
            enabled: None,
            interval_seconds: None,
            prompt: None,
            suppression_mode: Some("off".to_string()),
            reconciliation_seconds: Some(300),
        };
        let (mode, seconds) = parse_suppression_update(&request).unwrap();
        assert_eq!(mode, Some(SuppressionMode::Off));
        assert_eq!(seconds, Some(300));
    }

    #[tokio::test]
    async fn suppression_overload_backpressure_problem_carries_retry_after() {
        let response = manual_error(ManualStartError::Backpressure { retry_after_ms: 250 });
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/problem+json")
        );
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("problem body");
        let body: Value = serde_json::from_slice(&body).expect("problem json");
        assert_eq!(body["status"], 429);
        assert_eq!(body["code"], "wake_backpressure");
        assert_eq!(body["retryAfterMs"], 250);
    }
}
