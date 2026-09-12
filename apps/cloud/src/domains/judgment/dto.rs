//! T9：处置中心 feed API DTO（camelCase，ticket 域先例）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentDto {
    pub id: String,
    pub alarm_id: Option<String>,
    pub thing_id: Option<String>,
    pub ticket_id: Option<i64>,
    pub verdict: Option<String>,
    pub reason: String,
    pub evidence: serde_json::Value,
    pub suggested_action: Option<String>,
    pub action_category: Option<String>,
    pub status: String,
    pub latest_feedback: Option<JudgmentFeedbackDto>,
    pub created_at: String,
    pub judged_at: Option<String>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentFeedbackDto {
    pub verdict: String,
    pub reason: Option<String>,
    pub created_at: String,
}

impl From<tinyiothub_storage::judgment::JudgmentFeedback> for JudgmentFeedbackDto {
    fn from(f: tinyiothub_storage::judgment::JudgmentFeedback) -> Self {
        Self {
            verdict: f.verdict,
            reason: f.reason,
            created_at: f.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JudgmentQueryParams {
    /// needs_you | investigating | resolved | escalated | noise_archived | all（默认 needs_you）
    pub tab: Option<String>,
    /// 游标（上一页最后一条 judgment id）
    pub before: Option<String>,
    pub page_size: Option<i64>,
}

/// feed 头部摘要（v6 线框稿「今日 23 条已消化 · 2 条需要你」+ 学习计数）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentSummaryDto {
    pub needs_you: i64,
    pub investigating: i64,
    pub digested_today: i64,
    pub feedback_total: u64,
    pub feedback_right: u64,
    pub feedback_wrong: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackRequest {
    /// right | wrong
    pub verdict: String,
    /// verdict=wrong 时必填（≥4 字符，F13）
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectRequest {
    /// F14：拒绝必填原因（决定工单质量）
    pub reason: String,
}
