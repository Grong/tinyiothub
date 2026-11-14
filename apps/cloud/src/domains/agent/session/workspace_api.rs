//! Workspace 级的 Assistant Schedule、Global HUD 与 suppression 操作端点。
//!
//! 只读聚合：设备/告警/待答/待批/活动会话/最近事件都来自既有存储，不新增状态表。
//! 所有端点先做 workspace 归属校验。Suppression 的决策/执行仍住 session
//! 模块；本文件只做 HTTP DTO、权限、problem details 和幂等投影。

use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tinyiothub_storage::alarm::{AlarmLevel, AlarmQueryCriteria, AlarmStatus};
use tinyiothub_storage::assistant_schedule::{AssistantSchedule, SuppressionConfig};
use tinyiothub_storage::suppression::{
    MAX_RECONCILIATION_SECONDS, MIN_RECONCILIATION_SECONDS, SuppressionMode, SuppressionTurnTerminal,
};
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::domains::agent::session::handlers::is_admin;
use crate::domains::agent::session::hud::{
    HudActivity, HudAlarms, HudAssistant, HudDevices, HudPending, HudResponse, HudSchedule, HudStatusInputs,
    MAX_RECENT_EVENTS, assistant_status,
};
use crate::domains::agent::session::suppression::contract::{SourceDescriptor, SourceSnapshot};
use crate::domains::agent::session::suppression::preflight::PreflightDecision;
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

/// Task 10 owns the canary/enforce gate-artifact validation. The seam is
/// explicit here so this task never invents or bypasses an artifact: today it
/// only recognises the intent to leave the shadow-only modes.
fn validate_gate_artifact_seam(_workspace_id: &str, _mode: SuppressionMode) -> Result<(), Response> {
    // TODO(Task 10): reject canary without a valid shadow artifact and enforce
    // without a valid canary-measured artifact, with a 409
    // `shadow_gate_not_met` problem.
    Ok(())
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
        && let Err(response) = validate_gate_artifact_seam(&workspace_id, mode)
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
    if mode.is_some() || reconciliation_seconds.is_some() {
        if let Err(error) = state
            .db
            .update_suppression_config(&workspace_id, mode, reconciliation_seconds)
            .await
        {
            return internal(error);
        }
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

/// Only a bare snake_case token may leave the backend as an alert code.
fn safe_error_code(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
    {
        Some(value.to_string())
    } else {
        None
    }
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
        alerts: SuppressionAlertDto {
            active: state_row.last_error.is_some() || state_row.full_failed || state_row.audit_in_progress,
            code: safe_error_code(state_row.last_error.as_deref()),
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

/// Intent fingerprint: operation + workspace + the planned wake the request
/// would run. Source contents are deliberately excluded so a retry after
/// Source changes still replays the recorded result.
fn request_fingerprint(
    operation: ManualOperation,
    workspace_id: &str,
    planned_wake_at: Option<chrono::DateTime<Utc>>,
) -> String {
    let planned = planned_wake_at
        .map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(b"tinyiothub:suppression-http-idempotency:v1\0");
    hasher.update(operation.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(workspace_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(planned.as_bytes());
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
    let fingerprint = request_fingerprint(operation, &workspace_id, schedule.next_wake_at);

    match state.db.find_suppression_idempotency(&workspace_id, &key).await {
        Ok(Some(record)) => return replay_or_conflict(record, operation, &fingerprint),
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
            // its recorded result; the winner writes it immediately after the
            // synchronous attempt allocation (bounded by the 4s preflight
            // deadline, so wait up to 10s before reporting in-flight).
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
            if let Err(error) = state
                .db
                .record_suppression_idempotency(&workspace_id, operation.as_str(), &key, &start.turn_id, &result)
                .await
            {
                tracing::warn!(%workspace_id, %error, "record suppression idempotency failed");
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
}
