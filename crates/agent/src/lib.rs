#![forbid(unsafe_code)]
//! Agent 共性能力运行时 — 从 `apps/cloud/src/domains/agent` 抽取的纯运行时平面。
//!
//! 承载 agent loop（[`runtime`]）、记忆（[`memory`]）、pool（[`pool`]）、
//! 工具框架（[`tools`]）、会话键（[`session`]）、prompt 组装（[`prompt`]）等共性能力。
//!
//! ## 设计不变量（CI 守卫词表见 ci.yml G9 守卫；本注释刻意避开守卫词）
//! - 零 Web 框架依赖（HTTP/Web 关切属于组合层 apps/cloud 的 host）
//! - 零 SQL / 零存储实现依赖（持久化经端口抽象，实现住 apps/cloud）
//! - 不感知 apps/cloud 的领域划分（不引用其 domains 路径）
//! - CI 守卫：Agent Loop Purity Guard (G9) + cargo tree 依赖树检查

pub mod config;
pub mod error;
pub mod memory;
pub mod pool;
pub mod prompt;
pub mod runtime;
pub mod session;
pub mod tools;

pub use error::AgentError;
