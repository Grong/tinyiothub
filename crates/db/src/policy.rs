//! Policy 持久化：工作区自治策略 + 动作频率读取（P-集中化 E3，自 agent crate 归位）。
//!
//! 值类型归位 core（Task 1），本模块经 glob re-export 组织 db 内部路径；
//! 全部 SQL 留在本文件，经 `Db` 门面委托暴露（Task 9）。
//! 注意：实现读取 agent_runs 表（动作频率熔断）——db 拥有全部表。

use sqlx::SqlitePool;

use crate::database::Db;
use crate::error::Result;

// 领域值类型住 core（tinyiothub_core::policy）；此处 re-export 仅为 db
// 内部模块组织，非跨 crate 摆渡层。
pub use tinyiothub_core::policy::*;

// ──────────────────────────────────────────────
// 持久化函数（pool 参数）+ Db 门面委托
// ──────────────────────────────────────────────

pub(crate) async fn load_autonomy(pool: &SqlitePool, workspace_id: &str) -> Result<Option<AutonomyPolicy>> {
    let row: Option<(String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT mode, allowed_actions, denied_actions,
                    max_actions_per_run, max_actions_per_hour
             FROM workspace_autonomy_policy WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
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

pub(crate) async fn save_autonomy(
    pool: &SqlitePool,
    workspace_id: &str,
    policy: &AutonomyPolicy,
    updated_by: &str,
) -> Result<()> {
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
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn count_actions_last_hour(pool: &SqlitePool, workspace_id: &str) -> Result<u32> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(json_extract(report, '$.action_count')), 0)
             FROM agent_runs
             WHERE workspace_id = ? AND created_at > datetime('now', '-1 hour')",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(count as u32)
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 读取工作区自治策略（缺行返回 None；未知 mode fail-closed 到 Off）。
    pub async fn load_autonomy_policy(&self, workspace_id: &str) -> Result<Option<AutonomyPolicy>> {
        load_autonomy(self.pool(), workspace_id).await
    }

    /// upsert 工作区自治策略（每工作区单行）。
    pub async fn save_autonomy_policy(
        &self,
        workspace_id: &str,
        policy: &AutonomyPolicy,
        updated_by: &str,
    ) -> Result<()> {
        save_autonomy(self.pool(), workspace_id, policy, updated_by).await
    }

    /// 最近一小时工作区 agent 动作总数（自治频率熔断输入）。
    pub async fn count_autonomy_actions_last_hour(&self, workspace_id: &str) -> Result<u32> {
        count_actions_last_hour(self.pool(), workspace_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub async fn test_pool() -> SqlitePool {
        crate::test_helpers::test_pool().await
    }

    fn act_policy() -> AutonomyPolicy {
        AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["reboot_thing".to_string(), "set_property".to_string()],
            denied_actions: vec!["wipe_thing".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    #[tokio::test]
    pub async fn save_then_load_roundtrips() {
        let pool = test_pool().await;
        let db = Db::new(pool);

        db.save_autonomy_policy("ws_1", &act_policy(), "user_1")
            .await
            .expect("save");
        let loaded = db.load_autonomy_policy("ws_1").await.expect("load").expect("persisted");

        assert_eq!(loaded.mode, AutonomyMode::Act);
        assert_eq!(loaded.allowed_actions, vec!["reboot_thing", "set_property"]);
        assert_eq!(loaded.denied_actions, vec!["wipe_thing"]);
        assert_eq!(loaded.max_actions_per_run, 3);
        assert_eq!(loaded.max_actions_per_hour, 30);
    }

    #[tokio::test]
    pub async fn load_missing_row_returns_none() {
        let pool = test_pool().await;
        let db = Db::new(pool);
        assert!(db.load_autonomy_policy("ws_missing").await.expect("load").is_none());
    }

    #[tokio::test]
    pub async fn save_twice_updates_in_place() {
        let pool = test_pool().await;
        let db = Db::new(pool);

        db.save_autonomy_policy("ws_1", &act_policy(), "user_1")
            .await
            .expect("save");
        let mut updated = act_policy();
        updated.mode = AutonomyMode::Diagnose;
        updated.max_actions_per_hour = 5;
        db.save_autonomy_policy("ws_1", &updated, "user_2")
            .await
            .expect("update");

        let loaded = db.load_autonomy_policy("ws_1").await.expect("load").expect("persisted");
        assert_eq!(loaded.mode, AutonomyMode::Diagnose);
        assert_eq!(loaded.max_actions_per_hour, 5);

        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workspace_autonomy_policy WHERE workspace_id = 'ws_1'")
            .fetch_one(db.pool())
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
        let db = Db::new(pool.clone());

        // Empty table -> 0
        assert_eq!(db.count_autonomy_actions_last_hour("ws_1").await.expect("count"), 0);

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

        assert_eq!(db.count_autonomy_actions_last_hour("ws_1").await.expect("count"), 5);
        assert_eq!(db.count_autonomy_actions_last_hour("ws_2").await.expect("count"), 11);
    }
}
