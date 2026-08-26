//! Memory store contract — trait and types for agent memory persistence.
//!
//! Definitions live in `core`; implementations live in `tinyiothub-memory`.

use serde::{Deserialize, Serialize};

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
    pub thing_id: Option<String>,
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
    pub thing_id: Option<String>,
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
            thing_id: None,
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
