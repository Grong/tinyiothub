//! Memory 接口面 — vendored 自 zeroclaw-api/src/memory_traits.rs。
//!
//! 方法组裁定：只 port `WorkspaceScopedMemory`（crates/agent/src/memory/
//! workspace_memory.rs）实际重写的那 11 个方法 —— name/store/recall/get/
//! list/forget/forget_for_agent/store_with_agent/recall_for_agents/count/
//! health_check。zeroclaw 的其余默认方法（get_for_agent/store_with_metadata/
//! recall_namespaced/purge_*/export/reindex 等）不 port。
//!
//! 注意：WorkspaceScopedMemory 的 store()/recall() 目前调用 inner 的
//! store_with_metadata()/recall_namespaced()（zeroclaw 默认方法）——
//! 迁移到本 trait 时这两个调用点需改写（Task 6 的事）。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::port::attribution::Attributable;

/// A single memory entry
#[derive(Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
    /// Namespace for isolation between agents/contexts.
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Importance score (0.0–1.0) for prioritized retrieval.
    #[serde(default)]
    pub importance: Option<f64>,
    /// If this entry was superseded by a newer conflicting entry.
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Resolved, human-readable agent alias for this row.
    #[serde(default)]
    pub agent_alias: Option<String>,
    /// Raw value of the storage layer's agent column.
    #[serde(default, alias = "agent_id")]
    pub agent_id: Option<String>,
}

fn default_namespace() -> String {
    "default".into()
}

impl std::fmt::Debug for MemoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEntry")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("content", &self.content)
            .field("category", &self.category)
            .field("timestamp", &self.timestamp)
            .field("score", &self.score)
            .field("namespace", &self.namespace)
            .field("importance", &self.importance)
            .field("agent_alias", &self.agent_alias)
            .finish_non_exhaustive()
    }
}

/// Memory categories for organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCategory {
    /// Long-term facts, preferences, decisions
    Core,
    /// Daily session logs
    Daily,
    /// Conversation context
    Conversation,
    /// User-defined custom category
    Custom(String),
}

impl serde::Serialize for MemoryCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for MemoryCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "core" => Self::Core,
            "daily" => Self::Daily,
            "conversation" => Self::Conversation,
            _ => Self::Custom(s),
        })
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Core memory trait — implement for any persistence backend
#[async_trait]
pub trait Memory: Send + Sync + Attributable {
    /// Backend name
    fn name(&self) -> &str;

    /// Store a memory entry, optionally scoped to a session
    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recall memories matching a query (keyword search), optionally scoped to a session
    /// and time range. Empty, whitespace-only, and bare "*" queries return recent/time-only
    /// entries. Non-bare wildcard terms such as "wild*" remain keyword queries.
    /// Time bounds use RFC 3339 / ISO 8601 format
    /// (e.g. "2025-03-01T00:00:00Z"); inclusive (created_at >= since, created_at <= until).
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Get a specific memory by key.
    ///
    /// After composite uniqueness landed, multiple rows may share a `key`
    /// (one per agent). This method returns *some* matching row without an
    /// agent filter.
    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;

    /// List all memory keys, optionally filtered by category and/or session
    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Remove a memory by key. Deletes every row matching `key`, regardless
    /// of agent attribution.
    async fn forget(&self, key: &str) -> anyhow::Result<bool>;

    /// Remove the row matching `(key, agent_id)`. Siblings of the same key
    /// under other agents are untouched. Returns `true` if a row was
    /// removed.
    async fn forget_for_agent(&self, key: &str, agent_id: &str) -> anyhow::Result<bool>;

    /// Count total memories
    async fn count(&self) -> anyhow::Result<usize>;

    /// Health check
    async fn health_check(&self) -> bool;

    /// Store a memory entry attributed to an explicit agent UUID.
    /// Every backend must implement this explicitly so the agent_id
    /// is never silently dropped at storage time.
    async fn store_with_agent(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recall memory entries scoped to a specific set of agent UUIDs.
    /// When `allowed_agent_ids` is non-empty, the backend filters its
    /// result set to rows whose `agent_id` matches one of the listed
    /// UUIDs (or is NULL, for legacy rows written before the agent_id
    /// column existed). Every backend must implement this explicitly
    /// so the allowlist is never silently dropped at read time.
    async fn recall_for_agents(
        &self,
        allowed_agent_ids: &[&str],
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;
}

/// No-op memory backend — 供测试与 phase 2 未接线场景。
pub struct NoopMemory;

impl Attributable for NoopMemory {
    fn role(&self) -> crate::port::attribution::Role {
        crate::port::attribution::Role::Memory(crate::port::attribution::MemoryKind::None)
    }
    fn alias(&self) -> &str {
        "noop"
    }
}

#[async_trait]
impl Memory for NoopMemory {
    fn name(&self) -> &str {
        "noop"
    }

    async fn store(
        &self,
        _key: &str,
        _content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(Vec::new())
    }

    async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(None)
    }

    async fn list(
        &self,
        _category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(Vec::new())
    }

    async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    async fn forget_for_agent(&self, _key: &str, _agent_id: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn health_check(&self) -> bool {
        true
    }

    async fn store_with_agent(
        &self,
        _key: &str,
        _content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
        _namespace: Option<&str>,
        _importance: Option<f64>,
        _agent_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recall_for_agents(
        &self,
        _allowed_agent_ids: &[&str],
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(Vec::new())
    }
}
