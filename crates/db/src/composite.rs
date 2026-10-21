//! E2 composite 声明持久源（spec 2026-10-21-e2 §6.1）。

use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CompositeRow {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub description: String,
    /// JSON（CompositeDef，v=1）。
    pub definition: String,
    pub created_at: String,
}

pub async fn insert_composite(pool: &SqlitePool, row: &CompositeRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_composites (id, workspace_id, name, description, definition, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.workspace_id)
    .bind(&row.name)
    .bind(&row.description)
    .bind(&row.definition)
    .bind(&row.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_composites(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<CompositeRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM agent_composites WHERE workspace_id = ? ORDER BY created_at")
        .bind(workspace_id)
        .fetch_all(pool)
        .await
}

pub async fn get_composite(
    pool: &SqlitePool,
    workspace_id: &str,
    id: &str,
) -> Result<Option<CompositeRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM agent_composites WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn delete_composite(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM agent_composites WHERE workspace_id = ? AND id = ?")
        .bind(workspace_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn composite_roundtrip_and_workspace_scoping() {
        let pool = test_pool().await;
        let row = CompositeRow {
            id: "c1".into(),
            workspace_id: "ws1".into(),
            name: "storm_guard".into(),
            description: "d".into(),
            definition: "{}".into(),
            created_at: "2026-10-21T00:00:00Z".into(),
        };
        insert_composite(&pool, &row).await.unwrap();
        assert_eq!(list_composites(&pool, "ws1").await.unwrap().len(), 1);
        assert!(get_composite(&pool, "ws1", "c1").await.unwrap().is_some());
        assert!(get_composite(&pool, "ws2", "c1").await.unwrap().is_none());
        assert_eq!(
            delete_composite(&pool, "ws2", "c1").await.unwrap(),
            0,
            "跨区删除必须 0 行"
        );
        assert_eq!(delete_composite(&pool, "ws1", "c1").await.unwrap(), 1);
    }
}
