//! Heartbeat 领域值类型：巡检任务/结果/信任配置（自 db/heartbeat.rs 归位，Task 1）。
//!
//! 纯值类型住 core，供 policy/skills/agent 等 crate 直接依赖；
//! HeartbeatTaskRepository 与全部 SQL 留在 db（WorkspaceHeartbeatConfig 为
//! DB 行序列化格式，同样留在 db）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Trust level for automatic tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// All tools require human approval.
    ApprovalRequired,
    /// Read-only tools auto-execute; write tools require approval.
    ReadOnlyAuto,
    /// All tools auto-execute.
    FullAuto,
}

/// Per-workspace trust configuration for tool auto-execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    pub trust_level: TrustLevel,
    pub max_auto_actions_per_tick: u32,
    pub allowed_tool_categories: Vec<String>,
    pub blocked_tools: Vec<String>,
    /// Destructive tools explicitly allowlisted by workspace admin.
    /// Only takes effect under FullAuto; all other levels still require approval.
    #[serde(default)]
    pub allowed_destructive_tools: Vec<String>,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::ReadOnlyAuto,
            max_auto_actions_per_tick: 10,
            allowed_tool_categories: vec!["read".into(), "query".into(), "write".into()],
            blocked_tools: vec![],
            allowed_destructive_tools: vec![],
        }
    }
}

impl TrustConfig {
    /// Load from DB JSON column, falling back to safe default.
    pub fn from_db_json(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
    }

    /// Serialize to JSON for DB storage.
    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Status of a heartbeat tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeartbeatStatus {
    Complete,
    Partial,
    Error,
}

/// A single action executed during a heartbeat tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedAction {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub success: bool,
    pub details: String,
}

/// Result of a heartbeat tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResult {
    pub workspace_id: String,
    pub status: HeartbeatStatus,
    pub summary: String,
    /// Number of tasks executed this tick (set by the loop, not the LLM).
    #[serde(default)]
    pub task_count: u32,
    pub executed_actions: Vec<ExecutedAction>,
    pub proposals: Vec<crate::policy::Proposal>,
    pub error: Option<String>,
}

/// A periodic heartbeat check task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    pub id: i64,
    pub workspace_id: String,
    pub priority: String,
    pub text: String,
    pub paused: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating/replacing heartbeat tasks (no server-assigned fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewHeartbeatTask {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Lowest interval a workspace may configure — a tick can take minutes
/// (LLM call + tool execution), so tighter loops just pile up.
pub const MIN_HEARTBEAT_INTERVAL_MINUTES: u32 = 5;
