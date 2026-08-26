//! Agent prompt 模板 — 编译期内嵌（`include_str!`），单一家：
//! `crates/agent/templates/agent/`。
//!
//! 消费方：本 crate 的 prompt 组装（内嵌兜底层），以及组合层的 workspace
//! scaffold / files API / reflection（经这些常量引用，禁止跨 crate 相对路径
//! include）。

/// IDENTITY.md — 身份层模板
pub const IDENTITY_MD: &str = include_str!("../../templates/agent/IDENTITY.md");
/// SOUL.md — 行为原则模板
pub const SOUL_MD: &str = include_str!("../../templates/agent/SOUL.md");
/// AGENTS.md — agent 规则模板
pub const AGENTS_MD: &str = include_str!("../../templates/agent/AGENTS.md");
/// TOOLS.md — 能力说明模板
pub const TOOLS_MD: &str = include_str!("../../templates/agent/TOOLS.md");
/// USER.md — 用户上下文模板
pub const USER_MD: &str = include_str!("../../templates/agent/USER.md");
/// MEMORY.md — 长期记忆模板
pub const MEMORY_MD: &str = include_str!("../../templates/agent/MEMORY.md");
/// HEARTBEAT.md — 心跳清单模板
pub const HEARTBEAT_MD: &str = include_str!("../../templates/agent/HEARTBEAT.md");
/// BOOTSTRAP.md — 首次引导模板
pub const BOOTSTRAP_MD: &str = include_str!("../../templates/agent/BOOTSTRAP.md");
/// REFLECTION_PROMPT.md — 反思引擎指令模板
pub const REFLECTION_PROMPT_MD: &str = include_str!("../../templates/agent/REFLECTION_PROMPT.md");
/// COMPILE_PROMPT.md — profile 编译指令模板
pub const COMPILE_PROMPT_MD: &str = include_str!("../../templates/agent/COMPILE_PROMPT.md");
