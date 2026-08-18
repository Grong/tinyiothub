//! Agent runtime — thing-agent loop, orchestrator, heartbeat runner, AI event
//! bus, agent pool contract（自 apps/cloud `domains::agent::loop_` 迁入，Task 13）。
//!
//! 纯运行时平面：不依赖 web 框架与存储实现；持久化经端口抽象（如
//! [`thing_agent::traits::AutonomyPolicyReader`]），实现住 apps/cloud。

pub mod agent;
pub mod event;
pub mod events;
pub mod heartbeat;
pub mod orchestrator;
pub mod runtime;
pub mod snapshot;
pub mod thing_agent;

pub use runtime::{AgentRuntime, RuntimeDeps};
