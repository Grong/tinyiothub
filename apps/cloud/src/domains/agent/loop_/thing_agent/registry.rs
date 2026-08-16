//! RunRegistry — thing_agent run 记录的内存真相源（Task 4）。
//!
//! dedup 窗口语义（Step 0 前置验证结论）：dedup/pushback 判定只关心有界近期
//! 窗口，证据如下——
//! - `policy_relax_hint`（pushback.rs）只查询同 dedup_key **最近 3 条** run
//!   （`POLICY_DENIAL_LOOKBACK = POLICY_DENIAL_STREAK = 3`），判定"连续 3 次
//!   策略拒绝"；窗口之外的历史不参与判定。
//! - T10 prompt 注入（manager.rs `run_pipeline`）只取**最近 5 条** summary
//!   与同 dedup_key **最近 3 条**历史。
//!
//! 因此每 workspace 保留最近 [`COMPLETED_CAPACITY`] 条已完成 run 即可覆盖全部
//! 运行时读取路径；无需全量历史。全量历史由 Task 8 的持久化订阅者（消费
//! `AgentEventKind::RunRecorded`）落在 `agent_runs` 表，供 cloud handler 查询。
//!
//! dedup 匹配规则：`dedup_key` 不在 core 的 [`RunReport`] 上（值类型不含此
//! 字段），匹配基于 `trigger` 标签——`manager.rs::trigger_label` 是唯一生产
//! 点，保证 ThingEvent/Timer 的 `trigger == dedup_key`，Merged 为
//! `merged:{dedup_key}`；两种形式均视为命中（见 [`dedup_matches`]）。
//! Critical 信号绕过合并窗口（scheduler ②），X5 关心的 Critical 拒绝 run
//! 的 trigger 必等于 dedup_key，窗口近似与原 SQL `dedup_key = ?` 等价。
//!
//! 容量：`record` 在 run 完成时调用，窗口即"最近 50 条已完成"；in-flight
//! run 的实时标记（无 begin API——RunReport 只在完成时存在）不在本 Task，
//! [`RunRegistry::active`] 返回当前进程持有的全部窗口内容（D13 实时读）。

use std::collections::VecDeque;
use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::agent_runs::RunReport;

/// 每 workspace 保留的最近已完成 run 条数（超出驱逐最老）。
pub const COMPLETED_CAPACITY: usize = 50;

/// 跨 workspace 的内存 run 记录。Clone 廉价（内部 Arc），manager 依赖、
/// 测试探针与 AgentRuntime 门面共享同一实例。
#[derive(Clone, Default)]
pub struct RunRegistry {
    /// workspace_id → run 队列（旧→新，队尾为最新）。
    inner: Arc<DashMap<String, VecDeque<RunReport>>>,
}

impl RunRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条已完成 run；该 workspace 超出容量时驱逐最老。
    pub fn record(&self, report: RunReport) {
        let mut entry = self.inner.entry(report.workspace_id.clone()).or_default();
        entry.push_back(report);
        while entry.len() > COMPLETED_CAPACITY {
            entry.pop_front();
        }
    }

    /// 最近 run（新→旧），最多 `limit` 条。等价原 `recent_summaries` 的
    /// 排序语义（`ORDER BY created_at DESC, rowid DESC` —— 插入序倒序）。
    pub fn recent(&self, workspace_id: &str, limit: usize) -> Vec<RunReport> {
        self.inner
            .get(workspace_id)
            .map(|q| q.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    /// 同 dedup_key 的最近 run（新→旧），最多 `limit` 条——pushback X5
    /// 连续拒绝判定与 T10 历史段的查询（替代原 `recent_runs_by_dedup_key` /
    /// `history_by_dedup_key`）。窗口内匹配，见模块文档的 dedup 匹配规则。
    pub fn recent_by_dedup(&self, workspace_id: &str, key: &str, limit: usize) -> Vec<RunReport> {
        self.inner
            .get(workspace_id)
            .map(|q| {
                q.iter()
                    .rev()
                    .filter(|r| dedup_matches(&r.trigger, key))
                    .take(limit)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 窗口内同 dedup_key 的 run 计数（跨 workspace——与原测试探针
    /// `SELECT COUNT(*) ... WHERE dedup_key = ?` 的全局语义对齐；dedup key
    /// 内嵌 thing/event 标识，实践中不跨 workspace 碰撞）。
    pub fn count_by_dedup(&self, key: &str) -> usize {
        self.inner
            .iter()
            .map(|e| e.value().iter().filter(|r| dedup_matches(&r.trigger, key)).count())
            .sum()
    }

    /// restore 预热：输入按时间旧→新，逐条 record 复用容量驱逐。
    pub fn prewarm(&self, reports: Vec<RunReport>) {
        for report in reports {
            self.record(report);
        }
    }

    /// D13 实时读：当前窗口内全部 run（每 workspace 内新→旧）。
    /// RunReport 无时间戳，跨 workspace 次序不稳定，调用方不得依赖全局顺序。
    pub fn active(&self) -> Vec<RunReport> {
        let mut out = Vec::new();
        for entry in self.inner.iter() {
            out.extend(entry.value().iter().rev().cloned());
        }
        out
    }
}

/// trigger 标签是否命中 dedup key：直接相等（ThingEvent/Timer/UserDirective
/// 不参与合并），或 Merged 形式 `merged:{key}`。
fn dedup_matches(trigger: &str, key: &str) -> bool {
    trigger == key || trigger.strip_prefix("merged:") == Some(key)
}

#[cfg(test)]
mod fixtures {
    use tinyiothub_core::agent_runs::{Outcome, RunReport};

    pub fn report(ws: &str, run_id: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: ws.to_string(),
            trigger: format!("trigger:{run_id}"),
            outcome: Outcome::Acted,
            summary: "s".to_string(),
            actions: vec![],
            verified: true,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
        }
    }

    /// trigger 标签即 dedup key（ThingEvent 信号的 trigger_label 语义）。
    pub fn report_with_dedup(ws: &str, key: &str) -> RunReport {
        RunReport {
            trigger: key.to_string(),
            ..report(ws, &format!("run_{key}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_evicts_oldest_completed_beyond_50() {
        let reg = RunRegistry::new();
        for i in 0..55 {
            reg.record(fixtures::report("ws1", &format!("run{i}")));
        }
        assert_eq!(reg.recent("ws1", 100).len(), 50);
        assert_eq!(reg.recent("ws1", 1)[0].run_id, "run54");
        // 最老的 5 条已被驱逐。
        assert_eq!(reg.recent("ws1", 50).last().unwrap().run_id, "run5");
    }

    #[test]
    fn count_by_dedup_counts_within_window() {
        let reg = RunRegistry::new();
        reg.record(fixtures::report_with_dedup("ws1", "k1"));
        reg.record(fixtures::report_with_dedup("ws1", "k1"));
        assert_eq!(reg.count_by_dedup("k1"), 2);
    }

    #[test]
    fn count_by_dedup_excludes_evicted_and_unrelated() {
        let reg = RunRegistry::new();
        for _ in 0..55 {
            reg.record(fixtures::report_with_dedup("ws1", "k1"));
        }
        // 驱逐后窗口内只剩 50 条。
        assert_eq!(reg.count_by_dedup("k1"), 50);
        assert_eq!(reg.count_by_dedup("unknown"), 0);
    }

    #[test]
    fn dedup_matches_merged_trigger_label() {
        let reg = RunRegistry::new();
        let merged = RunReport {
            trigger: "merged:k1".to_string(),
            ..fixtures::report("ws1", "run_m")
        };
        reg.record(merged);
        assert_eq!(reg.count_by_dedup("k1"), 1);
        assert_eq!(reg.recent_by_dedup("ws1", "k1", 3)[0].run_id, "run_m");
        // 前缀碰撞不误命中：merged:k1 不等于 k10，反之亦然。
        assert_eq!(reg.count_by_dedup("merged:k1"), 1);
        assert_eq!(reg.count_by_dedup("k10"), 0);
    }

    #[test]
    fn recent_by_dedup_returns_newest_first_capped() {
        let reg = RunRegistry::new();
        for i in 0..5 {
            let mut r = fixtures::report_with_dedup("ws1", "k1");
            r.run_id = format!("run{i}");
            reg.record(r);
        }
        reg.record(fixtures::report_with_dedup("ws1", "k2"));
        let hits = reg.recent_by_dedup("ws1", "k1", 3);
        let ids: Vec<&str> = hits.iter().map(|r| r.run_id.as_str()).collect();
        assert_eq!(ids, ["run4", "run3", "run2"]);
    }

    #[test]
    fn recent_scoped_per_workspace() {
        let reg = RunRegistry::new();
        reg.record(fixtures::report("ws1", "a"));
        reg.record(fixtures::report("ws2", "b"));
        assert_eq!(reg.recent("ws1", 10).len(), 1);
        assert_eq!(reg.recent("ws_unknown", 10).len(), 0);
        assert_eq!(reg.recent_by_dedup("ws2", "trigger:a", 10).len(), 0);
    }

    #[test]
    fn prewarm_restores_window_with_eviction() {
        let reg = RunRegistry::new();
        // 旧→新输入；超过容量同样驱逐最老。
        reg.prewarm((0..52).map(|i| fixtures::report("ws1", &format!("run{i}"))).collect());
        assert_eq!(reg.recent("ws1", 100).len(), 50);
        assert_eq!(reg.recent("ws1", 1)[0].run_id, "run51");
    }

    #[test]
    fn active_returns_window_contents_newest_first_per_workspace() {
        let reg = RunRegistry::new();
        reg.record(fixtures::report("ws1", "a1"));
        reg.record(fixtures::report("ws1", "a2"));
        reg.record(fixtures::report("ws2", "b1"));
        let active = reg.active();
        assert_eq!(active.len(), 3);
        let ws1: Vec<&str> = active
            .iter()
            .filter(|r| r.workspace_id == "ws1")
            .map(|r| r.run_id.as_str())
            .collect();
        assert_eq!(ws1, ["a2", "a1"]);
    }

    #[test]
    fn clone_shares_state() {
        let reg = RunRegistry::new();
        let clone = reg.clone();
        reg.record(fixtures::report("ws1", "a"));
        assert_eq!(clone.recent("ws1", 1).len(), 1);
    }
}
