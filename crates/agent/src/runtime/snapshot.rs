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
