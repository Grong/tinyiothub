//! Agent 记忆纯逻辑 — 知识图谱、反思管道解析、工作区记忆适配。
//!
//! 从 crates/memory 吸收（Task 12, D10'）：唯一消费方是 agent 域。
//! 持久化引擎 MemoryService（吃 tinyiothub_storage）仍驻留 crates/memory，
//! 依赖方向为 memory → agent，禁止反向。

pub mod knowledge;
pub mod metrics;
pub mod reference;
pub mod reflect;
pub mod types;
pub mod workspace_memory;
