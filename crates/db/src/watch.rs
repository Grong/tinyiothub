//! E1 值守中枢持久源（spec 2026-10-20-e1 §4）：watch_rules + hub_ledger。

use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WatchRuleRow {
    pub id: String,
    pub workspace_id: String,
    pub kind: String,
    pub condition: String,
    pub persistent: i64,
    pub timeout_secs: i64,
    pub created_at: String,
}

pub async fn insert_rule(pool: &SqlitePool, row: &WatchRuleRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO watch_rules (id, workspace_id, kind, condition, persistent, timeout_secs, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.workspace_id)
    .bind(&row.kind)
    .bind(&row.condition)
    .bind(row.persistent)
    .bind(row.timeout_secs)
    .bind(&row.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<WatchRuleRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM watch_rules").fetch_all(pool).await
}

pub async fn delete_rule(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM watch_rules WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn append_ledger(
    pool: &SqlitePool,
    workspace_id: &str,
    ts: &str,
    kind: &str,
    thing_id: Option<&str>,
    payload: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO hub_ledger (workspace_id, ts, kind, thing_id, payload) VALUES (?, ?, ?, ?, ?)")
        .bind(workspace_id)
        .bind(ts)
        .bind(kind)
        .bind(thing_id)
        .bind(payload)
        .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        crate::test_helpers::test_pool().await
    }

    #[tokio::test]
    async fn watch_rule_roundtrip_and_workspace_scoped_delete() {
        let pool = test_pool().await;
        let row = WatchRuleRow {
            id: "r1".into(), workspace_id: "ws1".into(), kind: "property".into(),
            condition: r#"{"v":1}"#.into(), persistent: 0, timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
        };
        insert_rule(&pool, &row).await.unwrap();
        assert_eq!(list_rules(&pool).await.unwrap().len(), 1);
        // 跨工作区删除必须落空（多租户隔离）
        assert_eq!(delete_rule(&pool, "ws2", "r1").await.unwrap(), 0);
        assert_eq!(delete_rule(&pool, "ws1", "r1").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn ledger_append_and_read_back() {
        let pool = test_pool().await;
        append_ledger(&pool, "ws1", "2026-10-20T00:00:00Z", "watch_hit", Some("t1"), r#"{"v":1}"#)
            .await.unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM hub_ledger")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(n, 1);
    }
}
