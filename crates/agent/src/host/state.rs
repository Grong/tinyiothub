//! `AgentState` — the composition-state slice every agent host/chat handler
//! extracts. Routers are generic over `S: FromRef<AgentState>`; the
//! composition layer (cloud `AppState`) provides the `FromRef` impl.

use std::sync::Arc;

use tinyiothub_core::agent_hooks::AgentHooks;
use tinyiothub_core::config::SystemPromptsConfig;
use tinyiothub_storage::Database;
use tinyiothub_storage::cache::DeviceCache;
use tinyiothub_storage::memory::MemoryStore;

use crate::host::agent::AgentPool;
use crate::host::ports::WorkspaceAccess;
use crate::host::service::SessionService;
use crate::loop_::heartbeat::runner::HeartbeatRunner;
use crate::loop_::orchestrator::Orchestrator;
use crate::loop_::thing_agent::DirectiveSink;

/// State slice consumed by the agent host + chat HTTP planes.
#[derive(Clone)]
pub struct AgentState {
    /// 数据库连接
    pub database: Arc<Database>,
    /// Agent Pool — central agent lifecycle manager
    pub agent_pool: Arc<AgentPool>,
    /// 会话服务 - Agent 聊天会话管理
    pub session_service: Arc<SessionService>,
    /// Agent 记忆存储
    pub memory_store: Arc<MemoryStore>,
    /// 用户指令投递入口（T14）—— None 时指令入口返回 503
    pub directive_sink: Option<Arc<dyn DirectiveSink>>,
    /// 工作空间访问校验（tenant seam，组合层注入）
    pub workspace_access: Arc<dyn WorkspaceAccess>,
    /// 数据服务器 - 设备命令执行（tools）
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    /// 设备内存缓存（tools 读实时属性）
    pub device_cache: Arc<DeviceCache>,
    /// AI subsystem heartbeat runner (set during async startup)
    pub heartbeat_runner: Option<Arc<HeartbeatRunner>>,
    /// AI subsystem orchestrator (set during async startup)
    pub orchestrator: Option<Arc<Orchestrator>>,
    /// Agent hooks（P4.0d）—— workspace 域心跳任务 seam
    pub agent_hooks: Arc<dyn AgentHooks>,
    /// System prompts config（chat proxy 构造 full prompt）
    pub system_prompts: Arc<SystemPromptsConfig>,
}

impl AgentState {
    /// 数据库连接池（克隆开销低 — SqlitePool 内部是 Arc）
    pub fn db_pool(&self) -> sqlx::SqlitePool {
        self.database.pool().clone()
    }
}
