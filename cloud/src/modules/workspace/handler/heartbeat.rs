// Heartbeat handlers — per-workspace AI autonomous inspection endpoints
//
// Routes (registered under /workspaces/{id}/heartbeat):
//   GET  /config — read heartbeat config + tasks
//   PUT  /config — update enabled/intervalMinutes
//   GET  /logs  — query heartbeat execution history
//   GET  /tasks — read HEARTBEAT.md tasks
//   PUT  /tasks — write HEARTBEAT.md tasks

use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::agent::heartbeat::{
        HeartbeatTask, get_default_tasks, read_heartbeat_tasks, write_heartbeat_tasks,
    },
    shared::{api_response::ApiResponse, app_state::AppState, paths, security::jwt::Claims},
    verify_workspace_access,
};

// ── Response types ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfigResponse {
    enabled: bool,
    interval_minutes: u32,
    workspace_id: String,
    agent_id: String,
    tasks: Vec<HeartbeatTask>,
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
    pub tasks: Vec<HeartbeatTask>,
}

// ── GET /{id}/heartbeat/config ──

pub async fn get_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<HeartbeatConfigResponse>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);
    let tasks = read_heartbeat_tasks(&workspace_dir).await.unwrap_or_else(|e| {
        tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
        get_default_tasks()
    });

    let enabled = state.heartbeat_runner.as_ref()
        .map(|pm| pm.active_workspaces().contains(&workspace_id))
        .unwrap_or(false);
    ApiResponseBuilder::success(HeartbeatConfigResponse {
        enabled,
        interval_minutes: 15,
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

    // Apply config changes via heartbeat_runner stop/start
    if let Some(ref pm) = state.heartbeat_runner {
        let should_be_active = req.enabled.unwrap_or(true);
        let is_active = pm.active_workspaces().contains(&workspace_id);

        if should_be_active && !is_active {
            pm.start(&workspace_id).await;
        } else if !should_be_active && is_active {
            pm.stop(&workspace_id).await;
        }
    }

    ApiResponseBuilder::success(serde_json::json!({
        "enabled": req.enabled.unwrap_or(true),
        "intervalMinutes": req.interval_minutes.unwrap_or(15),
    }))
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
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);
    let tasks = read_heartbeat_tasks(&workspace_dir).await.unwrap_or_else(|e| {
        tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
        get_default_tasks()
    });

    ApiResponseBuilder::success(tasks)
}

// ── PUT /{id}/heartbeat/tasks ──

pub async fn update_tasks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateHeartbeatTasksRequest>,
) -> Json<ApiResponse<Vec<HeartbeatTask>>> {
    verify_workspace_access!(state, claims, workspace_id);

    let workspace_dir = paths::workspace_dir(&workspace_id);

    // Ensure workspace dir exists
    if !workspace_dir.exists()
        && let Err(e) = tokio::fs::create_dir_all(&workspace_dir).await
    {
        tracing::error!(%workspace_id, "Failed to create workspace dir: {}", e);
        return ApiResponseBuilder::error("创建工作空间目录失败");
    }

    if let Err(e) = write_heartbeat_tasks(&workspace_dir, &req.tasks).await {
        tracing::error!(%workspace_id, "Failed to write HEARTBEAT.md: {}", e);
        return ApiResponseBuilder::error("保存心跳任务失败");
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
            .filter_map(|(_, content, created_at)| {
                let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
                let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
                if status != "pending" {
                    return None;
                }
                Some(ProposalResponse {
                    proposal_id: parsed
                        .get("proposalId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: status.to_string(),
                    level: parsed.get("level").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    tool_name: parsed
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    device_id: parsed
                        .get("device_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    device_name: parsed
                        .get("device_name")
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
                    created_at,
                })
            })
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

    match update_proposal_status(&state, &workspace_id, &proposal_id, "approved").await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"status": "approved"})),
        Err(e) => ApiResponseBuilder::error(&e),
    }
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
