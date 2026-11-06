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

fn watch_device_id(condition: &serde_json::Value) -> Option<String> {
    condition
        .get("thing_id")
        .or_else(|| condition.get("thingId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

pub async fn insert_rule(pool: &SqlitePool, row: &WatchRuleRow) -> Result<(), sqlx::Error> {
    insert_rule_and_event(pool, row).await.map(|_| ())
}

pub async fn insert_rule_and_event(
    pool: &SqlitePool,
    row: &WatchRuleRow,
) -> Result<crate::runtime_event::RuntimeEvent, sqlx::Error> {
    let mut tx = pool.begin().await?;
    insert_rule_tx(&mut tx, row).await?;
    let condition: serde_json::Value = serde_json::from_str(&row.condition).unwrap_or(serde_json::Value::Null);
    let event = crate::runtime_event::append_runtime_event_tx(
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
            device_id: watch_device_id(&condition),
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: serde_json::json!({}),
        },
    )
    .await
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    tx.commit().await?;
    Ok(event)
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

pub async fn find_rule(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<Option<WatchRuleRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id
         FROM watch_rules WHERE workspace_id = ? AND id = ?",
    )
    .bind(workspace_id)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Best-effort compensation after a cross-database completion failure: put the
/// platform rule row back without emitting a second `watch.created` event.
pub async fn restore_rule(pool: &SqlitePool, row: &WatchRuleRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO watch_rules
             (id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id)
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
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<WatchRuleRow>, sqlx::Error> {
    // Task terminality is a workspace-Harness fact (Task 19C4), so this
    // platform query no longer filters owner Tasks. Harness-aware callers
    // (`WatchRuleStoreAdapter::list_all`) drop rules whose owner Task is
    // terminal or no longer nonterminal in its workspace.
    sqlx::query_as(
        "SELECT w.id, w.workspace_id, w.kind, w.condition, w.persistent, w.timeout_secs,
                w.created_at, w.owner_task_id
         FROM watch_rules w",
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
    complete_task_watch_and_event(pool, workspace_id, id, wake)
        .await
        .map(|(completed, _)| completed)
}

pub async fn complete_task_watch_and_event(
    pool: &SqlitePool,
    workspace_id: &str,
    id: &str,
    wake: &tinyiothub_core::agent_task::TaskWake,
) -> Result<(bool, Option<crate::runtime_event::RuntimeEvent>), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM watch_rules WHERE workspace_id = ? AND id = ? AND owner_task_id = ?")
        .bind(workspace_id)
        .bind(id)
        .bind(&wake.task_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Ok((false, None));
    }
    // The continuation wake is a workspace-Harness Task fact (Task 19C4); the
    // caller records it in the owning workspace before completing the watch.
    let event = crate::runtime_event::append_runtime_event_tx(
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
    Ok((true, Some(event)))
}

async fn append_watch_deleted_events(
    tx: &mut Transaction<'_, Sqlite>,
    rows: &[WatchRuleRow],
) -> Result<Vec<crate::runtime_event::RuntimeEvent>, sqlx::Error> {
    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let condition: serde_json::Value = serde_json::from_str(&row.condition).unwrap_or(serde_json::Value::Null);
        let event = crate::runtime_event::append_runtime_event_tx(
            tx,
            crate::runtime_event::RuntimeEventInput {
                event_id: format!("watch:{}:deleted", row.id),
                event_type: "watch.deleted".into(),
                occurred_at: chrono::Utc::now(),
                workspace_id: row.workspace_id.clone(),
                task_id: row.owner_task_id.clone(),
                run_id: None,
                tool_call_id: None,
                job_id: None,
                approval_id: None,
                device_id: watch_device_id(&condition),
                source_id: None,
                source_seq: None,
                evidence_id: None,
                data: serde_json::json!({"watchId": row.id}),
            },
        )
        .await
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        events.push(event);
    }
    Ok(events)
}

pub async fn delete_task_watches(pool: &SqlitePool, workspace_id: &str, task_id: &str) -> Result<u64, sqlx::Error> {
    delete_task_watches_and_events(pool, workspace_id, task_id)
        .await
        .map(|events| events.len() as u64)
}

pub async fn delete_task_watches_and_events(
    pool: &SqlitePool,
    workspace_id: &str,
    task_id: &str,
) -> Result<Vec<crate::runtime_event::RuntimeEvent>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, WatchRuleRow>(
        "SELECT id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id
         FROM watch_rules WHERE workspace_id = ? AND owner_task_id = ? ORDER BY created_at, id",
    )
    .bind(workspace_id)
    .bind(task_id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM watch_rules WHERE workspace_id = ? AND owner_task_id = ?")
        .bind(workspace_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await?;
    let events = append_watch_deleted_events(&mut tx, &rows).await?;
    tx.commit().await?;
    Ok(events)
}

pub async fn delete_rule(pool: &SqlitePool, workspace_id: &str, id: &str) -> Result<u64, sqlx::Error> {
    delete_rule_and_event(pool, workspace_id, id)
        .await
        .map(|event| u64::from(event.is_some()))
}

pub async fn delete_rule_and_event(
    pool: &SqlitePool,
    workspace_id: &str,
    id: &str,
) -> Result<Option<crate::runtime_event::RuntimeEvent>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, WatchRuleRow>(
        "DELETE FROM watch_rules WHERE workspace_id = ? AND id = ?
         RETURNING id, workspace_id, kind, condition, persistent, timeout_secs, created_at, owner_task_id",
    )
    .bind(workspace_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let events = append_watch_deleted_events(&mut tx, row.as_slice()).await?;
    tx.commit().await?;
    Ok(events.into_iter().next())
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

    async fn owned_watch_fixture() -> SqlitePool {
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
        pool
    }

    fn owned_watch(id: &str) -> WatchRuleRow {
        WatchRuleRow {
            id: id.into(),
            workspace_id: "ws1".into(),
            kind: "property".into(),
            condition: r#"{"thing_id":"device1","v":1}"#.into(),
            persistent: 0,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
            owner_task_id: Some("task1".into()),
        }
    }

    #[tokio::test]
    async fn list_rules_returns_all_rows_without_a_platform_task_join() {
        let pool = owned_watch_fixture().await;
        sqlx::query(
            "INSERT INTO agent_tasks
                 (id, workspace_id, idempotency_key, objective, created_by, status,
                  state_version, created_at, updated_at, ended_at)
             VALUES ('task_done','ws1','idem2','finished','user1','completed',1,'now','now','now')",
        )
        .execute(&pool)
        .await
        .expect("terminal task");
        insert_rule(&pool, &owned_watch("r_live")).await.expect("live watch");
        let mut terminal = owned_watch("r_terminal");
        terminal.owner_task_id = Some("task_done".into());
        insert_rule(&pool, &terminal).await.expect("terminal watch");
        let mut unowned = owned_watch("r_unowned");
        unowned.owner_task_id = None;
        insert_rule(&pool, &unowned).await.expect("unowned watch");

        let listed = list_rules(&pool).await.expect("list");
        let ids = listed.iter().map(|rule| rule.id.as_str()).collect::<Vec<_>>();
        // Task terminality is a workspace Harness fact now; this platform query
        // returns every rule and `WatchRuleStoreAdapter::list_all` applies the
        // terminal filter from the owning workspace.
        assert_eq!(ids, ["r_live", "r_terminal", "r_unowned"]);
    }

    #[tokio::test]
    async fn watch_events_extract_snake_and_camel_case_device_ids() {
        let pool = owned_watch_fixture().await;
        let mut snake = owned_watch("r_snake");
        snake.condition = r#"{"thing_id":"device-snake","v":1}"#.into();
        let created = insert_rule_and_event(&pool, &snake).await.expect("snake watch");
        assert_eq!(created.device_id.as_deref(), Some("device-snake"));

        let mut camel = owned_watch("r_camel");
        camel.condition = r#"{"thingId":"device-camel","v":1}"#.into();
        let created = insert_rule_and_event(&pool, &camel).await.expect("camel watch");
        assert_eq!(created.device_id.as_deref(), Some("device-camel"));
        let deleted = delete_rule_and_event(&pool, "ws1", "r_camel")
            .await
            .expect("delete")
            .expect("deleted event");
        assert_eq!(deleted.device_id.as_deref(), Some("device-camel"));
    }

    #[tokio::test]
    async fn watch_deletions_commit_stable_events_and_replay_in_order() {
        let pool = owned_watch_fixture().await;
        insert_rule(&pool, &owned_watch("r1")).await.expect("first watch");
        insert_rule(&pool, &owned_watch("r2")).await.expect("second watch");
        let deleted = delete_task_watches_and_events(&pool, "ws1", "task1")
            .await
            .expect("cancel watches");
        assert_eq!(deleted.len(), 2);
        assert_eq!(deleted[0].event_id, "watch:r1:deleted");
        assert_eq!(deleted[1].event_id, "watch:r2:deleted");
        assert!(deleted[0].cursor < deleted[1].cursor);
        assert!(deleted.iter().all(|event| event.event_type == "watch.deleted"));
        assert!(deleted.iter().all(|event| event.task_id.as_deref() == Some("task1")));
        assert!(list_rules(&pool).await.expect("deleted").is_empty());
        assert!(
            delete_task_watches_and_events(&pool, "ws1", "task1")
                .await
                .expect("idempotent cancellation")
                .is_empty()
        );
        insert_rule(&pool, &owned_watch("r3"))
            .await
            .expect("generic delete watch");
        let generic = delete_rule_and_event(&pool, "ws1", "r3")
            .await
            .expect("delete rule")
            .expect("deletion event");
        assert_eq!(generic.event_id, "watch:r3:deleted");
        let replayed = crate::Db::new(pool)
            .runtime_events_after("ws1", None, Some("task1"), 20)
            .await
            .expect("replay")
            .into_iter()
            .filter(|event| event.event_type == "watch.deleted")
            .map(|event| event.event_id)
            .collect::<Vec<_>>();
        assert_eq!(replayed, ["watch:r1:deleted", "watch:r2:deleted", "watch:r3:deleted"]);
    }

    #[tokio::test]
    async fn watch_deletion_rolls_back_when_event_append_fails() {
        let pool = owned_watch_fixture().await;
        insert_rule(&pool, &owned_watch("r1")).await.expect("first watch");
        insert_rule(&pool, &owned_watch("r2")).await.expect("second watch");
        sqlx::query(
            "CREATE TRIGGER fail_watch_deleted BEFORE INSERT ON hub_ledger
             WHEN NEW.kind = 'runtime.v1' AND json_extract(NEW.payload, '$.eventType') = 'watch.deleted'
             BEGIN SELECT RAISE(ABORT, 'injected watch deletion event failure'); END",
        )
        .execute(&pool)
        .await
        .expect("failure trigger");
        delete_task_watches(&pool, "ws1", "task1")
            .await
            .expect_err("cancellation event failure");
        assert_eq!(list_rules(&pool).await.expect("cancellation rollback").len(), 2);
        delete_rule(&pool, "ws1", "r1")
            .await
            .expect_err("generic deletion event failure");
        assert_eq!(list_rules(&pool).await.expect("delete rollback").len(), 2);
        let deletion_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM hub_ledger
             WHERE kind = 'runtime.v1' AND json_extract(payload, '$.eventType') = 'watch.deleted'",
        )
        .fetch_one(&pool)
        .await
        .expect("event count");
        assert_eq!(deletion_events, 0);
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
        // Task 19C4: the continuation wake is a workspace fact and is recorded
        // by the Harness-aware adapter, never by this platform watch writer.
        let wake_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = 'task1' AND event_id = 'watch:r1:hit'",
        )
        .fetch_one(&pool)
        .await
        .expect("wake");
        assert_eq!(wake_count, 0);
        assert!(
            !complete_task_watch(&pool, "ws1", "r1", &wake)
                .await
                .expect("idempotent completion")
        );
        let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox")
            .fetch_one(&pool)
            .await
            .expect("wake count");
        assert_eq!(wakes, 0);
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
