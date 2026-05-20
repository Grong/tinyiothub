//! Memory store contract — trait and types for agent memory persistence.
//!
//! Definitions live in `core`; implementations live in `tinyiothub-memory`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Zone-based memory partitioning (Memory Palace).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryZone {
    Core,
    Work,
    Episode,
    General,
}

impl MemoryZone {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Work => "work",
            Self::Episode => "episode",
            Self::General => "general",
        }
    }

    pub fn injection_priority(&self) -> u8 {
        match self {
            Self::Core => 0,
            Self::Work => 1,
            Self::General => 2,
            Self::Episode => 3,
        }
    }
}

/// Who or what created this memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    User,
    Reflection,
    Import,
    System,
    DeviceSnapshot,
}

/// Confidence level for auto-accept decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A single agent memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub zone: MemoryZone,
    pub content: String,
    pub source: MemorySource,
    pub confidence: Confidence,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub supersedes: Option<String>,
    pub device_id: Option<String>,
    pub snapshot_data: Option<String>,
    pub snapshot_time: Option<i64>,
    pub effectiveness: f64,
    pub load_count: u32,
    pub reference_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating a new memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInput {
    pub workspace_id: String,
    pub agent_id: String,
    pub zone: MemoryZone,
    pub content: String,
    pub source: MemorySource,
    pub confidence: Confidence,
    pub tags: Vec<String>,
    pub supersedes: Option<String>,
    pub device_id: Option<String>,
    pub snapshot_data: Option<String>,
    pub snapshot_time: Option<i64>,
}

impl Default for MemoryInput {
    fn default() -> Self {
        Self {
            workspace_id: String::new(),
            agent_id: String::new(),
            zone: MemoryZone::General,
            content: String::new(),
            source: MemorySource::User,
            confidence: Confidence::Medium,
            tags: vec![],
            supersedes: None,
            device_id: None,
            snapshot_data: None,
            snapshot_time: None,
        }
    }
}

/// A reflection queue item awaiting review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionQueueItem {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub session_key: String,
    pub candidate_type: String,
    pub candidate_data: String,
    pub status: String,
    pub created_at: String,
}

/// Input for enqueueing a reflection candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueCandidateInput {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_key: String,
    pub candidate_type: String,
    pub candidate_data: String,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Core trait for agent memory persistence.
///
/// Implementations live in `tinyiothub-memory` and store data via
/// `tinyiothub-storage`.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Store a new memory. Auto-generates id and timestamps.
    async fn put(&self, input: MemoryInput) -> Result<AgentMemory>;

    /// Get a single memory by id.
    async fn get(&self, id: &str) -> Result<Option<AgentMemory>>;

    /// Get all memories for a workspace/agent pair.
    async fn get_all(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<AgentMemory>>;

    /// List active (non-superseded) memories, sorted by retrieval score.
    async fn list_active(&self, workspace_id: &str, agent_id: &str) -> Result<Vec<AgentMemory>>;

    /// Get memories created after a timestamp.
    async fn get_since(
        &self,
        workspace_id: &str,
        agent_id: &str,
        since: &str,
    ) -> Result<Vec<AgentMemory>>;

    /// Pin or unpin a memory.
    async fn set_pinned(&self, id: &str, pinned: bool) -> Result<()>;

    /// Record a load event (memory injected into prompt). Increments load_count.
    async fn record_load(&self, id: &str) -> Result<()>;

    /// Record a reference event (LLM actually used this memory).
    /// Increments reference_count and recomputes effectiveness.
    async fn record_reference(&self, id: &str) -> Result<()>;

    /// Get pending reflection queue items.
    async fn get_pending_queue(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<Vec<ReflectionQueueItem>>;

    /// Approve or reject a queue item.
    async fn resolve_queue_item(
        &self,
        id: &str,
        approved: bool,
        reviewer_note: Option<&str>,
    ) -> Result<()>;

    /// Enqueue a candidate for review.
    async fn enqueue_candidate(&self, item: QueueCandidateInput) -> Result<String>;

    /// Count memories by source.
    async fn count_by_source(
        &self,
        workspace_id: &str,
        agent_id: &str,
        source: MemorySource,
    ) -> Result<u64>;
}
