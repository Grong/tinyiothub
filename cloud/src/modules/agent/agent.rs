// Agent struct + AgentPool + PoolEntry
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use sqlx::SqlitePool;

use crate::shared::agent::config::AgentRuntimeConfig;

pub struct Agent {
    pub agent_id: String,
    pub workspace_id: String,
    pub config: AgentRuntimeConfig,
}

pub struct AgentPool {
    pub(crate) agents: Arc<DashMap<String, PoolEntry>>,
    pub(crate) db_pool: SqlitePool,
    pub(crate) shared_memory: Arc<dyn zeroclaw::memory::Memory>,
    pub(crate) observer: Arc<dyn zeroclaw::observability::Observer>,
    pub(crate) response_cache: Option<Arc<zeroclaw::memory::ResponseCache>>,
    pub(crate) agent_settings: crate::shared::config::AgentSettings,
    pub chat_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
}

pub(crate) struct PoolEntry {
    pub zeroclaw_agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
    pub metadata: Agent,
    pub last_used: Instant,
}
