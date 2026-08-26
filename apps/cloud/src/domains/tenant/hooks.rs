//! Tenant-owned hooks seam (G5b) — the traits through which the tenant
//! domain consumes agent-owned capabilities (the default heartbeat task
//! set) and publishes workspace lifecycle events onto the AI event plane.
//!
//! The tenant domain defines the traits; the agent domain implements
//! `AgentHooks` (`AgentHooksImpl`, agent-side host adapter) and the
//! composition layer implements `WorkspaceEventPublisher`
//! (`shared::ai_adapter::WorkspaceAiPublisherAdapter`), injecting both as
//! trait objects. Dependency direction: agent → tenant (never the reverse).

/// A heartbeat task definition crossing the tenant→agent boundary (value
/// type). Mirrors the agent-side task entry (priority/text/paused);
/// server-assigned fields (id, version, timestamps) never cross this seam.
#[derive(Debug, Clone)]
pub struct HeartbeatTaskDef {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Agent-owned capabilities consumed by the tenant domain's workspace
/// service.
pub trait AgentHooks: Send + Sync {
    /// The default heartbeat task set seeded into every new workspace.
    fn default_heartbeat_tasks(&self) -> Vec<HeartbeatTaskDef>;

    /// 种子任务已落库（DB 先写，D11-⑤）：把回读的全量行同步推入 agent
    /// 运行时内存真源（Task 9）。必须在 `publish_workspace_created` 之前
    /// 调用 —— 排队的 WorkspaceCreated → heartbeat start 才能读到任务集
    /// （否则 runner 内存为空，loop 跳过启动）。tasks 为 core 值类型
    /// （tinyiothub_core::heartbeat::HeartbeatTask，含 server 字段——
    ///  runner 内存真源需要真实行，不违反 agent→tenant 依赖方向）。
    /// 默认 no-op（未接线 runtime 时种子保持 DB-only，重启后恢复）。
    fn heartbeat_tasks_seeded(&self, workspace_id: &str, tasks: Vec<tinyiothub_core::heartbeat::HeartbeatTask>) {
        let _ = (workspace_id, tasks);
    }
}

/// Outbound port for workspace lifecycle events on the AI event plane.
/// Implemented by the composition layer over the agent-owned event bus.
pub trait WorkspaceEventPublisher: Send + Sync {
    /// A workspace was created (after the row is committed).
    fn publish_workspace_created(&self, workspace_id: String);

    /// A workspace was deleted (after the row is gone).
    fn publish_workspace_deleted(&self, workspace_id: String);
}
