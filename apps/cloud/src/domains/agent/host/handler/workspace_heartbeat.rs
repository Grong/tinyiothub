// 数据实现，留 cloud（D2）
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
    device_id: String,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDetail {
    level: String,
    tool_name: String,
    device_id: String,
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
    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND event_type = 'heartbeat' \
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(&workspace_id)
    .fetch_all(state.db.pool())
    .await;

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
                                        device_id: parsed
                                            .get("deviceId")
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
                                        device_id: parsed
                                            .get("deviceId")
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
    device_id: String,
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
        device_id: str_field("deviceId"),
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

    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&workspace_id)
    .fetch_all(state.db.pool())
    .await;

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

    let Some(registry) = crate::domains::agent::host::ports::external_tool_registry() else {
        return ApiResponseBuilder::error("工具注册表未初始化");
    };
    match approve_and_execute(state.db.pool(), &workspace_id, &proposal_id, &registry).await {
        Ok(output) => ApiResponseBuilder::success(serde_json::json!({
            "status": "approved",
            "output": output,
        })),
        Err(e) => ApiResponseBuilder::error(&e),
    }
}

/// Approve a pending proposal and execute its tool with the stored parameters.
/// The human approval IS the authorization, so execution bypasses the trust
/// engine. The status flip is a conditional UPDATE so a concurrent approve
/// cannot double-execute.
async fn approve_and_execute(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    proposal_id: &str,
    registry: &std::sync::Arc<dyn tinyiothub_agent::tools::ExternalToolRegistry>,
) -> Result<serde_json::Value, String> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, content FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         AND json_extract(content, '$.proposalId') = ? \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(proposal_id)
    .fetch_optional(pool)
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
    let device_id = parsed["deviceId"].as_str().map(str::to_string);
    let params = parsed.get("parameters").cloned().unwrap_or(serde_json::json!({}));

    let handler = registry
        .get_handler(&tool_name)
        .await
        .ok_or_else(|| format!("工具未注册: {}", tool_name))?;

    // Atomic flip: only a row still pending transitions, so a second approve
    // affects 0 rows and never re-executes.
    let flipped = sqlx::query(
        "UPDATE agent_actions SET content = json_set(content, '$.status', 'approved') \
         WHERE id = ? AND json_extract(content, '$.status') = 'pending'",
    )
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| format!("更新失败: {}", e))?;
    if flipped.rows_affected() == 0 {
        return Err("提案已处理".to_string());
    }

    // Execute under the same auth context as the heartbeat agent path — the
    // composition layer's external-tool adapter scopes handler queries by
    // this identity and fails closed without it.
    let ctx = tinyiothub_agent::tools::ExternalToolContext {
        workspace_id: workspace_id.to_string(),
        actor: format!("__heartbeat__:{workspace_id}"),
    };
    let outcome = handler.execute(&ctx, params).await;
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
        "deviceId": device_id,
        "summary": summary,
        "success": success,
        "source": "approved_proposal",
        "proposalId": proposal_id,
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
         VALUES (?, ?, ?, 'heartbeat', 'auto_executed', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(format!("__heartbeat__:{workspace_id}"))
    .bind(outcome_content.to_string())
    .bind(&now)
    .execute(pool)
    .await
    {
        tracing::error!(%workspace_id, %proposal_id, "Failed to record proposal execution: {}", e);
    }

    outcome.map_err(|e| format!("执行失败: {}", e))
}

// ── POST /{id}/heartbeat/approvals/{proposal_id}/reject ──

pub async fn reject_proposal(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, proposal_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access_port!(state, claims, workspace_id);

    match update_proposal_status(&state, &workspace_id, &proposal_id, "rejected").await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"status": "rejected"})),
        Err(e) => ApiResponseBuilder::error(&e),
    }
}

async fn update_proposal_status(
    state: &AgentState,
    workspace_id: &str,
    proposal_id: &str,
    new_status: &str,
) -> Result<(), String> {
    // Push proposal_id filtering to SQL via json_extract instead of
    // fetching up to 100 rows and scanning in Rust.
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, content FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         AND json_extract(content, '$.proposalId') = ? \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(proposal_id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| format!("查询失败: {}", e))?;

    match row {
        Some((id, content)) => {
            let mut parsed: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e))?;
            parsed["status"] = serde_json::Value::String(new_status.to_string());
            let new_content = parsed.to_string();
            sqlx::query("UPDATE agent_actions SET content = ? WHERE id = ?")
                .bind(&new_content)
                .bind(&id)
                .execute(state.db.pool())
                .await
                .map_err(|e| format!("更新失败: {}", e))?;
            Ok(())
        }
        None => Err("提案不存在".to_string()),
    }
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
    use tinyiothub_agent::tools::{
        ExternalToolContext, ExternalToolHandler, ExternalToolMeta, ExternalToolRegistry,
    };

    #[derive(Clone)]
    struct RecordingHandler {
        calls: Arc<Mutex<Vec<serde_json::Value>>>,
        fail: bool,
    }

    #[async_trait]
    impl ExternalToolHandler for RecordingHandler {
        fn name(&self) -> &str {
            "write_properties"
        }
        fn description(&self) -> &str {
            "test"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(
            &self,
            _ctx: &ExternalToolContext,
            args: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            self.calls.lock().unwrap().push(args);
            if self.fail {
                return Err("device offline".into());
            }
            Ok(serde_json::json!({"applied": true}))
        }
    }

    struct TestRegistry {
        handler: Arc<dyn ExternalToolHandler>,
    }

    #[async_trait]
    impl ExternalToolRegistry for TestRegistry {
        async fn list_tools(&self) -> Vec<ExternalToolMeta> {
            vec![ExternalToolMeta {
                name: self.handler.name().to_string(),
                description: self.handler.description().to_string(),
                input_schema: self.handler.input_schema(),
            }]
        }
        async fn get_handler(&self, name: &str) -> Option<Arc<dyn ExternalToolHandler>> {
            (name == self.handler.name()).then(|| self.handler.clone())
        }
    }

    fn registry_with(handler: RecordingHandler) -> Arc<dyn ExternalToolRegistry> {
        Arc::new(TestRegistry {
            handler: Arc::new(handler),
        })
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        pool
    }

    async fn seed_proposal(pool: &SqlitePool, proposal_id: &str, status: &str) {
        let content = serde_json::json!({
            "proposalId": proposal_id,
            "status": status,
            "toolName": "write_properties",
            "deviceId": "dev_1",
            "summary": "set temp",
            "reason": "tune",
            "risk": "medium",
            "parameters": {"device_id": "dev_1", "properties": {"target_temp": 22}},
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
        let handler = RecordingHandler {
            calls: Arc::new(Mutex::new(vec![])),
            fail: false,
        };
        let calls = handler.calls.clone();
        let registry = registry_with(handler);

        approve_and_execute(&pool, "ws_1", "p1", &registry)
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
        assert_eq!(parsed["deviceId"], "dev_1");
        assert_eq!(parsed["success"], true);
    }

    #[tokio::test]
    async fn approve_twice_does_not_reexecute() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p1", "pending").await;
        let handler = RecordingHandler {
            calls: Arc::new(Mutex::new(vec![])),
            fail: false,
        };
        let calls = handler.calls.clone();
        let registry = registry_with(handler);

        approve_and_execute(&pool, "ws_1", "p1", &registry).await.unwrap();
        let second = approve_and_execute(&pool, "ws_1", "p1", &registry).await;
        assert!(second.is_err(), "second approve must be rejected");

        assert_eq!(calls.lock().unwrap().len(), 1, "tool must run exactly once");
    }

    #[tokio::test]
    async fn approve_records_failed_execution() {
        let pool = test_pool().await;
        seed_proposal(&pool, "p1", "pending").await;
        let handler = RecordingHandler {
            calls: Arc::new(Mutex::new(vec![])),
            fail: true,
        };
        let registry = registry_with(handler);

        let result = approve_and_execute(&pool, "ws_1", "p1", &registry).await;
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
        let handler = RecordingHandler {
            calls: Arc::new(Mutex::new(vec![])),
            fail: false,
        };
        let calls = handler.calls.clone();
        let registry = registry_with(handler);
        let result = approve_and_execute(&pool, "ws_1", "nope", &registry).await;
        assert!(result.is_err());
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn proposal_from_row_includes_parameters_for_blind_signing() {
        let content = serde_json::json!({
            "proposalId": "p1",
            "status": "pending",
            "level": "high",
            "toolName": "write_properties",
            "deviceId": "dev_1",
            "deviceName": "Thermostat",
            "summary": "set temp",
            "reason": "tune",
            "risk": "medium",
            "parameters": {"device_id": "dev_1", "properties": {"target_temp": 22}},
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
