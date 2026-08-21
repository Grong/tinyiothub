//! Agent-side implementation of the tenant domain's `AgentHooks` seam (G5b).
//!
//! Adapts the agent domain's heartbeat-task knowledge (the default task set)
//! to the tenant-owned trait so the tenant domain carries no dependency edge
//! on the agent domain. Dependency direction: agent → tenant.

use std::sync::{Arc, RwLock};

use tinyiothub_core::heartbeat::HeartbeatTask;

use crate::domains::agent::host::heartbeat;
use tinyiothub_agent::runtime::runtime::AgentRuntime;

/// [`AgentHooks`](crate::domains::tenant::hooks::AgentHooks) backed by the
/// agent domain's real implementations.
///
/// `runtime` 由 service_manager 在 AgentRuntime restore 后注入
/// （[`AgentHooksImpl::with_runtime`]）；未注入时
/// `heartbeat_tasks_seeded` 退化为告警 no-op（种子保持 DB-only，
/// 下次启动经快照恢复）。
#[derive(Default)]
pub struct AgentHooksImpl {
    runtime: RwLock<Option<Arc<AgentRuntime>>>,
}

impl AgentHooksImpl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Task 9 接线：注入 runtime，使新工作区种子任务可经 reload 命令
    /// 推入 runner 内存真源。
    pub fn with_runtime(self, runtime: Arc<AgentRuntime>) -> Self {
        *self.runtime.write().unwrap() = Some(runtime);
        self
    }
}

impl crate::domains::tenant::hooks::AgentHooks for AgentHooksImpl {
    fn default_heartbeat_tasks(&self) -> Vec<crate::domains::tenant::hooks::HeartbeatTaskDef> {
        heartbeat::get_default_tasks()
            .into_iter()
            .map(|t| crate::domains::tenant::hooks::HeartbeatTaskDef {
                priority: t.priority,
                text: t.text,
                paused: t.paused,
            })
            .collect()
    }

    fn heartbeat_tasks_seeded(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>) {
        let runtime = self.runtime.read().unwrap().clone();
        match runtime {
            // reload 命令：更新 runner 内存 + 发射 HeartbeatTasksChanged
            // （无投影，仅内存信号）；DB 已由调用方先写（D11-⑤）。
            Some(runtime) => runtime.reload_heartbeat_tasks(workspace_id, tasks),
            None => tracing::warn!(
                workspace_id,
                "agent runtime not wired; seeded heartbeat tasks remain DB-only until restart"
            ),
        }
    }
}
