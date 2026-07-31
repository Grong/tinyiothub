//! RunReport 落库抽象（T12）——cloud 注入的持久化能力
//! （HeartbeatTaskRepository 先例：trait 在 ai crate，Sqlite 实现在 cloud）。
//!
//! 供数关系：
//! - T10 prompt 装配：`recent_summaries`（记忆段 ≤5）/ `history_by_dedup_key` （历史段
//!   ≤3），条目格式统一为 `"[outcome] summary"`（[`format_summary`]）。
//! - X6 dedup（T18）：`last_problem_run` 取窗口内该 problem_key 最近一次 run 的 (outcome, verified,
//!   acked)。
//! - T4 `count_actions_last_hour` 依赖 `json_extract(report,'$.action_count')`： 实现方落库时必须在
//!   report JSON 里显式写入 `action_count` （`RunReport` 结构体无此字段，由 `actions.len()`
//!   派生）。

use super::types::{Outcome, RunReport};

#[async_trait::async_trait]
pub trait AgentRunsRepository: Send + Sync {
    /// 持久化一次 run 的完整报告。
    async fn insert_run(
        &self,
        report: &RunReport,
        problem_key: Option<&str>,
        dedup_key: Option<&str>,
    ) -> anyhow::Result<()>;
    /// 最近 N 条 run 摘要，新→旧（T10 记忆段）。
    async fn recent_summaries(&self, workspace_id: &str, limit: u32) -> anyhow::Result<Vec<String>>;
    /// 同 dedup_key 的历史摘要，新→旧（T10 历史段，条数由调用方截断）。
    async fn history_by_dedup_key(&self, workspace_id: &str, key: &str, limit: u32) -> anyhow::Result<Vec<String>>;
    /// 人工确认一次 run；首次确认返回 true，重复确认/不存在返回 false（幂等）。
    async fn ack_run(&self, run_id: &str, actor: &str) -> anyhow::Result<bool>;
    /// `since_hours` 窗口内该 problem_key 最近一次 run 的
    /// (outcome, verified, acked)（X6 dedup 用）。
    async fn last_problem_run(
        &self,
        workspace_id: &str,
        problem_key: &str,
        since_hours: u32,
    ) -> anyhow::Result<Option<(Outcome, bool, bool)>>;
}

/// 记忆/历史段统一条目格式：`"[acted] 调低设定值成功"`。
pub fn format_summary(outcome: &str, summary: &str) -> String {
    format!("[{outcome}] {summary}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_db_strings_round_trip() {
        for o in [
            Outcome::Acted,
            Outcome::NoActionNeeded,
            Outcome::Failed,
            Outcome::BudgetExceeded,
            Outcome::Rejected,
        ] {
            assert_eq!(Outcome::from_db(o.as_str()), Some(o));
        }
        assert_eq!(Outcome::from_db("success"), None);
        assert_eq!(Outcome::from_db(""), None);
    }

    #[test]
    fn summary_format_includes_outcome_prefix() {
        assert_eq!(format_summary("acted", "调低设定值成功"), "[acted] 调低设定值成功");
    }
}
