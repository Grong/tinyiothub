// Infrastructure Layer
// This module contains infrastructure concerns and external integrations

pub mod config;
pub mod hardware;
pub mod messaging;
pub mod persistence;

// Event infrastructure services
pub mod event;

// Self-healing infrastructure
pub mod self_healing;

// Redis 客户端 - 用于会话管理和频率限制
pub mod redis;

// Agent client (ZeroClaw adapter)
pub mod zeroclaw_agent;

// ZeroClaw runtime integration (embedded)
pub mod zeroclaw_runtime;

// Agent runtime module (consolidated interface)
pub mod agent;

// Batch command infrastructure
pub mod batch_command;

// Diagnostics infrastructure
pub mod diagnostics;
