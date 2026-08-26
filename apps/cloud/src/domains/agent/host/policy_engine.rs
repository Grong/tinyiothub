//! SQLite implementation of the unified PolicyEngine trait (X3/T16).
//!
//! Loads rules from the `policy_rules` table and arbitrates them through
//! `tinyiothub_policy::evaluate_rules` (priority desc, ties fail safe,
//! default Allow). Read failures are fail-closed as
//! `RequireApproval("policy_read_failed")` — defer to a human rather than
//! silently allow or hard-block.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use tinyiothub_policy::{PolicyAction, PolicyCategory, PolicyDecision, PolicyEngine, PolicyRule, evaluate_rules};

/// PolicyEngine backed by the `policy_rules` table.
///
/// Rules are loaded per workspace and arbitrated through
/// [`evaluate_rules`](tinyiothub_policy::evaluate_rules) — priority desc,
/// ties fail safe, default Allow. Rows whose category/action strings don't
/// parse are skipped so a bad row never poisons evaluation. Read failures fail
/// closed as `RequireApproval("policy_read_failed")` — defer to a human rather
/// than silently allow or hard-block.
#[derive(Clone)]
pub struct SqlitePolicyEngine {
    pool: SqlitePool,
}

impl SqlitePolicyEngine {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyEngine for SqlitePolicyEngine {
    async fn evaluate(&self, workspace_id: &str, category: PolicyCategory, target: &str) -> PolicyDecision {
        match self.load_rules(workspace_id).await {
            Ok(rules) => evaluate_rules(&rules, workspace_id, category, target),
            Err(_) => PolicyDecision::RequireApproval {
                reason: "policy_read_failed".to_string(),
            },
        }
    }

    async fn add_rule(&self, rule: PolicyRule) -> anyhow::Result<()> {
        sqlx::query(
            // guard-exempt: policy trait impl（Task 12 裁决）
            "INSERT INTO policy_rules (id, workspace_id, category, action, target, priority, reason)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               workspace_id = excluded.workspace_id,
               category = excluded.category,
               action = excluded.action,
               target = excluded.target,
               priority = excluded.priority,
               reason = excluded.reason",
        )
        .bind(&rule.id)
        .bind(&rule.workspace_id)
        .bind(rule.category.as_str())
        .bind(rule.action.as_str())
        .bind(&rule.target)
        .bind(rule.priority as i64)
        .bind(&rule.reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remove_rule(&self, rule_id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM policy_rules WHERE id = ?") // guard-exempt: policy trait impl（Task 12 裁决）
            .bind(rule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_rules(&self, workspace_id: &str) -> Vec<PolicyRule> {
        self.load_rules(workspace_id).await.unwrap_or_default()
    }
}

impl SqlitePolicyEngine {
    /// Load a workspace's rules sorted by priority desc (rowid as the stable
    /// tiebreaker). Rows whose category/action strings don't parse are skipped.
    async fn load_rules(&self, workspace_id: &str) -> anyhow::Result<Vec<PolicyRule>> {
        let rows = sqlx::query(
            // guard-exempt: policy trait impl（Task 12 裁决）
            "SELECT id, workspace_id, category, action, target, priority, reason
             FROM policy_rules WHERE workspace_id = ? ORDER BY priority DESC, rowid",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(category) = PolicyCategory::from_db(row.try_get::<String, _>("category")?.as_str()) else {
                continue;
            };
            let Some(action) = PolicyAction::from_db(row.try_get::<String, _>("action")?.as_str()) else {
                continue;
            };
            rules.push(PolicyRule {
                id: row.try_get("id")?,
                workspace_id: row.try_get("workspace_id")?,
                category,
                action,
                target: row.try_get("target")?,
                priority: row.try_get::<i64, _>("priority")? as u32,
                reason: row.try_get("reason")?,
            });
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use tinyiothub_policy::{PolicyAction, PolicyCategory, PolicyDecision, PolicyEngine, PolicyRule};

    use super::*;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory sqlite");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("run migrations");
        pool
    }

    fn rule(id: &str, ws: &str, action: PolicyAction, target: &str, priority: u32) -> PolicyRule {
        PolicyRule {
            id: id.to_string(),
            workspace_id: ws.to_string(),
            category: PolicyCategory::AgentAction,
            action,
            target: target.to_string(),
            priority,
            reason: format!("reason-{id}"),
        }
    }

    #[tokio::test]
    async fn add_list_remove_roundtrip() {
        let pool = test_pool().await;
        let engine = SqlitePolicyEngine::new(pool);

        engine
            .add_rule(rule("r1", "ws1", PolicyAction::Block, "reboot", 10))
            .await
            .expect("add r1");
        engine
            .add_rule(rule("r2", "ws1", PolicyAction::Allow, "*", 1))
            .await
            .expect("add r2");

        let rules = engine.list_rules("ws1").await;
        assert_eq!(rules.len(), 2);
        // Sorted by priority desc.
        assert_eq!(rules[0].id, "r1");
        assert_eq!(rules[1].id, "r2");
        assert_eq!(rules[0].category, PolicyCategory::AgentAction);
        assert_eq!(rules[0].reason, "reason-r1");

        engine.remove_rule("r1").await.expect("remove");
        let rules = engine.list_rules("ws1").await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "r2");
    }

    #[tokio::test]
    async fn evaluate_arbitrates_by_priority() {
        let pool = test_pool().await;
        let engine = SqlitePolicyEngine::new(pool);

        engine
            .add_rule(rule("allow-low", "ws1", PolicyAction::Allow, "reboot", 1))
            .await
            .unwrap();
        engine
            .add_rule(rule("block-high", "ws1", PolicyAction::Block, "reboot", 10))
            .await
            .unwrap();
        engine
            .add_rule(rule("ra-star", "ws1", PolicyAction::RequireApproval, "*", 5))
            .await
            .unwrap();

        assert_eq!(
            engine.evaluate("ws1", PolicyCategory::AgentAction, "reboot").await,
            PolicyDecision::Block {
                reason: "reason-block-high".to_string()
            }
        );
        // Star rule fires for unlisted targets.
        assert_eq!(
            engine.evaluate("ws1", PolicyCategory::AgentAction, "shutdown").await,
            PolicyDecision::RequireApproval {
                reason: "reason-ra-star".to_string()
            }
        );
        // Category filter: no tool_execution rules → default Allow.
        assert_eq!(
            engine.evaluate("ws1", PolicyCategory::ToolExecution, "reboot").await,
            PolicyDecision::Allow
        );
    }

    #[tokio::test]
    async fn rules_are_workspace_isolated() {
        let pool = test_pool().await;
        let engine = SqlitePolicyEngine::new(pool);

        engine
            .add_rule(rule("r1", "ws1", PolicyAction::Block, "reboot", 10))
            .await
            .unwrap();

        assert!(matches!(
            engine.evaluate("ws2", PolicyCategory::AgentAction, "reboot").await,
            PolicyDecision::Allow
        ));
        assert!(engine.list_rules("ws2").await.is_empty());
    }

    #[tokio::test]
    async fn unknown_action_or_category_rows_are_skipped() {
        let pool = test_pool().await;
        // Bypass add_rule to insert rows with strings the enum can't parse.
        sqlx::query(
            "INSERT INTO policy_rules (id, workspace_id, category, action, target, priority)
             VALUES ('bad-action', 'ws1', 'agent_action', 'explode', '*', 100),
                    ('bad-category', 'ws1', 'nonsense', 'block', '*', 100)",
        )
        .execute(&pool)
        .await
        .expect("insert bad rows");

        let engine = SqlitePolicyEngine::new(pool);
        // Bad rows must not poison evaluation → default Allow.
        assert_eq!(
            engine.evaluate("ws1", PolicyCategory::AgentAction, "reboot").await,
            PolicyDecision::Allow
        );
        assert!(engine.list_rules("ws1").await.is_empty());
    }

    #[tokio::test]
    async fn read_failure_is_fail_closed_require_approval() {
        // No policy_rules table in this pool → every read errors.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory sqlite");
        let engine = SqlitePolicyEngine::new(pool);
        match engine.evaluate("ws1", PolicyCategory::AgentAction, "reboot").await {
            PolicyDecision::RequireApproval { reason } => assert_eq!(reason, "policy_read_failed"),
            other => panic!("read failure must fail closed as RequireApproval, got {other:?}"),
        }
        // list_rules degrades to empty rather than panicking.
        assert!(engine.list_rules("ws1").await.is_empty());
    }
}
