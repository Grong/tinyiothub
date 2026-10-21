// Agent host module — HTTP/services plane (P4-Task22, was cloud::modules::agent)
//
// Task 16 起按概念分组（20+ 平铺 → 归位）：
// judge/    判定链：judgment_subscriber + policy_engine
// notify/   通知链：ticket_subscriber（开票 + SSE 广播装配）
// persist/  持久化链：persist + dlq_repo + memory/
// trigger/  触发链：directive_sink + thing_action_hooks + agent_hooks
// chat/     Chat capability（service + history + handler，domains/agent/chat 已并入）
// tools/    数据工具实现（thing/canvas 等）+ ToolService 数据面——不动，已是边界
// config/   Config capability (ConfigService + ConfigHandler)
// handler/  HTTP handlers（agent_tasks / files / skills / workspace_heartbeat 等）
//
// host 壳（平铺保留）：
// thing_agent_host.rs / service.rs / scaffold.rs / pool_adapter.rs
// heartbeat.rs / ports.rs / reflect.rs / skill.rs / types.rs / autonomous_factory.rs
//
// 每组 mod.rs 头部有三行契约（做什么/谁调用/依赖谁）。注册表关系注记
// （RunRegistry vs ActiveRunRegistry，eng S1-P3）见 persist/mod.rs。
//
// 工具注册点：service_manager.rs 把数据工具 provider 注册进
// `tinyiothub_agent::tools::ToolRegistry`（Task 14）。

pub mod autonomous_factory;
pub mod chat;
pub mod composite;
pub mod config;
pub mod judge;
pub mod notify;
pub mod persist;
pub mod ports;
pub mod reflect;
pub mod tools;
pub mod trigger;

pub mod heartbeat;
pub mod pool_adapter;
pub mod scaffold;
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
