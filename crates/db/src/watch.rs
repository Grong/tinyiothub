//! E1 值守中枢持久源（spec 2026-10-20-e1 §4）：watch_rules + hub_ledger。

use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WatchRuleRow {
    pub id: String,
    pub workspace_id: String,
    pub kind: String,
    pub condition: String,
    pub persistent: i64,
    pub timeout_secs: i64,
    pub created_at: String,
    pub owner_task_id: Option<String>,
}

pub async fn insert_rule(pool: &SqlitePool, row: &WatchRuleRow) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    insert_rule_tx(&mut tx, row).await?;
    let condition: serde_json::Value = serde_json::from_str(&row.condition).unwrap_or(serde_json::Value::Null);
    crate::runtime_event::append_runtime_event_tx(
        &mut tx,
        crate::runtime_event::RuntimeEventInput {
            event_id: format!("watch:{}:created", row.id),
            event_type: "watch.created".into(),
            occurred_at: chrono::Utc::now(),
            workspace_id: row.workspace_id.clone(),
            task_id: row.owner_task_id.clone(),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: condition
                .get("thing_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: serde_json::json!({}),
        },
    )
    .await
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    tx.commit().await?;
    Ok(())
}

/// 事务内插入（场景包实例化与 market 导入整包回滚）。
pub async fn insert_rule_tx(tx: &mut Transaction<'_, Sqlite>, row: &WatchRuleRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO watch_rules (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.workspace_id)
    .bind(&row.kind)
    .bind(&row.condition)
    .bind(row.persistent)
    .bind(row.timeout_secs)
    .bind(&row.created_at)
    .bind(&row.owner_task_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<WatchRuleRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id
         FROM watch_rules",
    )
    .fetch_all(pool)
    .await
}

pub async fn complete_task_watch(
    pool: &SqlitePool,
    workspace_id: &str,
    id: &str,
    wake: &tinyiothub_core::agent_task::TaskWake,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM watch_rules WHERE workspace_id = ? AND id = ? AND owner_task_id = ?")
        .bind(workspace_id)
        .bind(id)
        .bind(&wake.task_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO task_wake_outbox
             (task_id, event_id, signal_json, dispatch_state, created_at, dispatched_at, resolved_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(task_id, event_id) DO NOTHING",
    )
    .bind(&wake.task_id)
    .bind(&wake.event_id)
    .bind(&wake.signal_json)
    .bind(wake.dispatch_state.as_str())
    .bind(wake.created_at)
    .bind(wake.dispatched_at)
    .bind(wake.resolved_at)
    .execute(&mut *tx)
    .await?;
    crate::runtime_event::append_runtime_event_tx(
        &mut tx,
        crate::runtime_event::RuntimeEventInput {
            event_id: format!("watch:{id}:completed"),
            event_type: "watch.completed".into(),
            occurred_at: chrono::Utc::now(),
            workspace_id: workspace_id.to_string(),
            task_id: Some(wake.task_id.clone()),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: serde_json::json!({}),
        },
    )
    .await
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    tx.commit().await?;
    Ok(true)
}

pub async fn delete_task_watches(pool: &SqlitePool, workspace_id: &str, task_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM watch_rules WHERE workspace_id = ? AND owner_task_id = ?")
        .bind(workspace_id)
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
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
            owner_task_id: None,
        };
        insert_rule(&pool, &row).await.unwrap();
        assert_eq!(list_rules(&pool).await.unwrap().len(), 1);
        // 跨工作区删除必须落空（多租户隔离）
        assert_eq!(delete_rule(&pool, "ws2", "r1").await.unwrap(), 0);
        assert_eq!(delete_rule(&pool, "ws1", "r1").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn task_watch_completion_deletes_rule_and_commits_one_wake_atomically() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(&pool)
        .await
        .expect("seed plan");
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(&pool)
            .await
            .expect("seed tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','t1','now','now')",
        )
        .execute(&pool)
        .await
        .expect("seed workspace");
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, session_key, created_by,
                  status, current_run_id, state_version, created_at, updated_at, ended_at)
             VALUES ('task1','ws1','idem1','inspect','session1','user1','waiting_watch',NULL,0,'now','now',NULL)",
        )
        .execute(&pool)
        .await
        .expect("seed task");
        let row = WatchRuleRow {
            id: "r1".into(),
            workspace_id: "ws1".into(),
            kind: "property".into(),
            condition: r#"{"v":1}"#.into(),
            persistent: 0,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
            owner_task_id: Some("task1".into()),
        };
        insert_rule(&pool, &row).await.expect("insert owned watch");
        assert_eq!(
            list_rules(&pool).await.expect("list")[0].owner_task_id.as_deref(),
            Some("task1")
        );
        let wake = tinyiothub_core::agent_task::TaskWake {
            task_id: "task1".into(),
            event_id: "watch:r1:hit".into(),
            signal_json: r#"{"workspace_id":"ws1"}"#.into(),
            dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
            created_at: chrono::Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        };
        assert!(
            complete_task_watch(&pool, "ws1", "r1", &wake)
                .await
                .expect("complete watch")
        );
        assert!(list_rules(&pool).await.expect("deleted").is_empty());
        assert!(
            crate::Db::new(pool.clone())
                .wake_by_id("task1", "watch:r1:hit")
                .await
                .expect("wake")
                .is_some()
        );
        assert!(
            !complete_task_watch(&pool, "ws1", "r1", &wake)
                .await
                .expect("idempotent completion")
        );
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&pool)
            .await
            .expect("wake count");
        assert_eq!(wakes, 1);
    }

    #[tokio::test]
    async fn delete_task_watches_is_scoped_to_workspace_and_owner() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(&pool)
        .await
        .expect("seed plan");
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(&pool)
            .await
            .expect("seed tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','t1','now','now')",
        )
        .execute(&pool)
        .await
        .expect("seed workspace");
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, session_key, created_by,
                  status, current_run_id, state_version, created_at, updated_at, ended_at)
             VALUES ('task1','ws1','idem1','inspect','session1','user1','active',NULL,0,'now','now',NULL)",
        )
        .execute(&pool)
        .await
        .expect("seed task");
        let base = WatchRuleRow {
            id: "r1".into(),
            workspace_id: "ws1".into(),
            kind: "property".into(),
            condition: r#"{"v":1}"#.into(),
            persistent: 0,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
            owner_task_id: Some("task1".into()),
        };
        insert_rule(&pool, &base).await.expect("owned");
        insert_rule(
            &pool,
            &WatchRuleRow {
                id: "r2".into(),
                owner_task_id: None,
                ..base.clone()
            },
        )
        .await
        .expect("unowned");
        assert_eq!(
            delete_task_watches(&pool, "ws2", "task1")
                .await
                .expect("other workspace"),
            0
        );
        assert_eq!(delete_task_watches(&pool, "ws1", "task1").await.expect("owner"), 1);
        let remaining = list_rules(&pool).await.expect("remaining");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "r2");
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
