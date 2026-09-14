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

fn to_dto(
    j: &tinyiothub_storage::judgment::Judgment,
    fb: Option<tinyiothub_storage::judgment::JudgmentFeedback>,
) -> JudgmentDto {
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
    match state
        .db
        .list_judgments_feed(ws, statuses.as_deref(), params.before.as_deref(), page_size)
        .await
    {
        Ok(rows) => {
            // F-H：批量取最新反馈（替代逐行查询的 N+1）
            let ids: Vec<String> = rows.iter().map(|j| j.id.clone()).collect();
            let fbs = state.db.latest_feedbacks(&ids).await.unwrap_or_default();
            let out = rows.iter().map(|j| to_dto(j, fbs.get(&j.id).cloned())).collect();
            ApiResponseBuilder::success(out)
        }
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}

async fn judgment_summary(State(state): State<AppState>, claims: AuthClaims) -> Json<ApiResponse<JudgmentSummaryDto>> {
    let ws = &claims.0.workspace_id;
    let needs_you = count_status(
        &state,
        ws,
        &[
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Escalated,
        ],
    )
    .await;
    let investigating = count_status(
        &state,
        ws,
        &[tinyiothub_storage::judgment::JudgmentStatus::Investigating],
    )
    .await;
    let digested_today = state.db.count_judgments_today(ws).await.unwrap_or(0);
    let stats = state.db.judgment_stats(ws).await.ok();
    ApiResponseBuilder::success(JudgmentSummaryDto {
        needs_you,
        investigating,
        digested_today,
        feedback_total: stats.as_ref().map(|s| s.feedback_right + s.feedback_wrong).unwrap_or(0),
        feedback_right: stats.as_ref().map(|s| s.feedback_right).unwrap_or(0),
        feedback_wrong: stats.as_ref().map(|s| s.feedback_wrong).unwrap_or(0),
        latency_p50_secs: stats.as_ref().and_then(|s| s.latency_p50_secs),
        latency_p90_secs: stats.as_ref().and_then(|s| s.latency_p90_secs),
        feedback_by_category: stats
            .as_ref()
            .map(|s| {
                s.feedback_by_category
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            CategoryFeedbackDto {
                                right: v.right,
                                wrong: v.wrong,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

async fn count_status(state: &AppState, ws: &str, statuses: &[tinyiothub_storage::judgment::JudgmentStatus]) -> i64 {
    state.db.count_by_statuses(ws, statuses).await.unwrap_or(0)
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
        return ApiResponseBuilder::error_with_code(400, "点错必须填写原因（至少 4 个字符）");
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

    // 误判恢复闭环（F-C/T-17/C6）：噪声判断被点「错」→ 恢复被抑制的报警 +
    // 重开 judgment 为 investigating + 重派调查（「你判错了」的正确响应是
    // 「那我重查」，不是把报警丢回人工列表）。
    if req.verdict == "wrong" && judgment.status == tinyiothub_storage::judgment::JudgmentStatus::NoiseArchived {
        if let Some(alarm_id) = &judgment.alarm_id
            && let Err(e) = state.alarm_service.unsuppress_alarm(alarm_id, ws).await
        {
            tracing::warn!(judgment_id = %id, alarm_id, error = %e, "unsuppress on wrong-feedback failed (not suppressed?)");
        }
        match state.db.reopen_judgment(&id).await {
            Ok(true) => {
                redispatch_investigation(&state, &judgment).await;
            }
            Ok(false) => tracing::debug!(judgment_id = %id, "reopen skipped (not noise_archived)"),
            Err(e) => tracing::warn!(judgment_id = %id, error = %e, "reopen failed"),
        }
    }

    crate::domains::agent::host::judgment_subscriber::broadcast_judgment_pub(&state.sse_manager, ws, &id).await;
    ApiResponseBuilder::success(serde_json::json!({"ok": true}))
}

/// 重派调查（T-17/C6）：误判重开后用同一 problem_key 再次 dispatch。
/// 失败只记日志——清扫器的 investigating SLA 是兜底。
async fn redispatch_investigation(state: &AppState, judgment: &tinyiothub_storage::judgment::Judgment) {
    let Some(sink) = &state.directive_sink else {
        tracing::warn!(judgment_id = %judgment.id, "no directive sink — reopened judgment relies on SLA sweep");
        return;
    };
    let alarm = match (&judgment.alarm_id, &judgment.thing_id) {
        (Some(aid), Some(tid)) => match state.db.find_alarm_by_id(aid, Some(&judgment.workspace_id)).await {
            Ok(Some(a)) => Some((aid.clone(), tid.clone(), a)),
            _ => None,
        },
        _ => None,
    };
    let Some((alarm_id, thing_id, alarm)) = alarm else {
        tracing::warn!(judgment_id = %judgment.id, "reopened judgment lacks alarm/thing context — relies on SLA sweep");
        return;
    };
    let severity = alarm.alarm_level.as_str().to_string();
    let ai_alarm = tinyiothub_core::models::event::AlarmEvent {
        id: alarm_id,
        workspace_id: judgment.workspace_id.clone(),
        thing_id: thing_id.clone(),
        alarm_type: format!("{}", alarm.alarm_type),
        severity,
        message: alarm.message.clone(),
        rule_id: alarm.rule_id.clone(),
        resolved: false,
        created_at: alarm.alarm_time,
    };
    let rule_part = alarm.rule_id.as_deref().unwrap_or("-");
    let signal = tinyiothub_agent::runtime::thing_agent::types::WakeSignal {
        workspace_id: judgment.workspace_id.clone(),
        priority: tinyiothub_agent::runtime::thing_agent::types::Priority::High,
        source: tinyiothub_agent::runtime::thing_agent::types::TriggerSource::UserDirective {
            user_id: "alarm-triage".to_string(),
            text: tinyiothub_agent::runtime::orchestrator::callbacks::alarm_investigation_text(&ai_alarm),
            session_key: None,
            source: Some("alarm".to_string()),
            problem_key: Some(format!("alarm:{}:{}", thing_id, rule_part)),
        },
        dedup_key: None,
    };
    if let Err(e) = sink.enqueue(signal) {
        tracing::warn!(judgment_id = %judgment.id, error = %e, "re-investigation dispatch failed — SLA sweep backstops");
    }
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

    // 派发执行 directive（执行结果由 judgment_subscriber 的 exec: 键路径收尾）。
    // 4A/T-3：动作文本由服务端模板按 action_category 生成——LLM 的
    // suggested_action 只做展示，不进执行指令（注入面收敛）。
    // 6A/T-4：exec prompt 携带调查上下文（判断理由 + 证据摘录）。
    let action = tinyiothub_storage::judgment::exec_action_template(
        judgment.action_category.as_deref(),
        judgment.thing_id.as_deref(),
    );
    let evidence_excerpt = serde_json::from_str::<serde_json::Value>(&judgment.evidence_json)
        .ok()
        .and_then(|v| v.get("excerpt").and_then(|e| e.as_str()).map(str::to_string))
        .map(|e| e.chars().take(300).collect::<String>())
        .unwrap_or_default();
    let signal = tinyiothub_agent::runtime::thing_agent::types::WakeSignal {
        workspace_id: judgment.workspace_id.clone(),
        priority: tinyiothub_agent::runtime::thing_agent::types::Priority::High,
        source: tinyiothub_agent::runtime::thing_agent::types::TriggerSource::UserDirective {
            user_id: claims.0.user_id.clone(),
            text: format!(
                "执行已批准的处置动作：{action}。\n调查结论：{}\n证据摘录：{}\n完成后简述结果。",
                judgment.reason, evidence_excerpt
            ),
            session_key: None,
            source: Some("judgment-approval".to_string()),
            problem_key: Some(format!("exec:{id}")),
        },
        // C5/T-14：judgment_id 作 dedup_key——回滚后重批/网络重试不会执行两次
        dedup_key: Some(format!("exec:{id}")),
    };
    if let Err(e) = sink.enqueue(signal) {
        tracing::error!(judgment_id = %id, error = %e, "execution directive not admitted");
        // D3：补偿回滚（executing → awaiting_approval），用户可重试；
        // 回滚失败由 executing SLA 清扫兜底（转人工确认工单）。
        if let Err(re) = state
            .db
            .transit_judgment(
                &id,
                tinyiothub_storage::judgment::JudgmentStatus::Executing,
                tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                None,
            )
            .await
        {
            tracing::error!(judgment_id = %id, error = %re, "rollback to awaiting_approval failed — SLA sweep will backstop");
        }
        return ApiResponseBuilder::error("执行派发失败（队列满），已回滚待审批，可重试");
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
        return ApiResponseBuilder::error_with_code(400, "拒绝必须填写原因（至少 4 个字符）");
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
