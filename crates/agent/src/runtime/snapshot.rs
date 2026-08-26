//! RestoreSnapshot — AgentRuntime 重启恢复 / 周期对账的状态快照（Task 3）。
//!
//! 快照只含 core 值类型与本模块定义的预热行类型，不含任何 repo / 连接句柄：
//! - 启动时由 cloud 从 DB 装配后注入 `AgentRuntime::restore`（Task 9 接线）；
//! - 运行期经 `AgentRuntime::dump_state()` 导出，用于 Lagged resync 与周期对账。

use chrono::{DateTime, Utc};
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};

/// 单个工作区的心跳运行态。
#[derive(Debug, Clone)]
pub struct WorkspaceHeartbeatState {
    pub workspace_id: String,
    pub tasks: Vec<HeartbeatTask>,
    pub trust_config: TrustConfig,
    pub interval_minutes: u32,
}

/// O11 dedup 预热行（Task 9）：agent_runs 表的 problem_key 元数据投影。
/// core [`RunReport`] 无 problem_key/时间戳/ack 字段，快照构建器必须直接
/// 查询 agent_runs（problem_key/outcome/verified/acked_at/created_at 列
/// 齐全）——否则重启后 dedup 状态为空，近期已处理问题会重复派发一次。
/// 仅启动 restore 预热消费；`dump_state` 导出为空（周期对账不投影本段，
/// DB 即其真相源）。
#[derive(Debug, Clone)]
pub struct ProblemMetaRow {
    pub workspace_id: String,
    pub problem_key: String,
    pub run_id: String,
    pub outcome: Outcome,
    pub verified: bool,
    pub acked: bool,
    pub occurred_at: DateTime<Utc>,
}

/// resync 补插用的 run 级 dedup 键（CEO review T3）。
/// core [`RunReport`] 不含这两字段；事件路径在发射时携带、落库齐全，
/// 但 Lagged/对账补插缺失行时需要内存旁路提供，否则补插行永久丢失
/// problem_key，重启后 O11 dedup 失效、问题重复派发。
#[derive(Debug, Clone, Default)]
pub struct RunDedupKeys {
    pub problem_key: Option<String>,
    pub dedup_key: Option<String>,
}

/// AgentRuntime 全量状态快照。
#[derive(Debug, Clone, Default)]
pub struct RestoreSnapshot {
    pub heartbeat: Vec<WorkspaceHeartbeatState>,
    /// pushback/dedup 预热窗口：每 workspace 最近 50 条。
    /// 顺序契约（Task 9）：构建器必须按 **旧→新** 排列（SQL `ORDER BY
    /// created_at ASC, rowid ASC`）——[`RunReport`] 无时间戳，
    /// `RunRegistry::prewarm` 无法自排序，乱序输入会破坏"队尾为最新"。
    pub recent_runs: Vec<RunReport>,
    /// O11 dedup 元数据预热段（Task 9）：agent_runs 7d 保留窗内的
    /// problem_key 行；顺序无关（`RunRegistry::prewarm_problem_meta`
    /// 按 occurred_at 有序插入）。
    pub problem_meta: Vec<ProblemMetaRow>,
    /// run_id → dedup 键旁路（CEO review T3）：仅 `dump_state` 导出时填充
    /// （RunRegistry 窗口内 run 的发射期元数据）；启动构建器置空——预热行
    /// 在 DB 已有完整记录，resync 对它们只会幂等 no-op。
    pub recent_run_meta: std::collections::HashMap<String, RunDedupKeys>,
    /// 近期心跳结果（CEO review T22）：仅 `dump_state` 导出时填充
    /// （HeartbeatRunner 窗口，每工作区 cap 20）——Lagged resync/周期对账
    /// 补回丢失的 agent_actions 行（tick_id 幂等）。启动构建器置空：
    /// DB 即其真相源，预热只会让窗口与库双倍陈旧。
    pub heartbeat_results: Vec<tinyiothub_core::heartbeat::HeartbeatResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_snapshot_is_empty() {
        let snap = RestoreSnapshot::default();
        assert!(snap.heartbeat.is_empty());
        assert!(snap.recent_runs.is_empty());
        assert!(snap.problem_meta.is_empty());
    }
}
