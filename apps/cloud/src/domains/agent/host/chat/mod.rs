//! chat/ — Chat capability：无状态 ChatService（service）、会话历史持久化（history）、
//! OpenClaw 面向的 HTTP 代理面（handler，T16 起由 domains/agent/chat 并入，两处并一处）。
//! 谁调用：api/mod.rs 挂 handler 路由；thing_agent_host / pool_adapter / 测试经
//! history/service 进入。
//! 依赖谁：AgentPool / SessionKey（tinyiothub_agent）、persist/memory（prompt 记忆注入）、
//! host/types（ChatError/ChatEvent）、storage（会话历史落库）。

pub mod handler;
pub mod history;
pub mod service;
