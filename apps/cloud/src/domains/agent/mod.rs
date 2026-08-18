//! Agent domain for TinyIoTHub — host + chat（Task 13 起 loop 运行时迁出）。
//!
//! Two internal layers, plus the extracted runtime crate:
//!
//! - `tinyiothub_agent::runtime`（原 `loop_`）— the agent runtime:
//!   thing-agent loop, orchestrator, heartbeat runner, AI event bus, agent
//!   pool contract. Pure runtime code; lives in crates/agent (Task 13).
//! - [`host`] — the HTTP/service host: AgentPool, session/chat services,
//!   tools, handlers, repos, prompt scaffolding. Consumes the runtime crate.
//! - [`chat`] — the chat session proxy plane (OpenClaw-facing handlers).
//!
//! ## 设计不变量
//! - 运行时/宿主隔离：crates/agent（纯运行时，不依赖 web/存储实现）← host + chat
//! - 跨领域调用只许 agent→{event,thing,policy,memory,skills,llm,auth}
//! - D5：本模块不再做兼容 re-export —— 共享契约类型由消费方直接从
//!   tinyiothub_agent / tinyiothub_core / tinyiothub_llm / tinyiothub_policy /
//!   tinyiothub_skills 的真实住所导入。

pub mod chat;
pub mod host;

use std::sync::Arc;

use tinyiothub_storage::Database;

// AppState 削除（G7 FromRef 切片）：handler 萃取 `State<AgentState>`。

/// Agent domain state slice (G7) — the fields of cloud's `AppState` the
/// agent host/chat handlers actually consume. The composition layer (cloud)
/// derives it via `FromRef<AppState>`; this crate never names `AppState`.
#[derive(Clone)]
pub struct AgentState {
    /// 数据库连接池 - agent runs/policy/heartbeat SQL 查询
    pub database: Arc<Database>,
    /// 工作空间服务 - agent_tasks 的 verify_workspace_access! 租户校验
    pub workspace_service: Arc<crate::domains::tenant::WorkspaceService>,
    /// 工作空间访问校验（tenant seam）- files/heartbeat handlers
    pub workspace_access: Arc<crate::state::TenantWorkspaceAccess>,
    /// 用户指令投递入口 - agent_tasks 指令端点（None 时 503）
    pub directive_sink: Option<Arc<dyn tinyiothub_agent::runtime::thing_agent::DirectiveSink>>,
    /// 心跳运行器 - workspace heartbeat 配置/任务/信任 API
    pub heartbeat_runner: Option<Arc<tinyiothub_agent::runtime::heartbeat::runner::HeartbeatRunner>>,
    /// AI subsystem orchestrator - agent run ack 的 O11 抑制回写（Task 6）
    pub orchestrator: Option<Arc<tinyiothub_agent::runtime::orchestrator::Orchestrator>>,
    /// Agent MemoryService - memory profile compile/weekly digest（Task 6 起
    /// cloud 侧自持，不再经 Orchestrator 中转）
    pub memory_service: Option<Arc<tinyiothub_memory::service::MemoryService>>,
    /// Agent 记忆存储 - memory handlers + chat prompt 构造
    pub memory_store: Arc<tinyiothub_storage::memory::MemoryStore>,
    /// Agent Pool - chat proxy 的会话/配置/工具 API
    pub agent_pool: Arc<tinyiothub_agent::pool::AgentPool>,
    /// 会话服务 - chat sessions 列表/标签/删除
    pub session_service: Arc<host::SessionService>,
    /// System prompts 配置 - chat proxy 构造 full prompt
    pub system_prompts: Arc<crate::shared::config::SystemPromptsConfig>,
}

impl AgentState {
    /// 获取数据库连接池
    pub fn db_pool(&self) -> sqlx::SqlitePool {
        self.database.pool().clone()
    }
}
