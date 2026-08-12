//! Policy 持久化：工作区自治策略 + 动作频率读取（P-集中化 E3，自 agent crate 归位）。
//!
//! 类型随 repo 住 db（方案 B）：AutonomyPolicy/AutonomyMode 为 DB 行类型，
//! policy crate 保留 GateVerdict 评估纯逻辑，经 re-export 兼容。
//! 注意：实现读取 agent_runs 表（动作频率熔断）——db 拥有全部表。

use sqlx::SqlitePool;

use crate::error::Result;

// ──────────────────────────────────────────────
// 持久化类型（DB 行）— 自 policy/autonomy.rs 迁入
// ──────────────────────────────────────────────

/// Three-state autonomy mode for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyMode {
    Off,
    Diagnose,
    Act,
}

impl AutonomyMode {
    /// Stable string stored in `workspace_autonomy_policy.mode`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AutonomyMode::Off => "off",
            AutonomyMode::Diagnose => "diagnose",
            AutonomyMode::Act => "act",
        }
    }

    /// Inverse of `as_str`; unknown values return None (treat as fail-closed).
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "off" => Some(AutonomyMode::Off),
            "diagnose" => Some(AutonomyMode::Diagnose),
            "act" => Some(AutonomyMode::Act),
            _ => None,
        }
    }
}

/// Workspace-level autonomy policy for the thing-agent loop.
#[derive(Debug, Clone)]
pub struct AutonomyPolicy {
    pub mode: AutonomyMode,
    /// Allowed action names; `["*"]` means all actions.
    pub allowed_actions: Vec<String>,
    /// Denied action names (exact match); checked before the allowlist.
    pub denied_actions: Vec<String>,
    pub max_actions_per_run: u32,
    pub max_actions_per_hour: u32,
}

// ──────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────

pub struct PolicyRepository {
    pool: SqlitePool,
}

impl PolicyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl PolicyRepository {
    pub async fn load_autonomy(&self, workspace_id: &str) -> Result<Option<AutonomyPolicy>> {
        let row: Option<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT mode, allowed_actions, denied_actions,
                    max_actions_per_run, max_actions_per_hour
             FROM workspace_autonomy_policy WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((mode, allowed, denied, max_run, max_hour)) = row else {
            return Ok(None);
        };
        // Unknown mode string -> Off (fail-closed; the CHECK constraint should
        // make this unreachable).
        let mode = AutonomyMode::from_db(&mode).unwrap_or(AutonomyMode::Off);
        Ok(Some(AutonomyPolicy {
            mode,
            allowed_actions: serde_json::from_str(&allowed)?,
            denied_actions: serde_json::from_str(&denied)?,
            max_actions_per_run: max_run as u32,
            max_actions_per_hour: max_hour as u32,
        }))
    }

    pub async fn save_autonomy(&self, workspace_id: &str, policy: &AutonomyPolicy, updated_by: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO workspace_autonomy_policy
                 (workspace_id, mode, allowed_actions, denied_actions,
                  max_actions_per_run, max_actions_per_hour, updated_by, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
             ON CONFLICT(workspace_id) DO UPDATE SET
                 mode = excluded.mode,
                 allowed_actions = excluded.allowed_actions,
                 denied_actions = excluded.denied_actions,
                 max_actions_per_run = excluded.max_actions_per_run,
                 max_actions_per_hour = excluded.max_actions_per_hour,
                 updated_by = excluded.updated_by,
                 updated_at = excluded.updated_at",
        )
        .bind(workspace_id)
        .bind(policy.mode.as_str())
        .bind(serde_json::to_string(&policy.allowed_actions)?)
        .bind(serde_json::to_string(&policy.denied_actions)?)
        .bind(policy.max_actions_per_run as i64)
        .bind(policy.max_actions_per_hour as i64)
        .bind(updated_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn count_actions_last_hour(&self, workspace_id: &str) -> Result<u32> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(json_extract(report, '$.action_count')), 0)
             FROM agent_runs
             WHERE workspace_id = ? AND created_at > datetime('now', '-1 hour')",
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count as u32)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    pub async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory sqlite");
        let migration = include_str!("../migrations/20260729000001_thing_agent_loop.sql");
        for stmt in migration.split(';') {
            let stmt = stmt.trim();
            // Skip the events ALTER — the events table is not part of this pool.
            if !stmt.is_empty() && !stmt.starts_with("ALTER TABLE") {
                sqlx::query(stmt).execute(&pool).await.expect("apply migration");
            }
        }
        pool
    }

    fn act_policy() -> AutonomyPolicy {
        AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["reboot_device".to_string(), "set_property".to_string()],
            denied_actions: vec!["wipe_device".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    #[tokio::test]
    pub async fn save_then_load_roundtrips() {
        let pool = test_pool().await;
        let repo = PolicyRepository::new(pool);

        repo.save_autonomy("ws_1", &act_policy(), "user_1").await.expect("save");
        let loaded = repo.load_autonomy("ws_1").await.expect("load").expect("persisted");

        assert_eq!(loaded.mode, AutonomyMode::Act);
        assert_eq!(loaded.allowed_actions, vec!["reboot_device", "set_property"]);
        assert_eq!(loaded.denied_actions, vec!["wipe_device"]);
        assert_eq!(loaded.max_actions_per_run, 3);
        assert_eq!(loaded.max_actions_per_hour, 30);
    }

    #[tokio::test]
    pub async fn load_missing_row_returns_none() {
        let pool = test_pool().await;
        let repo = PolicyRepository::new(pool);
        assert!(repo.load_autonomy("ws_missing").await.expect("load").is_none());
    }

    #[tokio::test]
    pub async fn save_twice_updates_in_place() {
        let pool = test_pool().await;
        let repo = PolicyRepository::new(pool);

        repo.save_autonomy("ws_1", &act_policy(), "user_1").await.expect("save");
        let mut updated = act_policy();
        updated.mode = AutonomyMode::Diagnose;
        updated.max_actions_per_hour = 5;
        repo.save_autonomy("ws_1", &updated, "user_2").await.expect("update");

        let loaded = repo.load_autonomy("ws_1").await.expect("load").expect("persisted");
        assert_eq!(loaded.mode, AutonomyMode::Diagnose);
        assert_eq!(loaded.max_actions_per_hour, 5);

        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workspace_autonomy_policy WHERE workspace_id = 'ws_1'")
            .fetch_one(&repo.pool)
            .await
            .expect("count rows");
        assert_eq!(n, 1, "upsert must keep a single row per workspace");
    }

    pub async fn insert_run(pool: &SqlitePool, workspace_id: &str, action_count: i64, age_modifier: &str) {
        // age_modifier e.g. "-59 minutes"; bound as a datetime() modifier parameter
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, report, created_at)
             VALUES (?, ?, 'timer', 'success', json_object('action_count', ?), datetime('now', ?))",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(workspace_id)
        .bind(action_count)
        .bind(age_modifier)
        .execute(pool)
        .await
        .expect("insert agent_run");
    }

    #[tokio::test]
    pub async fn count_actions_last_hour_sums_in_window_and_ignores_old_rows() {
        let pool = test_pool().await;
        let repo = PolicyRepository::new(pool.clone());

        // Empty table -> 0
        assert_eq!(repo.count_actions_last_hour("ws_1").await.expect("count"), 0);

        insert_run(&pool, "ws_1", 2, "-59 minutes").await;
        insert_run(&pool, "ws_1", 3, "-10 minutes").await;
        // Outside the 1-hour window -> excluded
        insert_run(&pool, "ws_1", 7, "-61 minutes").await;
        // Other workspace -> excluded
        insert_run(&pool, "ws_2", 11, "-5 minutes").await;
        // Report without action_count -> contributes nothing
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, report, created_at)
             VALUES ('run_no_count', 'ws_1', 'timer', 'success', '{}', datetime('now'))",
        )
        .execute(&pool)
        .await
        .expect("insert run without action_count");

        assert_eq!(repo.count_actions_last_hour("ws_1").await.expect("count"), 5);
        assert_eq!(repo.count_actions_last_hour("ws_2").await.expect("count"), 11);
    }
}

// ── 行动提案（自 policy/proposal.rs 迁入，E6b）──

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    /// Tool or action name being proposed.
    pub tool_name: String,
    /// Target device or resource.
    pub device_id: Option<String>,
    /// Human-readable summary of what this will do.
    pub summary: String,
    /// Why the agent wants to take this action.
    pub reason: String,
    /// Risk assessment (low/medium/high).
    pub risk: String,
    /// Proposed parameters (tool-specific).
    pub parameters: Option<serde_json::Value>,
    /// ISO 8601 timestamp.
    pub created_at: String,
    /// Status lifecycle: Pending → Approved / Rejected.
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

impl std::fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalStatus::Pending => write!(f, "pending"),
            ProposalStatus::Approved => write!(f, "approved"),
            ProposalStatus::Rejected => write!(f, "rejected"),
        }
    }
}
