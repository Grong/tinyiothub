//! E1 watch 值守规则 API（Task 8）——`/workspaces/{id}/watch-rules`。
//!
//! 写序（spec §2.1）：校验 fail-loud 400（condition 解析 / 引用物存在性 /
//! 超时上限 / 活跃上限 20）→ watch_rules 表 INSERT（持久源）→ 内存注册表
//! upsert（engine 命令通道，best-effort——loop 未起/watch 未接线跳过，
//! 下次 start 启动重织自愈）。删除：表 DELETE → 注册表移除。
//! 多租户：全部按 workspace 隔离（访问校验 + SQL 双 WHERE 限定）。

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use tinyiothub_agent::runtime::thing_agent::WatchCommand;
use tinyiothub_core::watch::{WatchCondition, WatchRule};
use tinyiothub_runtime::ports::WatchRuleStore;
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use crate::shared::runtime_ports::WatchRuleStoreAdapter;
use crate::verify_workspace_access_port;

/// spec §2.2：每工作区活跃 watch 上限。
const MAX_ACTIVE_WATCHES: usize = 20;
/// spec §2.2：超时默认 600s。
const DEFAULT_TIMEOUT_SECS: u32 = 600;

/// Watch 规则路由（`/{id}/watch-rules*`），由组合层 nest 在 `/workspaces` 下
///（照 workspace_heartbeat 先例）。
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{id}/watch-rules", post(create_rule))
        .route("/{id}/watch-rules", get(list_rules))
        .route("/{id}/watch-rules/{rule_id}", delete(delete_rule))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWatchRuleRequest {
    /// 条件 JSON（词表见 tinyiothub_core::watch::WatchCondition，带 type tag）。
    pub condition: serde_json::Value,
    /// 持久规则（命中后保持）；缺省 false = one-shot 临时 watch。
    #[serde(default)]
    pub persistent: bool,
    /// 超时秒数（默认 600，上限 3600）。
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u32,
}

fn default_timeout_secs() -> u32 {
    DEFAULT_TIMEOUT_SECS
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchRuleResponse {
    pub id: String,
    pub workspace_id: String,
    pub condition: serde_json::Value,
    pub persistent: bool,
    pub timeout_secs: u32,
    pub created_at: String,
}

fn to_response(rule: &WatchRule) -> WatchRuleResponse {
    WatchRuleResponse {
        id: rule.id.clone(),
        workspace_id: rule.workspace_id.clone(),
        condition: serde_json::to_value(&rule.condition).unwrap_or(serde_json::Value::Null),
        persistent: rule.persistent,
        timeout_secs: rule.timeout_secs,
        created_at: rule.created_at.clone(),
    }
}

fn rule_store(state: &AgentState) -> WatchRuleStoreAdapter {
    WatchRuleStoreAdapter((*state.db).clone())
}

// ── POST /{id}/watch-rules ──

pub async fn create_rule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateWatchRuleRequest>,
) -> Json<ApiResponse<WatchRuleResponse>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    // 1. condition 词表解析（fail-loud 400）。
    let condition: WatchCondition = match serde_json::from_value(req.condition) {
        Ok(c) => c,
        Err(e) => return ApiResponseBuilder::error_with_code(400, format!("condition 解析失败: {e}")),
    };
    let rule = WatchRule {
        id: format!("wr_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: workspace_id.clone(),
        condition,
        persistent: req.persistent,
        timeout_secs: req.timeout_secs,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    // 2. 规则校验（timeout_secs 1..=3600，fail-loud 400）。
    if let Err(e) = rule.validate() {
        return ApiResponseBuilder::error_with_code(400, e);
    }
    // 3. 引用物存在性（限定本工作区——多租户隔离，他区物=不存在）。
    let (thing_id, _) = rule.condition.index_key();
    match state.db.find_thing_by_id(Some(&workspace_id), &thing_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return ApiResponseBuilder::error_with_code(400, format!("watch 引用的物不存在：{thing_id}"));
        }
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to query thing for watch rule: {}", e);
            return ApiResponseBuilder::error("查询物失败");
        }
    }
    // 4. 活跃上限（spec §2.2：每工作区 ≤20，fail-loud 400）。
    let store = rule_store(&state);
    let existing = match store.list_all().await {
        Ok(rules) => rules,
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to list watch rules: {}", e);
            return ApiResponseBuilder::error("查询 watch 规则失败");
        }
    };
    if existing.iter().filter(|r| r.workspace_id == workspace_id).count() >= MAX_ACTIVE_WATCHES {
        return ApiResponseBuilder::error_with_code(400, format!("活跃 watch 已达上限 {MAX_ACTIVE_WATCHES}"));
    }
    // 5. 持久源 INSERT（失败 fail-loud，内存不写——两边都不生效）。
    if let Err(e) = store.insert(&rule).await {
        tracing::error!(%workspace_id, "Failed to insert watch rule: {}", e);
        return ApiResponseBuilder::error("保存 watch 规则失败");
    }
    // 6. 内存注册表写穿（best-effort：loop 未起/未接线跳过，下次 start
    //    启动重织自愈；通道满/闭只记日志）。
    if let Some(manager) = state.thing_agents.as_ref()
        && let Some(sink) = manager.watch_command_sink(&workspace_id)
        && let Err(e) = sink.send(WatchCommand::Upsert(rule.clone())).await
    {
        tracing::warn!(%workspace_id, error = %e, "watch 规则内存写穿失败（下次启动重织自愈）");
    }

    ApiResponseBuilder::success(to_response(&rule))
}

// ── GET /{id}/watch-rules ──

pub async fn list_rules(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    match rule_store(&state).list_all().await {
        Ok(rules) => {
            let rules: Vec<WatchRuleResponse> = rules
                .iter()
                .filter(|r| r.workspace_id == workspace_id)
                .map(to_response)
                .collect();
            ApiResponseBuilder::success(serde_json::json!({ "rules": rules }))
        }
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to list watch rules: {}", e);
            ApiResponseBuilder::error("查询 watch 规则失败")
        }
    }
}

// ── DELETE /{id}/watch-rules/{rule_id} ──

pub async fn delete_rule(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, rule_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    // 持久源 DELETE（workspace 双限定——跨区删除落空 404，多租户隔离）。
    match rule_store(&state).delete(&workspace_id, &rule_id).await {
        Ok(true) => {}
        Ok(false) => return ApiResponseBuilder::error_with_code(404, "watch 规则不存在"),
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to delete watch rule: {}", e);
            return ApiResponseBuilder::error("删除 watch 规则失败");
        }
    }
    // 注册表移除（best-effort，同 create 的写穿语义）。
    if let Some(manager) = state.thing_agents.as_ref()
        && let Some(sink) = manager.watch_command_sink(&workspace_id)
        && let Err(e) = sink.send(WatchCommand::Remove(rule_id.clone())).await
    {
        tracing::warn!(%workspace_id, error = %e, "watch 规则移除写穿失败（下次启动重织自愈）");
    }

    ApiResponseBuilder::success(serde_json::json!({ "deleted": true }))
}
