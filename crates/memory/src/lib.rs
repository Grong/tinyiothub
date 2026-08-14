//! Agent memory crate — memory store, reflection pipeline, knowledge.
//!
//! ## 设计不变量
//! - 记忆/知识存储与反思管道；由组合层（apps/cloud agent 域）调用，禁止反向依赖
//! - 禁止依赖 apps/*、web、runtime；db（SQLite 持久化）与 llm（embedding 契约）为例外

pub mod knowledge;
pub mod metrics;
pub mod reference;
pub mod reflect;
pub mod service;
pub mod types;
pub mod workspace_memory;

pub use tinyiothub_llm::provider;

pub use tinyiothub_core::memory::{
    AgentMemory, Confidence, MemoryInput, MemorySource, MemoryZone, QueueCandidateInput, ReflectionQueueItem,
};
