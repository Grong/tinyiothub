use crate::dto::entity::self_healing::{
            ExecuteSelfHealRequest, ExecuteSelfHealResponse, HealingExecutionDto,
            ProbeConfig, ProbeConfig as ProbeConfigDto, ProbeResultDto, SelfHealingPolicyDto,
        };
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router
};
use serde::Deserialize;

use crate::{
    api::self_healing::get_self_healing_state,
    domain::self_healing::PolicyEvaluator,
    dto::{
        response::{builder::ApiResponseBuilder, ApiResponse}
    },
    shared::{app_state::AppState, security::jwt::Claims}
};

/// Create the self-healing router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Policy
        .route("/policies", get(get_policy))
        .route("/policies", put(update_policy))
        // Actions
        .route("/actions/{level}", post(execute_action))
        // History
        .route("/executions", get(get_executions))
        // Probe
        .route("/probes", get(get_probe_status))
        .route("/probes/config", get(get_probe_config))
        .route("/probes/config", put(update_probe_config))
}

/// Query params for execution history
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// GET /self-healing/policies - Get current policy
async fn get_policy(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SelfHealingPolicyDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let state = state.read().await;
    ApiResponseBuilder::success(SelfHealingPolicyDto::from(&state.policy))
}

/// PUT /self-healing/policies - Update policy
async fn update_policy(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(policy): Json<SelfHealingPolicyDto>,
) -> Json<ApiResponse<SelfHealingPolicyDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let mut state = state.write().await;
    state.policy.enabled = policy.enabled;
    state.evaluator = Arc::new(PolicyEvaluator::new(state.policy.clone()));

    ApiResponseBuilder::success(SelfHealingPolicyDto::from(&state.policy))
}

/// POST /self-healing/actions/{level} - Execute recovery action
async fn execute_action(
    State(_state): State<AppState>,
    _claims: Claims,
    Path(level): Path<String>,
    Json(request): Json<ExecuteSelfHealRequest>,
) -> Json<ApiResponse<ExecuteSelfHealResponse>> {
    use crate::domain::self_healing::{RecoveryActionType, SeverityLevel};

    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let state = state.read().await;

    let severity = match level.to_uppercase().as_str() {
        "L0" => SeverityLevel::L0,
        "L1" => SeverityLevel::L1,
        "L2" => SeverityLevel::L2,
        "L3" => SeverityLevel::L3,
        _ => return ApiResponseBuilder::error("Invalid severity level (L0-L3 required)")
};

    let action_type = match request.action_type.to_lowercase().as_str() {
        "log_only" => RecoveryActionType::LogOnly,
        "restart_driver" => RecoveryActionType::RestartDriver,
        "rejoin_lora" => RecoveryActionType::RejoinLora,
        "reconnect_device" => RecoveryActionType::ReconnectDevice,
        "clean_logs" => RecoveryActionType::CleanLogs,
        "report_cloud" => RecoveryActionType::ReportCloud,
        "create_ticket" => RecoveryActionType::CreateTicket,
        _ => return ApiResponseBuilder::error("Invalid action type")
};

    let cooldown = state.policy.levels.get(&severity)
        .map(|p| p.cooldown_secs)
        .unwrap_or(0);

    // Check require_approval flag — if set, reject manual execution
    if state.policy.levels.get(&severity)
        .map(|p| p.require_approval)
        .unwrap_or(false)
    {
        return ApiResponseBuilder::error("This action requires approval per policy — direct execution not allowed");
    }

    // Clone executor and repository while holding the read lock to avoid race condition
    let executor = state.executor.clone();
    let repository = state.repository.clone();

    drop(state);

    match executor.execute(severity, action_type, request.target.clone(), cooldown).await {
        Ok(execution) => {
            // Persist execution to database
            if let Err(e) = repository.save(&execution).await {
                tracing::error!("Failed to persist healing execution: {}", e);
                // Still return success to caller — execution happened, just recording failed
            }
            ApiResponseBuilder::success(ExecuteSelfHealResponse {
                execution: HealingExecutionDto::from(&execution),
                message: format!("Self-healing action executed successfully"),
            })
        }
        Err(e) => {
            tracing::error!("Self-heal execution failed: {}", e);
            ApiResponseBuilder::error(e.to_string())
        }
    }
}

/// GET /self-healing/executions - Get recovery history
async fn get_executions(
    State(_state): State<AppState>,
    _claims: Claims,
    Query(params): Query<HistoryQuery>,
) -> Json<ApiResponse<Vec<HealingExecutionDto>>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let state = state.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    let tenant_id = "default".to_string();

    match state.repository.get_recent(&tenant_id, limit, offset).await {
        Ok(execs) => {
            let dtos: Vec<HealingExecutionDto> = execs.iter().map(HealingExecutionDto::from).collect();
            ApiResponseBuilder::success(dtos)
        }
        Err(e) => {
            tracing::error!("Failed to fetch healing executions: {}", e);
            ApiResponseBuilder::error("Failed to fetch execution history")
        }
    }
}

/// GET /self-healing/probes - Get current probe status
async fn get_probe_status(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<ProbeResultDto>>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let state = state.read().await;
    let results = state.scheduler.get_all_results().await;
    let dtos: Vec<ProbeResultDto> = results.values().map(ProbeResultDto::from).collect();
    ApiResponseBuilder::success(dtos)
}

/// GET /self-healing/probes/config - Get probe configuration
async fn get_probe_config(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<ProbeConfigDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let state = state.read().await;
    ApiResponseBuilder::success(ProbeConfigDto {
        system_probe_enabled: state.probe_config.system_probe_enabled,
        system_probe_interval_secs: state.probe_config.system_probe_interval_secs,
        device_probe_enabled: state.probe_config.device_probe_enabled,
        device_probe_interval_secs: state.probe_config.device_probe_interval_secs,
        task_probe_enabled: state.probe_config.task_probe_enabled,
        task_probe_interval_secs: state.probe_config.task_probe_interval_secs,
    })
}

/// PUT /self-healing/probes/config - Update probe configuration
async fn update_probe_config(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(config): Json<ProbeConfigDto>,
) -> Json<ApiResponse<ProbeConfigDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized")
};

    let mut state = state.write().await;
    state.probe_config = ProbeConfig {
        system_probe_enabled: config.system_probe_enabled,
        system_probe_interval_secs: config.system_probe_interval_secs,
        device_probe_enabled: config.device_probe_enabled,
        device_probe_interval_secs: config.device_probe_interval_secs,
        task_probe_enabled: config.task_probe_enabled,
        task_probe_interval_secs: config.task_probe_interval_secs
};

    ApiResponseBuilder::success(ProbeConfigDto {
        system_probe_enabled: state.probe_config.system_probe_enabled,
        system_probe_interval_secs: state.probe_config.system_probe_interval_secs,
        device_probe_enabled: state.probe_config.device_probe_enabled,
        device_probe_interval_secs: state.probe_config.device_probe_interval_secs,
        task_probe_enabled: state.probe_config.task_probe_enabled,
        task_probe_interval_secs: state.probe_config.task_probe_interval_secs,
    })
}
