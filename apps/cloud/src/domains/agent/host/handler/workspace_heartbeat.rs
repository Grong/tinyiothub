// Heartbeat handlers — per-workspace AI autonomous inspection endpoints
//
// Routes (registered under /workspaces/{id}/heartbeat):
//   GET  /config — read heartbeat config + tasks
//   PUT  /config — update enabled/intervalMinutes
//   GET  /logs  — query heartbeat execution history
//   GET  /tasks — read heartbeat tasks (DB)
//   PUT  /tasks — replace heartbeat tasks (DB)

use crate::domains::agent::host::heartbeat;
use crate::verify_workspace_access_port;
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde::{Deserialize, Serialize};
use tinyiothub_agent::runtime::heartbeat::types::NewHeartbeatTask;
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::agent::AgentState;
use tinyiothub_agent::prompt::paths;

/// Heartbeat routes (`/{id}/heartbeat/*`), nested at `/workspaces` by the
/// composition layer next to `tinyiothub_tenant::workspace_router()` —
/// route-equivalent to the former in-module registration.
pub fn create_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/{id}/heartbeat/config", get(get_config))
        .route("/{id}/heartbeat/config", put(update_config))
        .route("/{id}/heartbeat/trust", get(get_trust_config))
        .route("/{id}/heartbeat/trust", put(update_trust_config))
        .route("/{id}/heartbeat/logs", get(get_logs))
        .route("/{id}/heartbeat/tasks", get(get_tasks))
        .route("/{id}/heartbeat/tasks", put(update_tasks))
        .route("/{id}/heartbeat/approvals", get(get_approvals))
        .route(
            "/{id}/heartbeat/approvals/{proposal_id}/approve",
            post(approve_proposal),
        )
        .route("/{id}/heartbeat/approvals/{proposal_id}/reject", post(reject_proposal))
}

// ── Response types ──

/// A heartbeat task as exposed by this API (priority/text/paused only;
/// server-assigned fields like id/version/timestamps never leave the wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTaskDef {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfigResponse {
    enabled: bool,
    interval_minutes: u32,
    workspace_id: String,
    agent_id: String,
    tasks: Vec<HeartbeatTaskDef>,
    /// 最近一次 tick 完成时间（D13 实时读：runner 内存态；无 tick 过为 null）
    last_tick: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHeartbeatConfigRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub interval_minutes: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatLogEntry {
    timestamp: String,
    task_count: u32,
    status: String,
    error_message: Option<String>,
    result: Option<String>,
    auto_executed: Vec<ActionDetail>,
    pending_proposals: Vec<ProposalDetail>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDetail {
    tool: String,
    thing_id: String,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDetail {
    level: String,
    tool_name: String,
    thing_id: String,
    device_name: String,
    summary: String,
    reason: String,
    risk: String,
    status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatLogsResponse {
    logs: Vec<HeartbeatLogEntry>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHeartbeatTasksRequest {
    pub tasks: Vec<HeartbeatTaskDef>,
}

// ── GET /{id}/heartbeat/config ──

pub async fn get_config(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let tasks = load_tasks(&state, &workspace_id).await;

    let enabled = state
        .heartbeat_runner
        .as_ref()
        .map(|pm| pm.active_workspaces().contains(&workspace_id))
        .unwrap_or(false);

    let mut interval_minutes = 15;
    if let Some(ref runner) = state.heartbeat_runner {
        interval_minutes = runner.effective_interval_minutes(&workspace_id);
    }

    // D13 实时字段：last_tick 读 runner 内存出口（历史/归档仍读 DB，见 /logs）。
    let last_tick = state
        .heartbeat_runner
        .as_ref()
        .and_then(|r| r.last_tick(&workspace_id))
        .map(|t| t.to_rfc3339());

    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled,
        interval_minutes,
        workspace_id: workspace_id.clone(),
        agent_id: "default".to_string(),
        tasks,
        last_tick,
    })
}

// ── PUT /{id}/heartbeat/config ──

pub async fn update_config(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatConfigRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let Some(ref runner) = state.heartbeat_runner else {
        return ApiResponseBuilder::error("心跳服务未启用");
    };

    // Merge with persisted/current values so a partial update doesn't reset
    // the other field.
    let current_interval = runner.effective_interval_minutes(&workspace_id);
    let is_active = runner.active_workspaces().contains(&workspace_id);
    let enabled = req.enabled.unwrap_or(is_active);
    let interval = req.interval_minutes.unwrap_or(current_interval);

    let config = match tinyiothub_storage::heartbeat::WorkspaceHeartbeatConfig::validated(enabled, interval) {
        Ok(c) => c,
        Err(e) => return ApiResponseBuilder::error(&e),
    };
    // D11-⑤ 写序：先写 DB，成功后更新 runner 内存（Task 5 起 runner 不触库）。
    if let Err(e) = state.db.save_heartbeat_config(&workspace_id, &config).await {
        tracing::error!(%workspace_id, "Failed to persist heartbeat config: {}", e);
        return ApiResponseBuilder::error("保存心跳配置失败");
    }
    runner.set_interval_minutes(&workspace_id, interval);

    let interval_changed = interval != current_interval;
    if enabled && (!is_active || interval_changed) {
        // (Re)start so a changed interval takes effect immediately.
        runner.start(&workspace_id).await;
    } else if !enabled && is_active {
        runner.stop(&workspace_id).await;
    }

    ApiResponseBuilder::success(serde_json::json!({
        "enabled": enabled,
        "intervalMinutes": interval,
    }))
}

// ── GET /{id}/heartbeat/trust ──

pub async fn get_trust_config(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let config = match state.heartbeat_runner {
        Some(ref runner) => match runner.get_trust_config(&workspace_id) {
            Some(c) => c,
            None => state
                .db
                .load_heartbeat_trust_config(&workspace_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
        },
        None => tinyiothub_core::heartbeat::TrustConfig::default(),
    };

    ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default())
}

// ── PUT /{id}/heartbeat/trust ──

pub async fn update_trust_config(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(config): Json<tinyiothub_core::heartbeat::TrustConfig>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let Some(ref runner) = state.heartbeat_runner else {
        return ApiResponseBuilder::error("心跳服务未启用");
    };

    // D11-⑤ 写序：先写 DB，成功后更新 runner 内存（Task 5 起 runner 不触库；
    // 内存更新热更 pool + 通知运行中 loop）。
    if let Err(e) = state.db.save_heartbeat_trust_config(&workspace_id, &config).await {
        tracing::error!(%workspace_id, "Failed to persist trust config: {}", e);
        return ApiResponseBuilder::error("保存信任配置失败");
    }
    runner.update_trust_config(&workspace_id, config.clone());

    ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default())
}

// ── GET /{id}/heartbeat/logs ──

pub async fn get_logs(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatLogsResponse>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    // Fetch all heartbeat rows (summary + error + auto_executed + proposal)
    let rows = state.db.list_agent_heartbeat_actions(&workspace_id).await;

    let logs = match rows {
        Ok(rows) => {
            // Group rows by timestamp — summary/error rows drive the timeline,
            // auto_executed/proposal rows from the same tick are nested inside
            let mut summaries: Vec<(String, String, String)> = Vec::new(); // (status, content, created_at)
            let mut details: std::collections::HashMap<String, Vec<(String, String)>> =
                std::collections::HashMap::new(); // created_at -> [(action_type, content)]

            for (action_type, content, created_at) in rows {
                match action_type.as_str() {
                    "summary" | "error" => {
                        summaries.push((action_type, content, created_at));
                    }
                    "auto_executed" | "proposal" => {
                        details
                            .entry(created_at.clone())
                            .or_default()
                            .push((action_type, content));
                    }
                    _ => {}
                }
            }

            summaries.truncate(50); // cap at 50 timeline entries

            summaries
                .into_iter()
                .map(|(action_type, content, created_at)| {
                    let status = if action_type == "error" { "error" } else { "success" };
                    let (task_count, message) = parse_action_content(&content);

                    let related = details.remove(&created_at).unwrap_or_default();
                    let mut auto_executed = Vec::new();
                    let mut pending_proposals = Vec::new();

                    for (a_type, a_content) in related {
                        match a_type.as_str() {
                            "auto_executed" => {
                                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&a_content) {
                                    auto_executed.push(ActionDetail {
                                        tool: parsed.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        thing_id: parsed
                                            .get("thingId")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        summary: parsed
                                            .get("summary")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                    });
                                }
                            }
                            "proposal" => {
                                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&a_content) {
                                    pending_proposals.push(ProposalDetail {
                                        level: parsed.get("level").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        tool_name: parsed
                                            .get("toolName")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        thing_id: parsed
                                            .get("thingId")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        device_name: parsed
                                            .get("deviceName")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        summary: parsed
                                            .get("summary")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        reason: parsed.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        risk: parsed.get("risk").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        status: parsed.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }

                    HeartbeatLogEntry {
                        timestamp: created_at,
                        task_count,
                        status: status.to_string(),
                        error_message: if status == "error" { message.clone() } else { None },
                        result: if status == "success" { message } else { None },
                        auto_executed,
                        pending_proposals,
                    }
                })
                .collect()
        }
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to query heartbeat logs: {}", e);
            vec![]
        }
    };

    ApiResponseBuilder::success(HeartbeatLogsResponse { logs })
}

// ── GET /{id}/heartbeat/tasks ──

pub async fn get_tasks(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<Vec<HeartbeatTaskDef>>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let tasks = load_tasks(&state, &workspace_id).await;

    ApiResponseBuilder::success(tasks)
}

// ── PUT /{id}/heartbeat/tasks ──

pub async fn update_tasks(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatTasksRequest>,
) -> Json<ApiResponse<Vec<HeartbeatTaskDef>>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let Some(ref runner) = state.heartbeat_runner else {
        return ApiResponseBuilder::error("心跳服务未启用");
    };

    let new_tasks: Vec<NewHeartbeatTask> = req
        .tasks
        .iter()
        .map(|t| NewHeartbeatTask {
            priority: t.priority.clone(),
            text: t.text.clone(),
            paused: t.paused,
        })
        .collect();

    // D11-⑤ 写序：先写 DB，成功后注入 runner 内存（set_tasks 内含
    // ReloadTasks 通知，运行中 loop 重读内存）。
    if let Err(e) = state.db.replace_heartbeat_tasks(&workspace_id, &new_tasks).await {
        tracing::error!(%workspace_id, "Failed to save heartbeat tasks: {}", e);
        return ApiResponseBuilder::error("保存心跳任务失败");
    }

    // 回读 DB 行（含 id/version 等 server 字段）作为内存真源。回读失败
    // 必须返错：DB 已是真源，内存未更新可安全报错；吞掉会把空集注入内存
    // 且客户端误收 success（Task 5 fix round 1）。
    let stored = match state.db.list_heartbeat_tasks(&workspace_id).await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to read back heartbeat tasks: {}", e);
            return ApiResponseBuilder::error("读取心跳任务失败");
        }
    };
    let has_tasks = !stored.is_empty();
    runner.set_tasks(&workspace_id, stored);
    if has_tasks && !runner.active_workspaces().contains(&workspace_id) {
        runner.start(&workspace_id).await;
    }

    ApiResponseBuilder::success(req.tasks)
}

// ── GET /{id}/heartbeat/approvals ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalResponse {
    proposal_id: String,
    status: String,
    level: String,
    tool_name: String,
    thing_id: String,
    device_name: String,
    summary: String,
    reason: String,
    risk: String,
    created_at: String,
    parameters: serde_json::Value,
}

/// Map a stored proposal row to its API shape. Returns None for non-pending
/// proposals and unparseable content. `parameters` is included verbatim so
/// the approver can see exactly what they are signing off on.
fn proposal_from_row(content: &str, created_at: String) -> Option<ProposalResponse> {
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
    if status != "pending" {
        return None;
    }
    let str_field = |key: &str| parsed.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some(ProposalResponse {
        proposal_id: str_field("proposalId"),
        status: status.to_string(),
        level: str_field("level"),
        tool_name: str_field("toolName"),
        thing_id: str_field("thingId"),
        device_name: str_field("deviceName"),
        summary: str_field("summary"),
        reason: str_field("reason"),
        risk: str_field("risk"),
        created_at,
        parameters: parsed.get("parameters").cloned().unwrap_or(serde_json::json!({})),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalsResponse {
    proposals: Vec<ProposalResponse>,
}

pub async fn get_approvals(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<ApprovalsResponse>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    let rows = state.db.list_agent_proposal_actions(&workspace_id).await;

    let proposals = match rows {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|(_, content, created_at)| proposal_from_row(&content, created_at))
            .collect(),
        Err(e) => {
            tracing::error!(%workspace_id, "Failed to query proposals: {}", e);
            vec![]
        }
    };

    ApiResponseBuilder::success(ApprovalsResponse { proposals })
}

// ── POST /{id}/heartbeat/approvals/{proposal_id}/approve ──

pub async fn approve_proposal(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, proposal_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);
    // F9：批准 = 物理动作授权，强制 admin；角色查询失败 fail-closed 403。
    if !super::is_admin(&state.db, &claims.user_id).await {
        return ApiResponseBuilder::error_with_code(403, "需要管理员权限");
    }

    // 与 agent 会话同源的工具注册表（内建 thing 工具 + MCP 适配器）——
    // 2026-09-24 前只查 MCP registry，thing 工具 9 件套里 8 个批准即
    // 「未注册」自动拒绝（词表错位：提案 tool_name 来自 agent 会话工具）。
    let registry = state.agent_pool.tool_registry();
    let runtime = state.agent_pool.runtime_context().await;
    match approve_and_execute(
        state.db.pool(),
        &workspace_id,
        &proposal_id,
        &registry,
        &runtime,
        &state.pending_actions,
    )
    .await
    {
        Ok(output) => ApiResponseBuilder::success(serde_json::json!({
            "status": "approved",
            "output": output,
        })),
        Err(e) => ApiResponseBuilder::error(&e),
    }
}

/// 批准通道不可执行的工具——编排工具（dispatch_thing_task，宪法禁止提案
/// 使用）与会话上下文工具（canvas 会把 LLM 写的 UI JSON 推进工作区画布；
/// get_skill 只在会话里有意义）。即使 agent 会话里注册了也不经批准通道
/// 执行，走「未注册」自动拒绝路径。
const APPROVAL_TOOL_DENYLIST: &[&str] = &["dispatch_thing_task", "canvas", "get_skill"];

/// Approve a pending proposal and execute its tool with the stored parameters.
/// The human approval IS the authorization, so execution bypasses the trust
/// engine. The status flip is a conditional UPDATE so a concurrent approve
/// cannot double-execute.
async fn approve_and_execute(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    proposal_id: &str,
    registry: &tinyiothub_agent::tools::ToolRegistry,
    runtime: &tinyiothub_agent::tools::ToolRuntimeContext,
    pending_actions: &crate::domains::agent::host::tools::thing::PendingActionStore,
) -> Result<serde_json::Value, String> {
    let db = tinyiothub_storage::Db::new(pool.clone());
    let row: Option<(String, String)> = db
        .find_agent_proposal(workspace_id, proposal_id)
        .await
        .map_err(|e| format!("查询失败: {}", e))?;

    let Some((id, content)) = row else {
        return Err("提案不存在".to_string());
    };
    let parsed: serde_json::Value = serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e))?;
    if parsed["status"].as_str() != Some("pending") {
        return Err("提案已处理".to_string());
    }
    let tool_name = parsed["toolName"].as_str().unwrap_or("").to_string();
    let thing_id = parsed["thingId"].as_str().map(str::to_string);
    let params = parsed.get("parameters").cloned().unwrap_or(serde_json::json!({}));

    // 盲签锚点一致性（对抗评审 F3）：顶层 thingId 是审批界面的显示锚点，
    // parameters 里的 thingId/thing_id 才是执行目标。两者都在且不一致时
    // 拒绝——人批的必须是实际执行的。
    if let Some(anchor) = &thing_id {
        let exec_target = params
            .get("thingId")
            .or_else(|| params.get("thing_id"))
            .and_then(|v| v.as_str());
        if let Some(target) = exec_target
            && target != anchor
        {
            return Err(format!(
                "提案参数不一致：界面显示 {anchor}，参数执行 {target}——已拒绝执行"
            ));
        }
    }

    // 解析工具：内建（thing 工具 9 件套等）优先、MCP 适配器兜底——与
    // agent 会话 load_all_tools 的碰撞规则一致。
    let tool = if APPROVAL_TOOL_DENYLIST.contains(&tool_name.as_str()) {
        None
    } else {
        registry
            .load_all_tools(workspace_id, runtime)
            .await
            .into_iter()
            .find(|t| t.name() == tool_name)
    };
    let Some(tool) = tool else {
        // 提案引用了不可执行的工具（LLM 幻觉工具名，如 dispatch_thing_task——
        // 2026-09-20 实测）：自动拒绝并留痕，不让它永远挂在待审批队列里
        // 报同一个错。条件翻转（F4）：并发下已被处理的提案不被覆盖。
        let reason = format!("工具 {tool_name} 不在可执行注册表中");
        match db
            .flip_agent_proposal_status(&id, "pending", "rejected", Some(&reason))
            .await
        {
            Ok(0) => {
                tracing::warn!(%workspace_id, %proposal_id, "auto-reject skipped: proposal already processed");
                return Err("提案已处理".to_string());
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(%workspace_id, %proposal_id, error = %e, "auto-reject unregistered-tool proposal failed");
            }
        }
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let outcome_content = serde_json::json!({
            "tool": tool_name,
            "thingId": thing_id,
            "summary": reason,
            "success": false,
            "source": "unregistered_tool",
            "proposalId": proposal_id,
        });
        let _ = db
            .insert_agent_heartbeat_outcome(workspace_id, outcome_content.to_string(), &now)
            .await
            .inspect_err(
                |e| tracing::warn!(%workspace_id, %proposal_id, error = %e, "auto-reject outcome insert failed"),
            );
        return Err(format!("提案已自动拒绝：{reason}"));
    };

    // Atomic flip: only a row still pending transitions, so a second approve
    // affects 0 rows and never re-executes.
    let flipped = db
        .flip_agent_proposal_approved(&id)
        .await
        .map_err(|e| format!("更新失败: {}", e))?;
    if flipped == 0 {
        return Err("提案已处理".to_string());
    }

    let outcome = execute_approved_tool(tool.as_ref(), params, workspace_id, runtime, pending_actions).await;
    let (success, summary) = match &outcome {
        Ok(v) => {
            let s = v.to_string();
            (true, s.chars().take(500).collect::<String>())
        }
        Err(e) => (false, e.to_string()),
    };

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let outcome_content = serde_json::json!({
        "tool": tool_name,
        "thingId": thing_id,
        "summary": summary,
        "success": success,
        "source": "approved_proposal",
        "proposalId": proposal_id,
    });
    if let Err(e) = db
        .insert_agent_heartbeat_outcome(workspace_id, outcome_content.to_string(), &now)
        .await
    {
        tracing::error!(%workspace_id, %proposal_id, "Failed to record proposal execution: {}", e);
    }

    outcome.map_err(|e| format!("执行失败: {}", e))
}

/// 执行已批准的工具。invoke_action 类工具若返回 confirmation_required +
/// token，直接消费 token 下发——人工批准即授权，替代 chat 的二次确认
/// （复用 autonomous_invoke 的 auto-confirm 语义与 dispatch 尾巴）。
async fn execute_approved_tool(
    tool: &dyn tinyiothub_agent::port::tool::Tool,
    params: serde_json::Value,
    workspace_id: &str,
    runtime: &tinyiothub_agent::tools::ToolRuntimeContext,
    pending_actions: &crate::domains::agent::host::tools::thing::PendingActionStore,
) -> Result<serde_json::Value, String> {
    fn output_json(output: String) -> serde_json::Value {
        serde_json::from_str(&output).unwrap_or(serde_json::json!({ "output": output }))
    }

    let result = tool.execute(params).await.map_err(|e| e.to_string())?;
    if !result.success {
        return Err(result.error.unwrap_or(result.output));
    }
    let mut value = output_json(result.output);
    if value.get("status").and_then(|s| s.as_str()) == Some("confirmation_required") {
        let Some(token) = value.get("token").and_then(|t| t.as_str()) else {
            return Err("auto-confirm failed: token missing; action NOT dispatched".to_string());
        };
        let pending = crate::domains::agent::host::tools::thing::take_pending_action(pending_actions, token)
            .ok_or_else(|| "auto-confirm failed: token mismatch or expired; action NOT dispatched".to_string())?;
        // 跨工作区绑定（对抗评审 F1）：token store 是全实例共享的，
        // 任何工具（含 MCP 适配器）返回的 token 都必须属于本工作区，
        // 否则一次批准点击会把别的工作区的设备命令下发出去。
        if pending.workspace_id != workspace_id {
            return Err("auto-confirm failed: workspace mismatch; action NOT dispatched".to_string());
        }
        let dispatched = crate::domains::agent::host::tools::autonomous_invoke::dispatch_command(
            runtime.data_server.as_ref(),
            &pending.thing_id,
            &pending.action_name,
            pending.params.as_ref(),
        );
        if !dispatched.success {
            return Err(dispatched.error.unwrap_or(dispatched.output));
        }
        value = output_json(dispatched.output);
    }
    Ok(value)
}

// ── POST /{id}/heartbeat/approvals/{proposal_id}/reject ──

#[derive(serde::Deserialize)]
pub struct RejectProposalRequest {
    reason: String,
}

pub async fn reject_proposal(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, proposal_id)): Path<(String, String)>,
    Json(req): Json<RejectProposalRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    // X2：patrol 拒绝与 judgment 同一契约——必填原因（落 content.dismiss_reason，
    // P2 学习闭环的输入信号）。
    let reason = req.reason.trim();
    if reason.chars().count() < 4 {
        return ApiResponseBuilder::error_with_code(400, "拒绝必须填写原因（至少 4 个字符）");
    }
    if reason.chars().count() > 500 {
        return ApiResponseBuilder::error_with_code(400, "拒绝原因过长（最多 500 字符）");
    }

    match update_proposal_status(state.db.pool(), &workspace_id, &proposal_id, "rejected", Some(reason)).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"status": "rejected"})),
        Err(ProposalFlipError::Conflict) => {
            // F4：提案已被并发处理（如批准已执行）——审计记录不可覆盖，
            // 明确 409 让前端提示刷新，而非静默改写。
            ApiResponseBuilder::error_with_code(409, "提案已处理（状态已变更，请刷新）")
        }
        Err(ProposalFlipError::NotFound) => ApiResponseBuilder::error("提案不存在"),
        Err(ProposalFlipError::Internal(e)) => ApiResponseBuilder::error(&e),
    }
}

/// 提案状态翻转的失败分类——冲突（已处理）与不存在/内部错误对外语义不同。
#[derive(Debug)]
enum ProposalFlipError {
    NotFound,
    Conflict,
    Internal(String),
}

/// 条件翻转提案状态（F4）：只有仍为 pending 的行会被翻转——已批准/已拒绝的
/// 审计记录不可被迟到的写入覆盖。reason 一并落 content（拒绝原因留痕）。
async fn update_proposal_status(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    proposal_id: &str,
    new_status: &str,
    reason: Option<&str>,
) -> Result<(), ProposalFlipError> {
    let db = tinyiothub_storage::Db::new(pool.clone());
    // Push proposal_id filtering to SQL via json_extract instead of
    // fetching up to 100 rows and scanning in Rust.
    let row: Option<(String, String)> = db
        .find_agent_proposal(workspace_id, proposal_id)
        .await
        .map_err(|e| ProposalFlipError::Internal(format!("查询失败: {}", e)))?;

    let Some((id, _)) = row else {
        return Err(ProposalFlipError::NotFound);
    };
    let flipped = db
        .flip_agent_proposal_status(&id, "pending", new_status, reason)
        .await
        .map_err(|e| ProposalFlipError::Internal(format!("更新失败: {}", e)))?;
    if flipped == 0 {
        return Err(ProposalFlipError::Conflict);
    }
    Ok(())
}

// ── Helpers ──

/// DB is the single source of truth for heartbeat tasks. Migrates legacy
/// HEARTBEAT.md on first access; falls back to file/defaults when the
/// heartbeat runner (and thus the repo) is unavailable.
async fn load_tasks(state: &AgentState, workspace_id: &str) -> Vec<HeartbeatTaskDef> {
    fn to_def(t: heartbeat::HeartbeatTask) -> HeartbeatTaskDef {
        HeartbeatTaskDef {
            priority: t.priority,
            text: t.text,
            paused: t.paused,
        }
    }
    if let Some(ref _runner) = state.heartbeat_runner {
        // DB 门面只依赖连接池，就地取用（Task 5 起 runner 不再持有存储句柄）。
        let workspace_dir = paths::workspace_dir(workspace_id);
        if let Err(e) = heartbeat::migrate_file_tasks_to_db(&state.db, workspace_id, &workspace_dir).await {
            tracing::warn!(%workspace_id, "Heartbeat task migration failed: {}", e);
        }
        match state.db.list_heartbeat_tasks(workspace_id).await {
            Ok(tasks) => {
                return tasks
                    .into_iter()
                    .map(|t| HeartbeatTaskDef {
                        priority: t.priority,
                        text: t.text,
                        paused: t.paused,
                    })
                    .collect();
            }
            Err(e) => {
                tracing::warn!(%workspace_id, "Failed to list heartbeat tasks: {}", e);
            }
        }
    }
    let workspace_dir = paths::workspace_dir(workspace_id);
    heartbeat::read_heartbeat_tasks(&workspace_dir)
        .await
        .map(|tasks| tasks.into_iter().map(to_def).collect())
        .unwrap_or_else(|e| {
            tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
            heartbeat::get_default_tasks().into_iter().map(to_def).collect()
        })
}

fn parse_action_content(content: &str) -> (u32, Option<String>) {
    // New format: {"taskCount": N, "result": "..."} or {"taskCount": N, "error": "..."}
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
        let task_count = parsed
            .get("taskCount")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(0);
        let message = parsed
            .get("result")
            .or_else(|| parsed.get("error"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return (task_count, message);
    }
    // Legacy format: plain text content
    (0, Some(content.to_string()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use sqlx::SqlitePool;

    use super::*;
    use crate::domains::agent::host::tools::thing::{PendingActionStore, store_pending_action};
    use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
    use tinyiothub_agent::port::tool::{Tool, ToolResult};
    use tinyiothub_agent::tools::{ToolRegistry, ToolRuntimeContext};
    use tinyiothub_skills::trust::ToolSafety;

    #[derive(Clone)]
    struct RecordingTool {
        name: &'static str,
        calls: Arc<Mutex<Vec<serde_json::Value>>>,
        fail: bool,
        /// 模拟 invoke_action 的确认流：mint token 进该 store 并返回
        /// confirmation_required 载荷。mint 时用的 workspace 可与批准工作区
        /// 不同（跨工作区失配测试）。
        confirm_store: Option<(Arc<PendingActionStore>, &'static str)>,
        /// 返回不带 token 的 confirmation_required 载荷（缺失字段分支）。
        bare_confirm: bool,
    }

    impl Attributable for RecordingTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            self.name
        }
    }

    #[async_trait]
    impl Tool for RecordingTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "test"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
            self.calls.lock().unwrap().push(args.clone());
            if self.fail {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("device offline".into()),
                });
            }
            if let Some((store, ws)) = &self.confirm_store {
                let token = store_pending_action(
                    store,
                    "dev_1".to_string(),
                    "reboot".to_string(),
                    args.get("params").cloned(),
                    ws.to_string(),
                );
                return Ok(ToolResult {
                    success: true,
                    output: serde_json::json!({
                        "status": "confirmation_required",
                        "token": token,
                    })
                    .to_string(),
                    error: None,
                });
            }
            if self.bare_confirm {
                return Ok(ToolResult {
                    success: true,
                    output: serde_json::json!({"status": "confirmation_required"}).to_string(),
                    error: None,
                });
            }
            Ok(ToolResult {
                success: true,
                output: serde_json::json!({"applied": true}).to_string(),
                error: None,
            })
        }
    }

    fn recording_tool(name: &'static str, fail: bool) -> RecordingTool {
        RecordingTool {
            name,
            calls: Arc::new(Mutex::new(vec![])),
            fail,
            confirm_store: None,
            bare_confirm: false,
        }
    }

    /// 与生产同构的解析输入：ToolRegistry（provider 注册内建工具）+
    /// 运行时上下文（无 DataServer → dispatch 走 simulated）+ 确认 store。
    fn fixture(tool: RecordingTool) -> (ToolRegistry, ToolRuntimeContext, Arc<PendingActionStore>) {
        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(move |_, _| {
            vec![(Box::new(tool.clone()) as Box<dyn Tool>, ToolSafety::Write)]
        }));
        (
            registry,
            ToolRuntimeContext::default(),
            Arc::new(PendingActionStore::default()),
        )
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        pool
    }

    async fn seed_proposal(pool: &SqlitePool, proposal_id: &str, status: &str) {
        seed_proposal_with_tool(pool, proposal_id, status, "write_properties").await;
    }

    async fn seed_proposal_with_tool(pool: &SqlitePool, proposal_id: &str, status: &str, tool_name: &str) {
        let content = serde_json::json!({
            "proposalId": proposal_id,
            "status": status,
            "toolName": tool_name,
            "thingId": "dev_1",
            "summary": "set temp",
            "reason": "tune",
            "risk": "medium",
            "parameters": {"thing_id": "dev_1", "properties": {"target_temp": 22}},
        });
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
             VALUES (?, 'ws_1', '__heartbeat__:ws_1', 'heartbeat', 'proposal', ?, '2026-07-20 10:00:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(content.to_string())
        .execute(pool)
        .await
        .unwrap();
    }

    async fn proposal_status(pool: &SqlitePool, proposal_id: &str) -> String {
        let (content,): (String,) = sqlx::query_as(
            "SELECT content FROM agent_actions WHERE action_type = 'proposal' \
             AND json_extract(content, '$.proposalId') = ?",
        )
        .bind(proposal_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        parsed["status"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn approve_executes_tool_with_stored_parameters() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p1", "pending").await;
        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        approve_and_execute(&pool, "ws_1", "p1", &registry, &runtime, &pending)
            .await
            .expect("approve");

        // Handler ran with the persisted parameters
        {
            let calls = calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0]["properties"]["target_temp"], 22);
        }

        // Status flipped to approved
        assert_eq!(proposal_status(&pool, "p1").await, "approved");

        // Outcome recorded as an auto_executed row so the log UI shows it
        let (content,): (String,) =
            sqlx::query_as("SELECT content FROM agent_actions WHERE action_type = 'auto_executed'")
                .fetch_one(&pool)
                .await
                .expect("outcome row");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["tool"], "write_properties");
        assert_eq!(parsed["thingId"], "dev_1");
        assert_eq!(parsed["success"], true);
    }

    #[tokio::test]
    async fn approve_twice_does_not_reexecute() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p1", "pending").await;
        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        approve_and_execute(&pool, "ws_1", "p1", &registry, &runtime, &pending)
            .await
            .unwrap();
        let second = approve_and_execute(&pool, "ws_1", "p1", &registry, &runtime, &pending).await;
        assert!(second.is_err(), "second approve must be rejected");

        assert_eq!(calls.lock().unwrap().len(), 1, "tool must run exactly once");
    }

    #[tokio::test]
    async fn approve_records_failed_execution() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p1", "pending").await;
        let tool = recording_tool("write_properties", true);
        let (registry, runtime, pending) = fixture(tool);

        let result = approve_and_execute(&pool, "ws_1", "p1", &registry, &runtime, &pending).await;
        assert!(result.is_err(), "execution failure must surface");

        let (content,): (String,) =
            sqlx::query_as("SELECT content FROM agent_actions WHERE action_type = 'auto_executed'")
                .fetch_one(&pool)
                .await
                .expect("outcome row");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["success"], false);
        assert!(parsed["summary"].as_str().unwrap().contains("device offline"));
    }

    #[tokio::test]
    async fn approve_unknown_proposal_fails() {
        let pool = test_pool().await;
        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);
        let result = approve_and_execute(&pool, "ws_1", "nope", &registry, &runtime, &pending).await;
        assert!(result.is_err());
        assert!(calls.lock().unwrap().is_empty());
    }

    /// 回归（2026-09-20 实测）：LLM 幻觉工具名的提案在批准时抛「工具未注册」
    /// 裸错误且提案永远卡在 pending。现在自动拒绝并留痕（outcome 行
    /// source=unregistered_tool）。
    #[tokio::test]
    async fn approve_unregistered_tool_auto_rejects_proposal() {
        let pool = test_pool().await;
        // 提案引用注册表里没有的工具
        seed_proposal_with_tool(&pool, "p-hallucinated", "pending", "nonexistent_tool").await;

        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        let result = approve_and_execute(&pool, "ws_1", "p-hallucinated", &registry, &runtime, &pending).await;
        let err = result.expect_err("unregistered tool must error");
        assert!(err.contains("提案已自动拒绝"), "错误应说明自动拒绝: {err}");
        assert!(calls.lock().unwrap().is_empty(), "未注册工具不得执行");

        // 提案已拒绝（不再卡 pending 报同一个错）
        assert_eq!(proposal_status(&pool, "p-hallucinated").await, "rejected");
        // 留痕：outcome 行记录来源与原因
        let (content,): (String,) =
            sqlx::query_as("SELECT content FROM agent_actions WHERE action_type = 'auto_executed'")
                .fetch_one(&pool)
                .await
                .expect("outcome row");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["source"].as_str().unwrap(), "unregistered_tool");
        assert!(parsed["summary"].as_str().unwrap().contains("不在可执行注册表"));
    }

    /// 回归（2026-09-24 实测）：提案 tool_name 来自 agent 会话的 thing 工具
    /// 词表（宪法明文引用 get_thing_profile 等），而批准执行此前只查 MCP
    /// registry——9 件套里 8 个批准即「未注册」。现在批准走与 agent 会话
    /// 同源的 ToolRegistry，内建工具可直接批准执行。
    #[tokio::test]
    async fn approve_builtin_thing_tool_executes() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-profile", "pending", "get_thing_profile").await;

        let tool = recording_tool("get_thing_profile", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        approve_and_execute(&pool, "ws_1", "p-profile", &registry, &runtime, &pending)
            .await
            .expect("内建 thing 工具必须可批准执行");

        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(proposal_status(&pool, "p-profile").await, "approved");
    }

    /// 编排工具（dispatch_thing_task）即使在 agent 会话里注册了，也不得经
    /// 批准通道执行——宪法禁止提案使用它们，走「未注册」自动拒绝路径。
    #[tokio::test]
    async fn approve_orchestration_tool_auto_rejects() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-orch", "pending", "dispatch_thing_task").await;

        let tool = recording_tool("dispatch_thing_task", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        let result = approve_and_execute(&pool, "ws_1", "p-orch", &registry, &runtime, &pending).await;
        let err = result.expect_err("orchestration tool must be denied");
        assert!(err.contains("提案已自动拒绝"), "错误应说明自动拒绝: {err}");
        assert!(calls.lock().unwrap().is_empty(), "编排工具不得经批准通道执行");
        assert_eq!(proposal_status(&pool, "p-orch").await, "rejected");
    }

    /// invoke_action 类工具返回 confirmation_required + token 时，批准即
    /// 授权——自动消费 token 下发（无 DataServer 时 simulated），不再要求
    /// 二次确认。
    #[tokio::test]
    async fn approve_confirmation_required_auto_confirms() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-invoke", "pending", "invoke_action").await;

        // 工具 mint token 到同一个 store（与生产 provider 捕获同一实例同构）
        let pending = Arc::new(PendingActionStore::default());
        let tool = RecordingTool {
            confirm_store: Some((pending.clone(), "ws_1")),
            ..recording_tool("invoke_action", false)
        };
        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(move |_, _| {
            vec![(Box::new(tool.clone()) as Box<dyn Tool>, ToolSafety::Write)]
        }));
        let runtime = ToolRuntimeContext::default();

        let output = approve_and_execute(&pool, "ws_1", "p-invoke", &registry, &runtime, &pending)
            .await
            .expect("auto-confirm must dispatch");

        assert_eq!(output["status"].as_str().unwrap(), "simulated");
        assert_eq!(output["thingId"].as_str().unwrap(), "dev_1");
        assert_eq!(output["actionName"].as_str().unwrap(), "reboot");
        // token 已消费——store 清空
        assert!(pending.is_empty(), "consumed token must leave the store");
    }

    /// token 失配/过期分支（2026-09-27 覆盖率审计 GAP）：工具 mint 的 token
    /// 不在批准路径的 store 里（或已过期）→ 必须报错且不得下发。
    #[tokio::test]
    async fn approve_auto_confirm_token_mismatch_fails_closed() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-badtoken", "pending", "invoke_action").await;

        // 工具 mint 到 store A；批准路径拿的是 store B —— token 取不到。
        let store_a = Arc::new(PendingActionStore::default());
        let pending_b = Arc::new(PendingActionStore::default());
        let tool = RecordingTool {
            confirm_store: Some((store_a, "ws_1")),
            ..recording_tool("invoke_action", false)
        };
        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(move |_, _| {
            vec![(Box::new(tool.clone()) as Box<dyn Tool>, ToolSafety::Write)]
        }));
        let runtime = ToolRuntimeContext::default();

        let result = approve_and_execute(&pool, "ws_1", "p-badtoken", &registry, &runtime, &pending_b).await;
        let err = result.expect_err("token mismatch must fail closed");
        assert!(err.contains("auto-confirm failed"), "{err}");

        // 失败留痕：outcome 行 success=false
        let (content,): (String,) =
            sqlx::query_as("SELECT content FROM agent_actions WHERE action_type = 'auto_executed'")
                .fetch_one(&pool)
                .await
                .expect("outcome row");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["success"], false);
    }

    /// 跨工作区失配（对抗评审 F1 回归）：token 属于别的工作区时，
    /// 批准点击不得下发该设备命令。
    #[tokio::test]
    async fn approve_auto_confirm_workspace_mismatch_fails_closed() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-ws-mismatch", "pending", "invoke_action").await;

        // token mint 自 ws_other；批准发生在 ws_1——同一 store 也绝不能下发。
        let store = Arc::new(PendingActionStore::default());
        let tool = RecordingTool {
            confirm_store: Some((store.clone(), "ws_other")),
            ..recording_tool("invoke_action", false)
        };
        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(move |_, _| {
            vec![(Box::new(tool.clone()) as Box<dyn Tool>, ToolSafety::Write)]
        }));
        let runtime = ToolRuntimeContext::default();

        let result = approve_and_execute(&pool, "ws_1", "p-ws-mismatch", &registry, &runtime, &store).await;
        let err = result.expect_err("workspace mismatch must fail closed");
        assert!(err.contains("workspace mismatch"), "{err}");
    }

    /// confirmation_required 载荷缺 token 字段（testing 评审 GAP）：
    /// 报错且不下发。
    #[tokio::test]
    async fn approve_confirmation_missing_token_fails_closed() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-no-token", "pending", "invoke_action").await;

        let tool = RecordingTool {
            bare_confirm: true,
            ..recording_tool("invoke_action", false)
        };
        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(move |_, _| {
            vec![(Box::new(tool.clone()) as Box<dyn Tool>, ToolSafety::Write)]
        }));
        let runtime = ToolRuntimeContext::default();
        let pending = Arc::new(PendingActionStore::default());

        let result = approve_and_execute(&pool, "ws_1", "p-no-token", &registry, &runtime, &pending).await;
        let err = result.expect_err("missing token must fail closed");
        assert!(err.contains("token missing"), "{err}");
    }

    /// 会话上下文工具（canvas/get_skill）同样不得经批准通道执行——canvas
    /// 会把 LLM 写的 UI JSON 推进工作区画布（对抗评审 F2，2026-09-27 评审
    /// 决策：denylist 扩展而非白名单，保留完整本体/MCP 工具可批准）。
    #[tokio::test]
    async fn approve_canvas_tool_auto_rejects() {
        let pool = test_pool().await;
        seed_proposal_with_tool(&pool, "p-canvas", "pending", "canvas").await;

        let tool = recording_tool("canvas", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        let result = approve_and_execute(&pool, "ws_1", "p-canvas", &registry, &runtime, &pending).await;
        let err = result.expect_err("canvas must be denied");
        assert!(err.contains("提案已自动拒绝"), "{err}");
        assert!(calls.lock().unwrap().is_empty(), "canvas 不得经批准通道执行");
    }

    /// 盲签锚点（对抗评审 F3 回归）：顶层 thingId（界面显示）与 parameters
    /// 里的执行目标不一致 → 拒绝执行、不执行工具、提案保持 pending。
    #[tokio::test]
    async fn approve_thing_id_mismatch_refuses_execution() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p-mismatch", "pending").await;
        // 把参数里的执行目标改成另一台设备（显示 dev_1，执行 dev_prod_9）
        let (id,): (String,) = sqlx::query_as(
            "SELECT id FROM agent_actions WHERE action_type = 'proposal' \
             AND json_extract(content, '$.proposalId') = 'p-mismatch'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let tampered = serde_json::json!({
            "proposalId": "p-mismatch",
            "status": "pending",
            "toolName": "write_properties",
            "thingId": "dev_1",
            "parameters": {"thing_id": "dev_prod_9", "properties": {"target_temp": 22}},
        });
        sqlx::query("UPDATE agent_actions SET content = ? WHERE id = ?")
            .bind(tampered.to_string())
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);

        let result = approve_and_execute(&pool, "ws_1", "p-mismatch", &registry, &runtime, &pending).await;
        let err = result.expect_err("mismatched anchor must refuse");
        assert!(err.contains("参数不一致"), "{err}");
        assert!(calls.lock().unwrap().is_empty(), "不一致提案不得执行");
        assert_eq!(
            proposal_status(&pool, "p-mismatch").await,
            "pending",
            "保持待处理，由人处置"
        );
    }

    /// F4 两向竞态之 A：批准已执行后迟到的 reject 不得覆盖审计——
    /// 条件翻转命中 0 行 → Conflict，content 保持 approved。
    #[tokio::test]
    async fn reject_after_approve_keeps_approved_audit() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p-race-a", "pending").await;
        let tool = recording_tool("write_properties", false);
        let (registry, runtime, pending) = fixture(tool);

        approve_and_execute(&pool, "ws_1", "p-race-a", &registry, &runtime, &pending)
            .await
            .expect("approve");
        assert_eq!(proposal_status(&pool, "p-race-a").await, "approved");

        let result = update_proposal_status(&pool, "ws_1", "p-race-a", "rejected", Some("迟到否决")).await;
        assert!(
            matches!(result, Err(ProposalFlipError::Conflict)),
            "已执行批准不得被 reject 覆盖: {result:?}"
        );
        assert_eq!(
            proposal_status(&pool, "p-race-a").await,
            "approved",
            "审计记录不可变——已执行批准保持 approved"
        );
    }

    /// F4 两向竞态之 B：reject 先落盘后迟到的 approve 不得执行——
    /// 读到的状态非 pending → 拒绝执行，工具零调用。
    #[tokio::test]
    async fn approve_after_reject_fails_closed() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p-race-b", "pending").await;

        update_proposal_status(&pool, "ws_1", "p-race-b", "rejected", Some("先否决"))
            .await
            .expect("reject pending");
        assert_eq!(proposal_status(&pool, "p-race-b").await, "rejected");

        let tool = recording_tool("write_properties", false);
        let calls = tool.calls.clone();
        let (registry, runtime, pending) = fixture(tool);
        let result = approve_and_execute(&pool, "ws_1", "p-race-b", &registry, &runtime, &pending).await;
        assert!(result.expect_err("已拒绝提案不得批准").contains("提案已处理"));
        assert!(calls.lock().unwrap().is_empty(), "已拒绝提案不得执行");
    }

    /// reject 正常路径：pending → rejected 且原因落 content。
    #[tokio::test]
    async fn reject_pending_proposal_records_reason() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p-reject", "pending").await;

        update_proposal_status(&pool, "ws_1", "p-reject", "rejected", Some("误报，无需处理"))
            .await
            .expect("reject");

        let (content,): (String,) = sqlx::query_as(
            "SELECT content FROM agent_actions WHERE action_type = 'proposal' \
             AND json_extract(content, '$.proposalId') = 'p-reject'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["status"], "rejected");
        assert_eq!(parsed["dismiss_reason"], "误报，无需处理");
    }

    #[test]
    fn proposal_from_row_includes_parameters_for_blind_signing() {
        let content = serde_json::json!({
            "proposalId": "p1",
            "status": "pending",
            "level": "high",
            "toolName": "write_properties",
            "thingId": "dev_1",
            "deviceName": "Thermostat",
            "summary": "set temp",
            "reason": "tune",
            "risk": "medium",
            "parameters": {"thing_id": "dev_1", "properties": {"target_temp": 22}},
        })
        .to_string();

        let p = proposal_from_row(&content, "2026-07-20 10:00:00".to_string()).expect("pending proposal maps");

        assert_eq!(p.proposal_id, "p1");
        assert_eq!(p.parameters["properties"]["target_temp"], 22);
    }

    #[test]
    fn proposal_from_row_defaults_missing_parameters_to_empty_object() {
        let content = serde_json::json!({
            "proposalId": "p2",
            "status": "pending",
            "toolName": "reboot",
        })
        .to_string();

        let p = proposal_from_row(&content, "t".to_string()).expect("maps");

        assert_eq!(p.parameters, serde_json::json!({}));
    }

    #[test]
    fn proposal_from_row_skips_non_pending_and_malformed() {
        let approved = serde_json::json!({"proposalId": "p", "status": "approved"}).to_string();
        assert!(proposal_from_row(&approved, "t".to_string()).is_none());
        assert!(proposal_from_row("not json", "t".to_string()).is_none());
    }
}
