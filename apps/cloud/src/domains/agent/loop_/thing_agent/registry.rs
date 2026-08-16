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
//!
//! ## O11 problem_key dedup 元数据（Task 6）
//!
//! 心跳桥（orchestrator/callbacks.rs HeartbeatBridge）的 O11 抑制判定按
//! **problem_key** 键控、按**时间窗**（6h 问题窗口 / 7d ack 抑制窗）查询
//! （原 SQL `WHERE problem_key = ? AND created_at > now - window` 的直接证据，
//! 见 db/agent_runs.rs `last_problem_run`/`count_problem_runs`）。50 条报告
//! 窗口无法承接：繁忙工作区 50 条可能覆盖不足 6h，且 core `RunReport` 无
//! problem_key/时间戳/ack 字段。因此 dedup 元数据独立为一张压缩映射
//! `(workspace_id, problem_key) → {近期 run 结果, 最近 ack 时间}`，时间界
//! 驱逐（保留窗 = 7d ack 抑制窗）；**不扩大 50 条报告窗口**。
//!
//! ack 语义（行级保真，Task 6 fix round 1）：ack 附着于具体 run——
//! `ProblemRunMeta` 携带 `run_id` 与逐条 `acked` 标记，`mark_problem_acked`
//! 只标记该 run_id 对应条目，`last_problem_run` 读窗口内最新条目的 acked，
//! 与 DB `ack_run(run_id)` + `latest.acked_at IS NOT NULL` 语义一一对应
//! （旧模型塌缩为每 problem_key 一个 `last_acked_at`，会把"ack 旧 run"误判
//! 为最新 run 已 ack——见 task-6-report.md Fix Round 1）。
//!
//! 写入路径：manager run 完成时 [`RunRegistry::record_problem_run`]（仅心跳桥
//! 投递的指令携带 problem_key）；人工 ack 经 orchestrator → bridge →
//! [`RunRegistry::mark_problem_acked`]（ack 端点回写链携带 run_id）。
//! restore 预热不恢复本映射（core `RunReport` 无 problem_key——Task 9 快照
//! 构建器需另行携带，见 Task 6 报告）。

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tinyiothub_core::agent_runs::{Outcome, RunReport};

/// 每 workspace 保留的最近已完成 run 条数（超出驱逐最老）。
pub const COMPLETED_CAPACITY: usize = 50;

/// O11 dedup 元数据保留窗：与 ack 抑制窗（7d）对齐，超窗读/写时惰性裁剪。
const PROBLEM_META_RETENTION: Duration = Duration::from_secs(7 * 24 * 3600);

/// dedup 判定所需的最小 run 元数据（不带报告全文；`run_id` 供行级 ack 标记）。
#[derive(Clone)]
struct ProblemRunMeta {
    run_id: String,
    occurred_at: DateTime<Utc>,
    outcome: Outcome,
    verified: bool,
    acked: bool,
}

/// 每 (workspace, problem_key) 的 dedup 状态：近期 run（按 occurred_at
/// 升序，队尾为最新——乱序插入也保持有序，见 [`RunRegistry::record_problem_run`]）。
#[derive(Default)]
struct ProblemDedupState {
    runs: VecDeque<ProblemRunMeta>,
}

/// 跨 workspace 的内存 run 记录。Clone 廉价（内部 Arc），manager 依赖、
/// 测试探针与 AgentRuntime 门面共享同一实例。
#[derive(Clone, Default)]
pub struct RunRegistry {
    /// workspace_id → run 队列（旧→新，队尾为最新）。
    inner: Arc<DashMap<String, VecDeque<RunReport>>>,
    /// O11 dedup 元数据：(workspace_id, problem_key) → 压缩状态（Task 6）。
    problem_meta: Arc<DashMap<(String, String), ProblemDedupState>>,
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

    // ── O11 problem_key dedup 元数据（Task 6）──────────────────

    /// 记录一次 problem run 结果（仅心跳桥投递的指令携带 problem_key，由
    /// manager 在 run 完成时调用）。按 `occurred_at` 有序插入（乱序安全，
    /// 队尾恒为最新），写入时按保留窗裁剪该 key 的旧记录。
    pub fn record_problem_run(
        &self,
        workspace_id: &str,
        problem_key: &str,
        run_id: &str,
        outcome: Outcome,
        verified: bool,
        occurred_at: DateTime<Utc>,
    ) {
        let key = (workspace_id.to_string(), problem_key.to_string());
        let mut entry = self.problem_meta.entry(key).or_default();
        let meta = ProblemRunMeta {
            run_id: run_id.to_string(),
            occurred_at,
            outcome,
            verified,
            acked: false,
        };
        // 生产路径时间戳单调（队尾插入命中首个分支）；测试/prewarm 乱序插入
        // 时回退到按时间戳定位，保持"队尾为最新"不变式。
        let pos = entry
            .runs
            .iter()
            .rposition(|r| r.occurred_at <= occurred_at)
            .map_or(0, |p| p + 1);
        entry.runs.insert(pos, meta);
        prune_problem_entry(&mut entry);
    }

    /// O11 ack 抑制（行级保真）：只标记该 run_id 对应条目；未知 run_id
    /// （已驱逐/非 problem run）no-op，不创建垃圾条目。
    pub fn mark_problem_acked(&self, workspace_id: &str, problem_key: &str, run_id: &str) {
        let key = (workspace_id.to_string(), problem_key.to_string());
        if let Some(mut entry) = self.problem_meta.get_mut(&key)
            && let Some(run) = entry.runs.iter_mut().find(|r| r.run_id == run_id)
        {
            run.acked = true;
        }
    }

    /// 窗口内最近一次 problem run 的 `(outcome, verified, acked)`（按
    /// occurred_at 最新一条，严格大于 `now - window`，等价原 SQL
    /// `ORDER BY created_at DESC LIMIT 1` + `created_at > now - window`）。
    /// acked 为该条目的行级标记，等价 DB `latest.acked_at IS NOT NULL`。
    pub fn last_problem_run(&self, workspace_id: &str, problem_key: &str, window: Duration) -> Option<(Outcome, bool, bool)> {
        let cutoff = Utc::now() - chrono::Duration::from_std(window).ok()?;
        let entry = self
            .problem_meta
            .get(&(workspace_id.to_string(), problem_key.to_string()))?;
        entry
            .runs
            .iter()
            .rev()
            .find(|r| r.occurred_at > cutoff)
            .map(|r| (r.outcome, r.verified, r.acked))
    }

    /// 窗口内同 problem_key 的 run 计数（workspace 作用域，等价原 SQL 语义）。
    pub fn count_problem_runs(&self, workspace_id: &str, problem_key: &str, window: Duration) -> usize {
        let Ok(window) = chrono::Duration::from_std(window) else {
            return 0;
        };
        let cutoff = Utc::now() - window;
        self.problem_meta
            .get(&(workspace_id.to_string(), problem_key.to_string()))
            .map(|entry| entry.runs.iter().filter(|r| r.occurred_at > cutoff).count())
            .unwrap_or_default()
    }
}

/// 按保留窗裁剪 dedup 条目：相对该条目最新活动（新 run 时间），超窗旧 run
/// 出队。裁剪基准是条目内最新 run 而非 now——窗口查询（≤7d，以 now 为界）
/// 命中的 run 必在最新 run 的 7d 内，裁剪永不误删查询可见记录。
fn prune_problem_entry(entry: &mut ProblemDedupState) {
    let Some(newest) = entry.runs.back().map(|r| r.occurred_at) else {
        return;
    };
    let retention = chrono::Duration::from_std(PROBLEM_META_RETENTION).expect("retention fits chrono");
    while entry.runs.front().is_some_and(|r| newest - r.occurred_at > retention) {
        entry.runs.pop_front();
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
    use chrono::{DateTime, Utc};
    use std::time::Duration;
    use tinyiothub_core::agent_runs::Outcome;

    const H6: Duration = Duration::from_secs(6 * 3600);
    const D7: Duration = Duration::from_secs(7 * 24 * 3600);

    fn hours_ago(h: i64) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::hours(h)
    }

    // ── O11 problem_key dedup 元数据（Task 6）──────────────────

    #[test]
    fn problem_run_last_respects_window() {
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "run_a", Outcome::Acted, true, hours_ago(8));
        // 6h 窗口外不可见，7d 窗口内可见。
        assert!(reg.last_problem_run("ws1", "p1", H6).is_none());
        let got = reg.last_problem_run("ws1", "p1", D7).expect("in 7d window");
        assert_eq!(got, (Outcome::Acted, true, false));
    }

    // 行级保真（Task 6 fix round 1）：ack 附着于 run_id；仅当窗口内最新
    // run 被 ack 时才抑制（DB `latest.acked_at IS NOT NULL` 语义）。
    #[test]
    fn problem_run_ack_targets_specific_run_identity() {
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "run_a", Outcome::Acted, true, hours_ago(2));
        reg.record_problem_run("ws1", "p1", "run_b", Outcome::Acted, true, hours_ago(1));
        // ack 旧 run A：最新 run B 未 ack → 不得抑制。
        reg.mark_problem_acked("ws1", "p1", "run_a");
        let (_, _, acked) = reg.last_problem_run("ws1", "p1", H6).expect("found");
        assert!(!acked, "ack 旧 run 不得把更新的未 ack run 视为已 ack");
        // ack 最新 run B → 抑制。
        reg.mark_problem_acked("ws1", "p1", "run_b");
        let (_, _, acked) = reg.last_problem_run("ws1", "p1", H6).expect("found");
        assert!(acked, "窗口内最新 run 被 ack 才抑制");
        // 未知 run_id（已驱逐/非 problem run）no-op，不创建垃圾条目。
        reg.mark_problem_acked("ws1", "p1", "run_missing");
        reg.mark_problem_acked("ws1", "p_unknown", "run_missing");
        let (_, _, acked) = reg.last_problem_run("ws1", "p1", H6).expect("found");
        assert!(acked);
        assert!(reg.last_problem_run("ws1", "p_unknown", H6).is_none());
    }

    // ack 之后发生的新 run 自带 acked=false（行级语义天然覆盖）。
    #[test]
    fn problem_run_newer_than_ack_is_unacked() {
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "run_a", Outcome::Acted, true, hours_ago(50));
        reg.mark_problem_acked("ws1", "p1", "run_a");
        reg.record_problem_run("ws1", "p1", "run_b", Outcome::Failed, false, hours_ago(1));
        let (outcome, _, acked) = reg.last_problem_run("ws1", "p1", H6).expect("found");
        assert_eq!(outcome, Outcome::Failed);
        assert!(!acked, "run newer than the ack is not acked");
    }

    #[test]
    fn problem_run_out_of_order_insert_keeps_newest_last() {
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "new", Outcome::Failed, false, hours_ago(1));
        reg.record_problem_run("ws1", "p1", "old", Outcome::Acted, true, hours_ago(5));
        let (outcome, ..) = reg.last_problem_run("ws1", "p1", H6).expect("found");
        assert_eq!(outcome, Outcome::Failed, "乱序插入后队尾仍为最新 run");
    }

    #[test]
    fn problem_run_count_scoped_by_window_workspace_key() {
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "r1", Outcome::Acted, false, hours_ago(1));
        reg.record_problem_run("ws1", "p1", "r2", Outcome::Acted, false, hours_ago(5));
        reg.record_problem_run("ws1", "p1", "r3", Outcome::Acted, false, hours_ago(7)); // 6h 窗口外
        reg.record_problem_run("ws1", "p2", "r4", Outcome::Acted, false, hours_ago(1)); // 其他 key
        reg.record_problem_run("ws2", "p1", "r5", Outcome::Acted, false, hours_ago(1)); // 其他工作区
        assert_eq!(reg.count_problem_runs("ws1", "p1", H6), 2);
        assert_eq!(reg.count_problem_runs("ws1", "p1", D7), 3);
        assert_eq!(reg.count_problem_runs("ws1", "missing", H6), 0);
    }

    #[test]
    fn problem_meta_survives_report_window_eviction() {
        // 50 条报告窗口驱逐不影响 dedup 元数据（两者内存独立）。
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "r1", Outcome::Acted, false, hours_ago(1));
        for i in 0..55 {
            reg.record(fixtures::report("ws1", &format!("r{i}")));
        }
        assert_eq!(reg.count_problem_runs("ws1", "p1", H6), 1);
    }

    #[test]
    fn problem_meta_pruned_beyond_retention() {
        // 超保留窗（7d）的旧 run 在新写入时裁剪，不参与 7d 窗口查询。
        let reg = RunRegistry::new();
        reg.record_problem_run("ws1", "p1", "old", Outcome::Acted, false, hours_ago(8 * 24));
        reg.record_problem_run("ws1", "p1", "new", Outcome::Failed, false, hours_ago(1));
        let (outcome, ..) = reg.last_problem_run("ws1", "p1", D7).expect("newest");
        assert_eq!(outcome, Outcome::Failed);
        assert_eq!(reg.count_problem_runs("ws1", "p1", D7), 1, "超窗旧 run 已裁剪");
    }

    #[test]
    fn problem_meta_clone_shares_state() {
        let reg = RunRegistry::new();
        let clone = reg.clone();
        reg.record_problem_run("ws1", "p1", "r1", Outcome::Acted, false, hours_ago(1));
        assert_eq!(clone.count_problem_runs("ws1", "p1", H6), 1);
    }

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
