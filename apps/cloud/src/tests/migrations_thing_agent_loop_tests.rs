//! Thing Agent Loop migration integration tests (T3)
//!
//! Runs the full migration chain against a fresh in-memory DB and asserts on
//! the 20260729000001_thing_agent_loop schema: the three new tables
//! (workspace_autonomy_policy / policy_rules / agent_runs), the
//! agent_daily_cost view, and events.actor (resonance guard: agent-produced
//! events must not re-wake the AI).

use sqlx::Row;

/// Fresh in-memory DB with the full migration chain applied.
async fn migrated_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool).await.expect("migrations");
    pool
}

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .unwrap();
    count > 0
}

// ──────────────────────────────────────────────
// ① Three new tables exist
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_thing_agent_loop_tables_exist() {
    let pool = migrated_pool().await;
    for table in ["workspace_autonomy_policy", "policy_rules", "agent_runs"] {
        assert!(table_exists(&pool, table).await, "{table} must exist after migrations");
    }
}

// ──────────────────────────────────────────────
// ② agent_runs full-field insert + indexed lookup
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_agent_runs_full_insert_and_indexed_query() {
    let pool = migrated_pool().await;

    sqlx::query(
        "INSERT INTO agent_runs (
            id, workspace_id, trigger_type, trigger_context, outcome, summary, report,
            verified, tool_calls, tokens, duration_ms, problem_key, dedup_key,
            acked_at, acked_by, created_at
         ) VALUES (
            'run-1', 'ws-1', 'timer', '{\"reason\":\"heartbeat\"}', 'acted', 'restarted sensor',
            '{\"actions\":[\"restart\"]}', 1, 2, 1500, 4200, 'sensor-offline', 'ws-1:sensor-offline',
            '2026-07-29 11:00:00', 'user-1', '2026-07-29 10:00:00'
         )",
    )
    .execute(&pool)
    .await
    .expect("full-field insert into agent_runs");

    let row = sqlx::query(
        "SELECT trigger_type, outcome, verified, tool_calls, tokens, duration_ms, acked_by
         FROM agent_runs
         WHERE workspace_id = 'ws-1' AND problem_key = 'sensor-offline' AND dedup_key = 'ws-1:sensor-offline'",
    )
    .fetch_one(&pool)
    .await
    .expect("indexed lookup by workspace_id + problem_key + dedup_key");

    assert_eq!(row.get::<String, _>("trigger_type"), "timer");
    assert_eq!(row.get::<String, _>("outcome"), "acted");
    assert_eq!(row.get::<i64, _>("verified"), 1);
    assert_eq!(row.get::<i64, _>("tool_calls"), 2);
    assert_eq!(row.get::<i64, _>("tokens"), 1500);
    assert_eq!(row.get::<i64, _>("duration_ms"), 4200);
    assert_eq!(row.get::<String, _>("acked_by"), "user-1");

    for index in ["idx_agent_runs_ws_created", "idx_agent_runs_problem", "idx_agent_runs_dedup"] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?)",
        )
        .bind(index)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{index} must exist");
    }
}

// ──────────────────────────────────────────────
// ③ agent_daily_cost aggregates same-day rows
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_agent_daily_cost_aggregates_same_day() {
    let pool = migrated_pool().await;

    for (id, time, tokens, duration_ms) in [
        ("run-a", "2026-07-29 08:00:00", 100, 1000),
        ("run-b", "2026-07-29 12:00:00", 200, 2000),
        ("run-c", "2026-07-29 23:59:59", 300, 3000),
    ] {
        sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, tokens, duration_ms, created_at)
             VALUES (?, 'ws-cost', 'timer', 'acted', ?, ?, ?)",
        )
        .bind(id)
        .bind(tokens)
        .bind(duration_ms)
        .bind(time)
        .execute(&pool)
        .await
        .expect("insert run row");
    }
    // Different day — must NOT be aggregated into the 2026-07-29 bucket.
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, tokens, duration_ms, created_at)
         VALUES ('run-other', 'ws-cost', 'timer', 'acted', 999, 999, '2026-07-30 00:00:01')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(
        "SELECT runs, tokens, duration_ms FROM agent_daily_cost
         WHERE workspace_id = 'ws-cost' AND day = '2026-07-29'",
    )
    .fetch_one(&pool)
    .await
    .expect("agent_daily_cost row for 2026-07-29");

    assert_eq!(row.get::<i64, _>("runs"), 3, "3 same-day runs aggregated");
    assert_eq!(row.get::<i64, _>("tokens"), 600);
    assert_eq!(row.get::<i64, _>("duration_ms"), 6000);
}

// ──────────────────────────────────────────────
// ④ events.actor defaults to 'device'
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_events_actor_defaults_to_device() {
    let pool = migrated_pool().await;

    sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, title)
         VALUES ('evt-1', 'device', 'telemetry', 2, '2026-07-29T10:00:00Z', 'device', 'temp reading')",
    )
    .execute(&pool)
    .await
    .expect("insert event without actor");

    let actor: String = sqlx::query_scalar("SELECT actor FROM events WHERE id = 'evt-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actor, "device", "events.actor must default to 'device'");
}

// ──────────────────────────────────────────────
// ⑤ workspace_autonomy_policy.mode CHECK rejects invalid values
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_autonomy_policy_mode_check_rejects_invalid() {
    let pool = migrated_pool().await;

    for mode in ["off", "diagnose", "act"] {
        sqlx::query("INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES (?, ?)")
            .bind(format!("ws-{mode}"))
            .bind(mode)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("valid mode {mode} must be accepted: {e}"));
    }

    let invalid = sqlx::query(
        "INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES ('ws-bad', 'yolo')",
    )
    .execute(&pool)
    .await;
    assert!(invalid.is_err(), "mode CHECK must reject 'yolo'");

    let default_mode: String = sqlx::query_scalar(
        "SELECT mode FROM workspace_autonomy_policy WHERE workspace_id = 'ws-off'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(default_mode, "off");
}
