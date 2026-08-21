// Thing Agent 用户指令端点（T14）
//
// Routes (nested under /workspaces by the composition layer — api/mod.rs):
//   POST /tasks                  {text} → {taskId}（队列满 → 429）
//   GET  /runs?limit=&offset=    分页（limit 默认 50 最大 200）
//   POST /runs/{run_id}/ack      幂等
//   GET/PUT /policy              三态策略读写（PUT 记 updated_by）
//
// 全部 workspace 隔离（verify_workspace_access!）+ admin 角色。
// admin 判定直接 JOIN roles.is_administrator——user/handler.rs 的
// AuthHelper::check_role 依赖 user_roles.role_name/is_active 列，
// 现行 schema 无此二列（查询必失败回落 "user"），不可用。

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tinyiothub_agent::runtime::thing_agent::{EnqueueError, Priority, TriggerSource, WakeSignal};
use tinyiothub_policy::autonomy::{AutonomyMode, AutonomyPolicy};
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::verify_workspace_access;

/// admin 角色判定：用户持有任一 is_administrator 角色。DB 错误 fail-closed。
async fn is_admin(state: &AgentState, user_id: &str) -> bool {
    state
        .db
        .count_user_admin_roles(user_id)
        .await
        .map(|n| n > 0)
        .unwrap_or(false)
}

/// workspace 隔离 + admin 角色组合守卫（V5 先例：越权 403 / 不存在 404）。
macro_rules! verify_agent_admin {
    ($state:expr, $claims:expr, $id:expr) => {{
        verify_workspace_access!($state, $claims, $id);
        if !is_admin(&$state, &$claims.user_id).await {
            return ApiResponseBuilder::error_with_code(403, "需要管理员权限");
        }
    }};
}

// ── POST /{id}/agent/tasks ──

/// Workspace-scoped agent directive routes (`/workspaces/{id}/agent/*`).
/// Registered by the composition layer (api/mod.rs) so the workspace module
/// carries no agent dependency edge (P4.0d).
pub fn create_workspace_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{id}/agent/tasks", post(create_task))
        .route("/{id}/agent/runs", get(list_runs))
        .route("/{id}/agent/runs/{run_id}/ack", post(ack_run))
        .route("/{id}/agent/policy", get(get_policy).put(update_policy))
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub text: String,
}

pub async fn create_task(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_agent_admin!(state, claims, workspace_id);

    let text = req.text.trim();
    if text.is_empty() {
        return ApiResponseBuilder::error_with_code(400, "指令内容不能为空");
    }

    let Some(ref sink) = state.directive_sink else {
        return ApiResponseBuilder::error_with_code(503, "Agent 任务服务未启动");
    };

    let signal = WakeSignal {
        workspace_id: workspace_id.clone(),
        priority: Priority::High,
        source: TriggerSource::UserDirective {
            user_id: claims.user_id.clone(),
            text: text.to_string(),
            session_key: None,
            source: None, // None = chat/API 用户指令（O5：不节流，队列满拒绝）
            problem_key: None,
        },
        dedup_key: None,
    };

    match sink.enqueue(signal) {
        // task_id 为受理凭据；与 agent_runs.run_id 的关联由 T15/T19 落地
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({
            "taskId": uuid::Uuid::new_v4().to_string(),
            "status": "accepted",
        })),
        Err(EnqueueError::Rejected) => ApiResponseBuilder::error_with_code(429, "任务队列已满，请稍后重试"),
        Err(EnqueueError::Duplicate) => ApiResponseBuilder::error_with_code(409, "相同指令已在队列中，请稍候"),
        Err(EnqueueError::Closed) => ApiResponseBuilder::error_with_code(503, "Agent 任务服务已停止"),
        Err(other) => {
            tracing::error!(%workspace_id, "directive enqueue failed: {}", other);
            ApiResponseBuilder::error("指令投递失败")
        }
    }
}

// ── GET /{id}/agent/runs ──

#[derive(Debug, Deserialize)]
pub struct ListRunsParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub use tinyiothub_storage::agent_runs::AgentRunRow;

pub async fn list_runs(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<ListRunsParams>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_agent_admin!(state, claims, workspace_id);

    // 本体工具分页先例（tools/thing.rs clamp_limit）：默认 50 最大 200
    let limit = i64::from(params.limit.unwrap_or(50).clamp(1, 200));
    let offset = i64::from(params.offset.unwrap_or(0));

    let runs = state.db.list_agent_run_rows(&workspace_id, limit, offset).await;

    match runs {
        Ok(runs) => ApiResponseBuilder::success(serde_json::json!({
            "runs": runs,
            "limit": limit,
            "offset": offset,
        })),
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to list agent runs: {}", e);
            ApiResponseBuilder::error("查询运行记录失败")
        }
    }
}

// ── POST /{id}/agent/runs/{run_id}/ack ──

pub async fn ack_run(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, run_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_agent_admin!(state, claims, workspace_id);

    // 存在性 + workspace 归属：不存在与跨区统一 404（不泄露存在性）；
    // 同时取出 problem_key 供 O11 ack 抑制回写（Task 6）。
    let owner: Option<(String, Option<String>)> = state.db.find_agent_run_owner(&run_id).await.unwrap_or(None);
    let problem_key = match owner {
        Some((ref ws, ref pk)) if ws == &workspace_id => pk.clone(),
        _ => return ApiResponseBuilder::error_with_code(404, "运行记录不存在"),
    };

    match state.db.ack_agent_run(&run_id, &claims.user_id).await {
        Ok(first_ack) => {
            // O11 ack 抑制内存真源同步（Task 6，fix round 1 行级保真）：DB
            // ack 成功后按 run_id 标记对应 run 条目，心跳桥 7d 内仅当窗口内
            // 最新 run 已 ack 时抑制投递。
            if let (Some(pk), Some(orchestrator)) = (&problem_key, state.orchestrator.as_ref()) {
                orchestrator.mark_problem_acked(&workspace_id, pk, &run_id);
            }
            // 幂等：重复确认仍 200，firstAck=false 表示本次未改状态
            ApiResponseBuilder::success(serde_json::json!({
                "runId": run_id,
                "acked": true,
                "firstAck": first_ack,
            }))
        }
        Err(e) => {
            tracing::error!(%workspace_id, %run_id, "Failed to ack agent run: {}", e);
            ApiResponseBuilder::error("确认运行记录失败")
        }
    }
}

// ── GET/PUT /{id}/agent/policy ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub mode: String,
    #[serde(default = "default_allowed_actions")]
    pub allowed_actions: Vec<String>,
    #[serde(default)]
    pub denied_actions: Vec<String>,
    #[serde(default = "default_max_actions_per_run")]
    pub max_actions_per_run: u32,
    #[serde(default = "default_max_actions_per_hour")]
    pub max_actions_per_hour: u32,
}

fn default_allowed_actions() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_max_actions_per_run() -> u32 {
    3
}

fn default_max_actions_per_hour() -> u32 {
    30
}

impl PolicyView {
    /// 无持久化行时的默认策略（与 migration 默认值一致）
    fn default_off() -> Self {
        Self {
            mode: "off".to_string(),
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec![],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }
}

impl From<AutonomyPolicy> for PolicyView {
    fn from(p: AutonomyPolicy) -> Self {
        Self {
            mode: p.mode.as_str().to_string(),
            allowed_actions: p.allowed_actions,
            denied_actions: p.denied_actions,
            max_actions_per_run: p.max_actions_per_run,
            max_actions_per_hour: p.max_actions_per_hour,
        }
    }
}

pub async fn get_policy(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<PolicyView>> {
    verify_agent_admin!(state, claims, workspace_id);

    match state.db.load_autonomy_policy(&workspace_id).await {
        Ok(Some(policy)) => ApiResponseBuilder::success(PolicyView::from(policy)),
        Ok(None) => ApiResponseBuilder::success(PolicyView::default_off()),
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to load autonomy policy: {}", e);
            ApiResponseBuilder::error("读取策略失败")
        }
    }
}

pub async fn update_policy(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<PolicyView>,
) -> Json<ApiResponse<PolicyView>> {
    verify_agent_admin!(state, claims, workspace_id);

    let Some(mode) = AutonomyMode::from_db(&req.mode) else {
        return ApiResponseBuilder::error_with_code(400, "无效的 mode，可选: off / diagnose / act");
    };
    let policy = AutonomyPolicy {
        mode,
        allowed_actions: req.allowed_actions,
        denied_actions: req.denied_actions,
        max_actions_per_run: req.max_actions_per_run,
        max_actions_per_hour: req.max_actions_per_hour,
    };

    if let Err(e) = state
        .db
        .save_autonomy_policy(&workspace_id, &policy, &claims.user_id)
        .await
    {
        tracing::error!(%workspace_id, "Failed to save autonomy policy: {}", e);
        return ApiResponseBuilder::error("保存策略失败");
    }

    // O26 kill switch：mode 切到 off 时清空该工作区调度器的待处理队列
    // （不取消在跑的 run，drain 返回时其已完成）。sink 未接线时跳过。
    if policy.mode == AutonomyMode::Off
        && let Some(sink) = state.directive_sink.as_ref()
    {
        sink.drain(&workspace_id).await;
        tracing::info!(
            metric = "agent_queue_drained",
            workspace_id = %workspace_id,
            "policy mode→off：待处理队列已清空（O26）"
        );
    }

    ApiResponseBuilder::success(PolicyView::from(policy))
}
