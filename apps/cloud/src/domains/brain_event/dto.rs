//! AI 大脑 P0 Task 3：/brain-events 只读端点 DTO（camelCase，judgment 域先例）。

use serde::{Deserialize, Serialize};

/// 列表行（无 evidence——性能评审裁决：证据只由 detail 端点载）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainEventDto {
    pub id: String,
    pub source: String,
    pub alarm_id: Option<String>,
    pub thing_id: Option<String>,
    pub title: String,
    pub verdict: Option<String>,
    pub reason: String,
    pub suggested_action: Option<String>,
    pub action_category: Option<String>,
    pub risk: Option<String>,
    pub run_id: Option<String>,
    pub ticket_id: Option<i64>,
    pub status: String,
    pub triage_mode: String,
    pub created_at: String,
    pub judged_at: Option<String>,
    pub state_entered_at: Option<String>,
    pub resolved_at: Option<String>,
}

impl From<&tinyiothub_storage::brain_event::BrainEvent> for BrainEventDto {
    fn from(e: &tinyiothub_storage::brain_event::BrainEvent) -> Self {
        Self {
            id: e.id.clone(),
            source: e.source.clone(),
            alarm_id: e.alarm_id.clone(),
            thing_id: e.thing_id.clone(),
            title: e.title.clone(),
            verdict: e.verdict.clone(),
            reason: e.reason.clone(),
            suggested_action: e.suggested_action.clone(),
            action_category: e.action_category.clone(),
            risk: e.risk.clone(),
            run_id: e.run_id.clone(),
            ticket_id: e.ticket_id,
            status: e.status.clone(),
            triage_mode: e.triage_mode.clone(),
            created_at: e.created_at.clone(),
            judged_at: e.judged_at.clone(),
            state_entered_at: e.state_entered_at.clone(),
            resolved_at: e.resolved_at.clone(),
        }
    }
}

/// 详情 = 列表行 + 完整证据（解析为 JSON，同 judgment evidence 先例）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainEventDetailDto {
    #[serde(flatten)]
    pub event: BrainEventDto,
    pub evidence: serde_json::Value,
}

/// feed 头部摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainEventsSummaryDto {
    pub digested_today: u64,
    pub needs_you: u64,
    pub latency_p50_secs: Option<f64>,
    pub feedback_right: u64,
    pub feedback_wrong: u64,
}

impl From<tinyiothub_storage::brain_event::BrainEventsSummary> for BrainEventsSummaryDto {
    fn from(s: tinyiothub_storage::brain_event::BrainEventsSummary) -> Self {
        Self {
            digested_today: s.digested_today,
            needs_you: s.needs_you,
            latency_p50_secs: s.latency_p50_secs,
            feedback_right: s.feedback_right,
            feedback_wrong: s.feedback_wrong,
        }
    }
}

/// 注意：feed 查询恒有 48h 时间窗（同 judgment feed F12 契约）。
#[derive(Debug, Deserialize)]
pub struct BrainEventQueryParams {
    /// needs_you | all | patrol | alarm | directive（默认 needs_you；未知值 400）
    pub tab: Option<String>,
    /// 游标（上一页最后一条 brain event id）
    pub before: Option<String>,
    pub limit: Option<i64>,
}
