//! AgentPool — agent 生命周期管理（Task 14 自 apps/cloud `host/agent/` 迁入）。
//!
//! 设计要点：
//!   - 存储无关（Task 7）：pool 不持有任何持久化句柄，方法签名亦不出现；
//!     调用方（组合层）先解析配置/工具列表，再注入 pool 的纯方法。
//!   - 显式创建：组合层解析 config + tools 后 `create`；`get_cached` 走快路径。
//!   - WorkspaceScopedMemory：namespace 包装实现 workspace 级隔离。
//!   - 失效：配置变更时从 pool 移除，下次访问重建。
//!
//! 文件划分（一事一文件）：
//!   pool.rs     — AgentPool 结构、生命周期（get_cached/create/invalidate/
//!                 cleanup）、agent builder、skills prompt section
//!   chat.rs     — chat abort + heartbeat runs（run_single/run_streaming）
//!                 + 流式结果类型
//!   provider.rs — ProviderFactory 与 minimax provider 缝（组合层注册设置）

mod chat;
#[allow(clippy::module_inception)]
mod pool;
pub mod provider;

pub use chat::{StreamingRunResult, StreamingToolCall, heartbeat_agent_id};
pub use pool::{Agent, AgentPool, load_workspace_skills};
pub use provider::{
    MinimaxSettings, ProviderFactory, create_minimax_provider, minimax_provider_factory, minimax_settings,
    set_minimax_settings,
};
