//! persist/ — 持久化链：AI 事件持久化订阅（persist.rs）、DLQ 仓储（dlq_repo）、
//! 记忆存储与 HTTP 面（memory/）。
//! 谁调用：service_manager 装配 `supervise_persistence_subscriber`；resync 对账由
//! thing_agent 统一循环触发；memory handler 经 api/mod.rs 挂路由。
//! 依赖谁：runtime 事件总线/AI 事件总线、storage（Db/MemoryStore）、AgentRuntime
//! （快照恢复）；不依赖 judge/notify/trigger。
//!
//! 注册表关系注记（eng S1-P3）：persist.rs 的 RunRegistry 投影 = 记忆注入源
//! （chat prompt 构造读最近 run 摘要）；`tinyiothub_agent::runtime::thing_agent::ActiveRunRegistry`
//! = 实时 trace 源（trace 端点读在途 run）。两者职责不同，归一时不合并。

pub mod dlq_repo;
pub mod memory;
// 纯移动分组（T16）：文件名保持 persist.rs 不改名，组名与模块同名属预期。
#[allow(clippy::module_inception)]
pub mod persist;
