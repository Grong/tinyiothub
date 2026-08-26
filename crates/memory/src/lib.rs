//! Agent memory crate — 持久化引擎（MemoryService）。
//!
//! ## 设计不变量
//! - 纯逻辑模块（knowledge/reflect/types/metrics/workspace_memory/reference）
//!   已迁入 `tinyiothub_agent::memory`（Task 12, D10'），本 crate 只剩引擎
//! - 依赖方向 memory → agent，禁止反向；由组合层（apps/cloud agent 域）调用
//! - 禁止依赖 apps/*、web、runtime；db（SQLite 持久化）与 llm（embedding 契约）为例外

pub mod service;
