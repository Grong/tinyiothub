// Agent host module — HTTP/services plane (P4-Task22, was cloud::modules::agent)
//
// Task 14 后分层：通用机制（AgentPool / 工具框架 / SessionKey / prompt 组装 /
// 配置类型）住 `tinyiothub_agent` crate；本模块只剩组合层关切：
// chat/:          Chat capability (stateless ChatService + history persistence)
// tools/:         数据工具实现（thing/ canvas 等）+ ToolService 数据面
// config/:        Config capability (ConfigService + ConfigHandler)
// heartbeat.rs:   HeartbeatService (uses AgentPool directly)
// scaffold.rs:    Workspace scaffold + files CRUD
// ports.rs:       Composition seams (workspace access / storage adapters / MCP bridge)
// service.rs:     SessionService (session index, db-backed)
//
// 工具注册点：service_manager.rs 把数据工具 provider 注册进
// `tinyiothub_agent::tools::ToolRegistry`（Task 14）。

pub mod autonomous_factory;
pub mod chat;
pub mod config;
pub mod memory;
pub mod ports;
pub mod reflect;
pub mod tools;

pub mod agent_hooks;
// Test stub, also used by cloud's integration tests (agent_tasks_api_tests)
// — compiled unconditionally so downstream test harnesses can consume it.
pub mod directive_sink;
pub mod dlq_repo;
pub mod heartbeat;
pub mod persist;
pub mod policy_engine;
pub mod pool_adapter;
pub mod scaffold;
pub mod thing_action_hooks;
pub mod thing_agent_host;

#[cfg(test)]
pub(crate) mod test_utils;

// Re-exports from old modules/agent/ — kept for compat
pub mod handler;
pub mod service;
pub mod skill;
pub mod types;

pub use service::SessionService;
pub use skill::{AgentSkill, SkillType};
pub use types::*;
