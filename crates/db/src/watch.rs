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

/// E3 账本读面：一行 hub_ledger。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LedgerRow {
    pub id: i64,
    pub workspace_id: String,
    pub ts: String,
    pub kind: String,
    pub thing_id: Option<String>,
    pub payload: String,
}

/// E3：按工作区/时间窗/类型/物/cursor 查询账本（id 升序）。
pub async fn list_ledger_range(
    pool: &SqlitePool,
    workspace_id: &str,
    from: &str,
    to: &str,
    kinds: &[String],
    thing_id: Option<&str>,
    after_id: Option<i64>,
    limit: i64,
) -> Result<Vec<LedgerRow>, sqlx::Error> {
    let mut qb: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
        "SELECT id, workspace_id, ts, kind, thing_id, payload FROM hub_ledger WHERE workspace_id = ",
    );
    qb.push_bind(workspace_id);
    qb.push(" AND ts >= ").push_bind(from);
    qb.push(" AND ts <= ").push_bind(to);
    if !kinds.is_empty() {
        qb.push(" AND kind IN (");
        let mut sep = qb.separated(", ");
        for k in kinds {
            sep.push_bind(k);
        }
        qb.push(")");
    }
    if let Some(thing_id) = thing_id {
        qb.push(" AND thing_id = ").push_bind(thing_id);
    }
    if let Some(after_id) = after_id {
        qb.push(" AND id > ").push_bind(after_id);
    }
    qb.push(" ORDER BY id ASC LIMIT ").push_bind(limit);
    qb.build_query_as::<LedgerRow>().fetch_all(pool).await
}

/// E3 SSE：工作区当前最大账本 id（无行 = 0）。
pub async fn max_ledger_id(pool: &SqlitePool, workspace_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM hub_ledger WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_one(pool)
        .await
}

/// E3：重建 `at` 时刻每设备每字段的最近一次 state_change（按 (thing_id, field) 取 MAX(id)）。
pub async fn latest_state_changes_at(
    pool: &SqlitePool,
    workspace_id: &str,
    at: &str,
) -> Result<Vec<LedgerRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT h.id, h.workspace_id, h.ts, h.kind, h.thing_id, h.payload
         FROM hub_ledger h
         JOIN (
             SELECT thing_id,
                    json_extract(payload, '$.field') AS f,
                    MAX(id) AS max_id
             FROM hub_ledger
             WHERE workspace_id = ? AND kind = 'state_change' AND ts <= ?
             GROUP BY thing_id, f
         ) m ON h.id = m.max_id
         ORDER BY h.thing_id, h.id",
    )
    .bind(workspace_id)
    .bind(at)
    .fetch_all(pool)
    .await
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
            id: "r1".into(),
            workspace_id: "ws1".into(),
            kind: "property".into(),
            condition: r#"{"v":1}"#.into(),
            persistent: 0,
            timeout_secs: 600,
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
        append_ledger(
            &pool,
            "ws1",
            "2026-10-20T00:00:00Z",
            "watch_hit",
            Some("t1"),
            r#"{"v":1}"#,
        )
        .await
        .unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM hub_ledger")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    /// E3：范围查询按 workspace/kind/物/cursor 过滤，id 升序。
    #[tokio::test]
    async fn ledger_range_filters_and_cursor() {
        let pool = test_pool().await;
        for (ws, ts, kind, thing) in [
            ("ws1", "2026-10-21T10:00:00Z", "state_change", Some("t1")),
            ("ws1", "2026-10-21T10:01:00Z", "watch_hit", Some("t1")),
            ("ws1", "2026-10-21T10:02:00Z", "state_change", Some("t2")),
            ("ws2", "2026-10-21T10:03:00Z", "state_change", Some("t1")),
        ] {
            append_ledger(&pool, ws, ts, kind, thing, r#"{"v":1,"field":"温度","new":"1"}"#)
                .await
                .unwrap();
        }
        let all = list_ledger_range(
            &pool,
            "ws1",
            "2026-10-21T00:00:00Z",
            "2026-10-21T23:59:59Z",
            &[],
            None,
            None,
            50,
        )
        .await
        .unwrap();
        assert_eq!(all.len(), 3, "跨区必须隔离");
        assert!(all.windows(2).all(|w| w[0].id < w[1].id), "id 升序");

        let hits = list_ledger_range(
            &pool,
            "ws1",
            "2026-10-21T00:00:00Z",
            "2026-10-21T23:59:59Z",
            &["watch_hit".to_string()],
            None,
            None,
            50,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "watch_hit");

        let after = list_ledger_range(
            &pool,
            "ws1",
            "2026-10-21T00:00:00Z",
            "2026-10-21T23:59:59Z",
            &[],
            Some("t1"),
            None,
            50,
        )
        .await
        .unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|r| r.thing_id.as_deref() == Some("t1")));

        let page = list_ledger_range(
            &pool,
            "ws1",
            "2026-10-21T00:00:00Z",
            "2026-10-21T23:59:59Z",
            &[],
            None,
            Some(all[0].id),
            50,
        )
        .await
        .unwrap();
        assert_eq!(page.len(), 2);
    }

    /// E3：state_at 按 (thing, field) 取截止时刻前最新值并折叠。
    #[tokio::test]
    async fn state_at_folds_latest_per_field_before_cutoff() {
        let pool = test_pool().await;
        for (ts, thing, field, new) in [
            ("2026-10-21T10:00:00Z", "t1", "温度", "20"),
            ("2026-10-21T10:01:00Z", "t1", "温度", "22"),
            ("2026-10-21T10:01:30Z", "t1", "湿度", "50"),
            ("2026-10-21T10:02:00Z", "t1", "温度", "99"), // 截止后，必须忽略
            ("2026-10-21T10:00:30Z", "t2", "__availability", "false"),
        ] {
            append_ledger(
                &pool,
                "ws1",
                ts,
                "state_change",
                Some(thing),
                &format!(r#"{{"v":1,"field":"{field}","new":"{new}"}}"#),
            )
            .await
            .unwrap();
        }
        let rows = latest_state_changes_at(&pool, "ws1", "2026-10-21T10:01:59Z")
            .await
            .unwrap();
        let t1: Vec<_> = rows.iter().filter(|r| r.thing_id.as_deref() == Some("t1")).collect();
        assert_eq!(t1.len(), 2, "每字段一条：温度+湿度");
        let temp = t1.iter().find(|r| r.payload.contains("温度")).expect("温度");
        assert!(temp.payload.contains("\"new\":\"22\""), "必须取截止前最新值");
    }
}
