//! T9：处置中心 feed API handler。
//!
//! ```text
//! GET    /judgments              tab 筛选 + 游标分页（默认 needs_you）
//! GET    /judgments/summary      头部摘要（消化数/需要你/学习计数）
//! POST   /judgments/{id}/feedback  ✓/✕ 反馈（wrong 必填原因≥4字符；写记忆）
//! POST   /judgments/{id}/approve   待审批 → 执行中 + 派发执行 directive
//! POST   /judgments/{id}/reject    待审批 → 升级工单（必填原因，F14）
//! ```

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};

use tinyiothub_web::middleware::workspace::AuthClaims;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::domains::judgment::dto::*;
use crate::state::AppState;

pub fn create_judgment_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_judgments))
        .route("/summary", get(judgment_summary))
        .route("/{id}/feedback", post(submit_feedback))
        .route("/{id}/approve", post(approve_judgment))
        .route("/{id}/reject", post(reject_judgment))
}

fn to_dto(j: &tinyiothub_storage::judgment::Judgment, fb: Option<tinyiothub_storage::judgment::JudgmentFeedback>) -> JudgmentDto {
    JudgmentDto {
        id: j.id.clone(),
        alarm_id: j.alarm_id.clone(),
        thing_id: j.thing_id.clone(),
        ticket_id: j.ticket_id,
        verdict: j.verdict.map(|v| v.as_str().to_string()),
        reason: j.reason.clone(),
        evidence: serde_json::from_str(&j.evidence_json).unwrap_or(serde_json::Value::Null),
        suggested_action: j.suggested_action.clone(),
        action_category: j.action_category.clone(),
        status: j.status.as_str().to_string(),
        latest_feedback: fb.map(JudgmentFeedbackDto::from),
        created_at: j.created_at.to_rfc3339(),
        judged_at: j.judged_at.map(|t| t.to_rfc3339()),
        resolved_at: j.resolved_at.map(|t| t.to_rfc3339()),
    }
}

async fn list_judgments(
    Query(params): Query<JudgmentQueryParams>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<Vec<JudgmentDto>>> {
    use tinyiothub_storage::judgment::JudgmentStatus as S;
    let ws = &claims.0.workspace_id;
    let statuses: Option<Vec<S>> = match params.tab.as_deref().unwrap_or("needs_you") {
        "needs_you" => Some(vec![S::AwaitingApproval, S::Escalated]),
        "investigating" => Some(vec![S::Investigating]),
        "resolved" => Some(vec![S::Resolved]),
        "noise" => Some(vec![S::NoiseArchived]),
        "all" => None,
        _ => None,
    };
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
    match state.db.list_judgments(ws, statuses.as_deref(), params.before.as_deref(), page_size).await {
        Ok(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for j in &rows {
                let fb = state.db.latest_judgment_feedback(&j.id).await.ok().flatten();
                out.push(to_dto(j, fb));
            }
            ApiResponseBuilder::success(out)
        }
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}

async fn judgment_summary(State(state): State<AppState>, claims: AuthClaims) -> Json<ApiResponse<JudgmentSummaryDto>> {
    let ws = &claims.0.workspace_id;
    let needs_you = count_status(&state, ws, &[tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval, tinyiothub_storage::judgment::JudgmentStatus::Escalated]).await;
    let investigating = count_status(&state, ws, &[tinyiothub_storage::judgment::JudgmentStatus::Investigating]).await;
    let digested_today = state.db.count_judgments_today(ws).await.unwrap_or(0);
    let stats = state.db.judgment_stats(ws).await.ok();
    ApiResponseBuilder::success(JudgmentSummaryDto {
        needs_you,
        investigating,
        digested_today,
        feedback_total: stats.as_ref().map(|s| s.feedback_right + s.feedback_wrong).unwrap_or(0),
        feedback_right: stats.as_ref().map(|s| s.feedback_right).unwrap_or(0),
        feedback_wrong: stats.as_ref().map(|s| s.feedback_wrong).unwrap_or(0),
    })
}

async fn count_status(state: &AppState, ws: &str, statuses: &[tinyiothub_storage::judgment::JudgmentStatus]) -> i64 {
    state
        .db
        .list_judgments(ws, Some(statuses), None, 1000)
        .await
        .map(|v| v.len() as i64)
        .unwrap_or(0)
}

/// ✓/✕ 反馈（D11 独立表全历史；「错」写 agent_memories 知识层）。
async fn submit_feedback(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<FeedbackRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = &claims.0.workspace_id;
    if req.verdict != "right" && req.verdict != "wrong" {
        return ApiResponseBuilder::error_with_code(400, "verdict 必须是 right 或 wrong");
    }
    let reason = req.reason.as_deref().map(str::trim).filter(|r| !r.is_empty());
    if req.verdict == "wrong" && reason.map(|r| r.chars().count()).unwrap_or(0) < 4 {
        return ApiResponseBuilder::error_with_code(
            400,
            "点错必须填写原因（至少 4 个字符）",
        );
    }
    let judgment = match state.db.find_judgment_by_id(&id, ws).await {
        Ok(Some(j)) => j,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "judgment 不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("查询失败: {e}")),
    };

    match state
        .db
        .add_judgment_feedback(&id, ws, &claims.0.user_id, &req.verdict, reason)
        .await
    {
        Ok(_) => {}
        Err(e) => return ApiResponseBuilder::error(format!("反馈写入失败: {e}")),
    }

    // 知识闭环：「错」的原因写 agent_memories（ticket-resolution 先例），
    // 下次调查同类问题时注入。
    if req.verdict == "wrong"
        && let Err(e) = write_feedback_memory(&state, &judgment, reason.unwrap_or_default()).await
    {
        tracing::warn!(judgment_id = %id, error = %e, "feedback memory write failed (feedback persisted)");
    }

    crate::domains::agent::host::judgment_subscriber::broadcast_judgment_pub(&state.sse_manager, ws, &id).await;
    ApiResponseBuilder::success(serde_json::json!({"ok": true}))
}

/// 「错」反馈写知识层（tags 便于后续按 judgment 追溯退役）。
async fn write_feedback_memory(
    state: &AppState,
    judgment: &tinyiothub_storage::judgment::Judgment,
    reason: &str,
) -> Result<(), String> {
    let input = tinyiothub_core::memory::MemoryInput {
        workspace_id: judgment.workspace_id.clone(),
        agent_id: "default".to_string(),
        zone: tinyiothub_core::memory::MemoryZone::Work,
        content: format!(
            "AI 判断被纠正（{}）：{}\n用户纠正：{}",
            judgment.alarm_id.as_deref().unwrap_or("-"),
            judgment.reason,
            reason
        ),
        source: tinyiothub_core::memory::MemorySource::User,
        confidence: tinyiothub_core::memory::Confidence::High,
        tags: vec!["judgment-feedback".to_string(), format!("judgment:{}", judgment.id)],
        supersedes: None,
        thing_id: judgment.thing_id.clone(),
        snapshot_data: None,
        snapshot_time: None,
    };
    let store = tinyiothub_storage::memory::MemoryStore::new(state.db.pool().clone());
    store.put(input).await.map(|_| ()).map_err(|e| e.to_string())
}

/// 批准：awaiting_approval → executing + 派发执行 directive（problem_key=exec:{id}）。
async fn approve_judgment(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = &claims.0.workspace_id;
    let judgment = match state.db.find_judgment_by_id(&id, ws).await {
        Ok(Some(j)) => j,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "judgment 不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("查询失败: {e}")),
    };

    // 先验执行通道再迁移状态——sink 缺失时保持 awaiting_approval 不卡 executing
    let Some(sink) = &state.directive_sink else {
        return ApiResponseBuilder::error("执行通道未就绪");
    };

    // 条件迁移防并发互撞
    match state
        .db
        .transit_judgment(
            &id,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Executing,
            None,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return ApiResponseBuilder::error_with_code(409, "该判断已不在待审批状态");
        }
        Err(e) => return ApiResponseBuilder::error(format!("状态迁移失败: {e}")),
    }

    // 派发执行 directive（执行结果由 judgment_subscriber 的 exec: 键路径收尾）
    let action = judgment.suggested_action.clone().unwrap_or_else(|| "按判断建议处置".to_string());
    let signal = tinyiothub_agent::runtime::thing_agent::types::WakeSignal {
        workspace_id: judgment.workspace_id.clone(),
        priority: tinyiothub_agent::runtime::thing_agent::types::Priority::High,
        source: tinyiothub_agent::runtime::thing_agent::types::TriggerSource::UserDirective {
            user_id: claims.0.user_id.clone(),
            text: format!("执行已批准的处置动作：{action}。完成后简述结果。"),
            session_key: None,
            source: Some("judgment-approval".to_string()),
            problem_key: Some(format!("exec:{id}")),
        },
        dedup_key: None,
    };
    if let Err(e) = sink.enqueue(signal) {
        tracing::error!(judgment_id = %id, error = %e, "execution directive not admitted");
        return ApiResponseBuilder::error("执行派发失败（队列满）");
    }

    crate::domains::agent::host::judgment_subscriber::broadcast_judgment_pub(&state.sse_manager, ws, &id).await;
    ApiResponseBuilder::success(serde_json::json!({"ok": true, "status": "executing"}))
}

/// 拒绝：awaiting_approval → escalated + 工单（F14：必填原因，不写 feedback）。
async fn reject_judgment(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<RejectRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = &claims.0.workspace_id;
    let reason = req.reason.trim();
    if reason.chars().count() < 4 {
        return ApiResponseBuilder::error_with_code(
            400,
            "拒绝必须填写原因（至少 4 个字符）",
        );
    }
    let judgment = match state.db.find_judgment_by_id(&id, ws).await {
        Ok(Some(j)) => j,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "judgment 不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("查询失败: {e}")),
    };

    match state
        .db
        .transit_judgment(
            &id,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Escalated,
            None,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return ApiResponseBuilder::error_with_code(409, "该判断已不在待审批状态");
        }
        Err(e) => return ApiResponseBuilder::error(format!("状态迁移失败: {e}")),
    }

    let ticket_id = crate::domains::ticket::create_escalation(
        &state.db,
        &state.sse_manager,
        crate::domains::ticket::Escalation {
            workspace_id: judgment.workspace_id.clone(),
            thing_id: judgment.thing_id.clone(),
            agent_run_id: judgment.run_id.clone().unwrap_or_else(|| format!("reject:{id}")),
            title: format!("审批被拒绝：{}", judgment.reason.chars().take(60).collect::<String>()),
            briefing: serde_json::json!({
                "problem": judgment.reason,
                "source": "approval_rejected",
                "judgment_id": id,
                "reject_reason": reason,
                "suggested_action": judgment.suggested_action,
            }),
            failure_hash: format!("pk:approval-rejected:{}", judgment.alarm_id.as_deref().unwrap_or(&id)),
        },
    )
    .await;
    if let Some(tid) = ticket_id {
        let _ = state.db.link_judgment_ticket(&id, tid).await;
    }

    crate::domains::agent::host::judgment_subscriber::broadcast_judgment_pub(&state.sse_manager, ws, &id).await;
    ApiResponseBuilder::success(serde_json::json!({"ok": true, "status": "escalated", "ticketId": ticket_id}))
}
