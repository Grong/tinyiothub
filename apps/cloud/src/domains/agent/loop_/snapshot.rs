//! RestoreSnapshot — AgentRuntime 重启恢复 / 周期对账的状态快照（Task 3）。
//!
//! 快照只含 core 值类型，不含任何 repo / 连接句柄：
//! - 启动时由 cloud 从 DB 装配后注入 `AgentRuntime::restore`（Task 11 接线）；
//! - 运行期经 `AgentRuntime::dump_state()` 导出，用于 Lagged resync 与周期对账。

use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};

/// 单个工作区的心跳运行态。
#[derive(Debug, Clone)]
pub struct WorkspaceHeartbeatState {
    pub workspace_id: String,
    pub tasks: Vec<HeartbeatTask>,
    pub trust_config: TrustConfig,
    pub interval_minutes: u32,
}

/// AgentRuntime 全量状态快照。
#[derive(Debug, Clone, Default)]
pub struct RestoreSnapshot {
    pub heartbeat: Vec<WorkspaceHeartbeatState>,
    /// pushback/dedup 预热窗口：每 workspace 最近 50 条
    pub recent_runs: Vec<RunReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_snapshot_is_empty() {
        let snap = RestoreSnapshot::default();
        assert!(snap.heartbeat.is_empty());
        assert!(snap.recent_runs.is_empty());
    }
}
