// Heartbeat handlers — per-workspace AI autonomous inspection endpoints
//
// Routes (registered under /workspaces/{id}/heartbeat):
//   GET  /config — read heartbeat config + tasks
//   PUT  /config — update enabled/intervalMinutes
//   GET  /logs  — query heartbeat execution history
//   GET  /tasks — read heartbeat tasks (DB)
//   PUT  /tasks — replace heartbeat tasks (DB)

use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde::{Deserialize, Serialize};
use tinyiothub_ai::heartbeat::types::NewHeartbeatTask;
use tinyiothub_auth::security::jwt::Claims;
use tinyiothub_core::agent_hooks::HeartbeatTaskDef;
use tinyiothub_tenant::verify_workspace_access;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::shared::{api_response::ApiResponse, app_state::AppState, paths};

/// Heartbeat routes (`/{id}/heartbeat/*`), nested at `/workspaces` by the
/// composition layer next to `tinyiothub_tenant::workspace_router()` —
/// route-equivalent to the former in-module registration.
pub fn create_router() -> axum::Router<AppState> {
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
        .route("/{id}/heartbeat/approvals/{proposal_id}/approve", post(approve_proposal))
        .route("/{id}/heartbeat/approvals/{proposal_id}/reject", post(reject_proposal))
}

// ── Response types ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfigResponse {
    enabled: bool,
    interval_minutes: u32,
    workspace_id: String,
    agent_id: String,
    tasks: Vec<HeartbeatTaskDef>,
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    let tasks = load_tasks(&state, &workspace_id).await;

    let enabled = state
        .heartbeat_runner
        .as_ref()
        .map(|pm| pm.active_workspaces().contains(&workspace_id))
        .unwrap_or(false);

    let mut interval_minutes = 15;
    if let Some(ref runner) = state.heartbeat_runner {
        interval_minutes = runner.effective_interval_minutes(&workspace_id).await;
    }

    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled,
        interval_minutes,
        workspace_id: workspace_id.clone(),
        agent_id: "default".to_string(),
        tasks,
    })
}

// ── PUT /{id}/heartbeat/config ──

pub async fn update_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatConfigRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    let Some(ref runner) = state.heartbeat_runner else {
        return ApiResponseBuilder::error("心跳服务未启用");
    };

    // Merge with persisted/current values so a partial update doesn't reset
    // the other field.
    let current_interval = runner.effective_interval_minutes(&workspace_id).await;
    let is_active = runner.active_workspaces().contains(&workspace_id);
    let enabled = req.enabled.unwrap_or(is_active);
    let interval = req.interval_minutes.unwrap_or(current_interval);

    let config = match tinyiothub_ai::heartbeat::types::WorkspaceHeartbeatConfig::validated(
        enabled, interval,
    ) {
        Ok(c) => c,
        Err(e) => return ApiResponseBuilder::error(&e),
    };
    if let Err(e) = runner.task_repo().save_heartbeat_config(&workspace_id, &config).await {
        tracing::error!(%workspace_id, "Failed to persist heartbeat config: {}", e);
        return ApiResponseBuilder::error("保存心跳配置失败");
    }

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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    let config = match state.heartbeat_runner {
        Some(ref runner) => match runner.get_trust_config(&workspace_id) {
            Some(c) => c,
            None => runner
                .task_repo()
                .load_trust_config(&workspace_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
        },
        None => tinyiothub_skills::trust::TrustConfig::default(),
    };

    ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default())
}

// ── PUT /{id}/heartbeat/trust ──

pub async fn update_trust_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(config): Json<tinyiothub_skills::trust::TrustConfig>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    let Some(ref runner) = state.heartbeat_runner else {
        return ApiResponseBuilder::error("心跳服务未启用");
    };

    // Persists to DB and hot-updates pool + cache + running loop.
    runner.update_trust_config(&workspace_id, config.clone()).await;

    ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default())
}

// ── GET /{id}/heartbeat/logs ──

pub async fn get_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatLogsResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    // Fetch all heartbeat rows (summary + error + auto_executed + proposal)
    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND event_type = 'heartbeat' \
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(&workspace_id)
    .fetch_all(state.database.pool())
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
                        details.entry(created_at.clone()).or_default().push((action_type, content));
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
                                if let Ok(parsed) =
                                    serde_json::from_str::<serde_json::Value>(&a_content)
                                {
                                    auto_executed.push(ActionDetail {
                                        tool: parsed
                                            .get("tool")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
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
                                if let Ok(parsed) =
                                    serde_json::from_str::<serde_json::Value>(&a_content)
                                {
                                    pending_proposals.push(ProposalDetail {
                                        level: parsed
                                            .get("level")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
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
                                        reason: parsed
                                            .get("reason")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        risk: parsed
                                            .get("risk")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        status: parsed
                                            .get("status")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<Vec<HeartbeatTaskDef>>> {
    verify_workspace_access!(state, claims, workspace_id);

    let tasks = load_tasks(&state, &workspace_id).await;

    ApiResponseBuilder::success(tasks)
}

// ── PUT /{id}/heartbeat/tasks ──

pub async fn update_tasks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatTasksRequest>,
) -> Json<ApiResponse<Vec<HeartbeatTaskDef>>> {
    verify_workspace_access!(state, claims, workspace_id);

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

    if let Err(e) = runner.task_repo().replace_all(&workspace_id, &new_tasks).await {
        tracing::error!(%workspace_id, "Failed to save heartbeat tasks: {}", e);
        return ApiResponseBuilder::error("保存心跳任务失败");
    }

    runner.notify_tasks_changed(&workspace_id);
    if !new_tasks.is_empty() && !runner.active_workspaces().contains(&workspace_id) {
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<ApprovalsResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT action_type, content, created_at FROM agent_actions \
         WHERE workspace_id = ? AND action_type = 'proposal' \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&workspace_id)
    .fetch_all(state.database.pool())
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, proposal_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    let Some(registry) = crate::modules::mcp::get_mcp_registry() else {
        return ApiResponseBuilder::error("工具注册表未初始化");
    };
    let registry = registry.read().await;
    match approve_and_execute(state.database.pool(), &workspace_id, &proposal_id, &registry).await {
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
    registry: &crate::modules::mcp::tool_registry::HandlerRegistry,
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
    let parsed: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e))?;
    if parsed["status"].as_str() != Some("pending") {
        return Err("提案已处理".to_string());
    }
    let tool_name = parsed["toolName"].as_str().unwrap_or("").to_string();
    let device_id = parsed["deviceId"].as_str().map(str::to_string);
    let params = parsed.get("parameters").cloned().unwrap_or(serde_json::json!({}));

    let handler =
        registry.get_owned(&tool_name).ok_or_else(|| format!("工具未注册: {}", tool_name))?;

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

    // Execute under the same MCP auth context as the heartbeat agent path —
    // handlers scope their queries by get_mcp_context() and fail closed
    // without it.
    let _guard = crate::modules::mcp::handlers::McpContextGuard::new(
        crate::modules::mcp::handlers::McpAuthContext::for_heartbeat(
            workspace_id.to_string(),
            format!("__heartbeat__:{workspace_id}"),
        ),
    );
    let outcome = handler.execute(params).await;
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, proposal_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, workspace_id);

    match update_proposal_status(&state, &workspace_id, &proposal_id, "rejected").await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"status": "rejected"})),
        Err(e) => ApiResponseBuilder::error(&e),
    }
}

async fn update_proposal_status(
    state: &AppState,
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
    .fetch_optional(state.database.pool())
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
                .execute(state.database.pool())
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
async fn load_tasks(state: &AppState, workspace_id: &str) -> Vec<HeartbeatTaskDef> {
    if let Some(ref runner) = state.heartbeat_runner {
        let workspace_dir = paths::workspace_dir(workspace_id);
        if let Err(e) =
            state.agent_hooks.migrate_legacy_heartbeat_tasks(workspace_id, &workspace_dir).await
        {
            tracing::warn!(%workspace_id, "Heartbeat task migration failed: {}", e);
        }
        match runner.task_repo().list_by_workspace(workspace_id).await {
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
    state.agent_hooks.read_legacy_heartbeat_tasks(&workspace_dir).await.unwrap_or_else(|e| {
        tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
        state.agent_hooks.default_heartbeat_tasks()
    })
}

fn parse_action_content(content: &str) -> (u32, Option<String>) {
    // New format: {"taskCount": N, "result": "..."} or {"taskCount": N, "error": "..."}
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
        let task_count =
            parsed.get("taskCount").and_then(|v| v.as_u64()).map(|n| n as u32).unwrap_or(0);
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
    use crate::modules::mcp::tool_registry::{
        HandlerRegistry, InputSchema, ToolError, ToolHandler,
    };

    #[derive(Clone)]
    struct RecordingHandler {
        calls: Arc<Mutex<Vec<serde_json::Value>>>,
        fail: bool,
    }

    #[async_trait]
    impl ToolHandler for RecordingHandler {
        fn name(&self) -> &str {
            "write_properties"
        }
        fn description(&self) -> &str {
            "test"
        }
        fn input_schema(&self) -> InputSchema {
            InputSchema {
                schema_type: "object".into(),
                required: vec![],
                properties: Default::default(),
            }
        }
        async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
            self.calls.lock().unwrap().push(args);
            if self.fail {
                return Err(ToolError::Internal("device offline".into()));
            }
            Ok(serde_json::json!({"applied": true}))
        }
    }

    fn registry_with(handler: RecordingHandler) -> HandlerRegistry {
        let mut reg = HandlerRegistry::new(None);
        reg.register(handler);
        reg
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool).await.unwrap();
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
        let handler = RecordingHandler { calls: Arc::new(Mutex::new(vec![])), fail: false };
        let calls = handler.calls.clone();
        let registry = registry_with(handler);

        approve_and_execute(&pool, "ws_1", "p1", &registry).await.expect("approve");

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
        let handler = RecordingHandler { calls: Arc::new(Mutex::new(vec![])), fail: false };
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
        let handler = RecordingHandler { calls: Arc::new(Mutex::new(vec![])), fail: true };
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
        let handler = RecordingHandler { calls: Arc::new(Mutex::new(vec![])), fail: false };
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

        let p = proposal_from_row(&content, "2026-07-20 10:00:00".to_string())
            .expect("pending proposal maps");

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
