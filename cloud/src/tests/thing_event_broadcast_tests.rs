//! Thing event broadcast + actor marking integration tests (T6).
//!
//! Covers:
//! - a routed event publishes a `ThingEventSignal` on the global bus with all
//!   fields populated, and `event_id` equals the row's rowid (replay cursor);
//! - the `actor` parameter lands on the persisted row and on the signal
//!   (agent actions mark 'agent' — resonance guard);
//! - `CloudThingAgentHost::replay_events_since` honours the rowid cursor and
//!   `min_level` filter and only returns thing-sourced events.

use std::sync::Arc;

use tinyiothub_ai::thing_agent::ThingAgentHost;
use tinyiothub_core::models::event::EventLevel;

use crate::{
    modules::{
        agent::thing_agent_host::CloudThingAgentHost,
        event::{
            bus::ThingEventBus,
            router::{ThingEventInput, ThrottleState, route_thing_event},
        },
    },
    test_utils::seed_test_workspace,
};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    crate::shared::persistence::migrations::run_migrations(&pool).await.expect("migrations");
    pool
}

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, workspace_id: &str) {
    seed_test_workspace(pool, "tenant-1", workspace_id).await;
    sqlx::query("INSERT INTO devices (id, name, workspace_id) VALUES (?, ?, ?)")
        .bind(id)
        .bind(format!("Device {id}"))
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("insert device");
}

fn input(
    thing_id: &str,
    workspace_id: &str,
    event_name: &str,
    level: EventLevel,
) -> ThingEventInput {
    ThingEventInput {
        thing_id: thing_id.to_string(),
        workspace_id: workspace_id.to_string(),
        event_name: event_name.to_string(),
        level,
        data: serde_json::json!({"value": 42}),
        ts: None,
        template_events: None,
    }
}

#[tokio::test]
async fn test_routed_event_broadcasts_signal_with_all_fields() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-b", "ws-b").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();
    let mut rx = bus.subscribe();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-b", "ws-b", "temp_high", EventLevel::Warning),
    )
    .await;
    assert!(!result.malformed && !result.throttled);

    let signal = rx.recv().await.expect("subscriber must receive the routed event");
    assert_eq!(signal.workspace_id, "ws-b");
    assert_eq!(signal.thing_id, "dev-b");
    assert_eq!(signal.event_name, "temp_high");
    assert_eq!(signal.level, EventLevel::Warning.to_numeric());
    assert_eq!(signal.data, serde_json::json!({"value": 42}));
    assert!(!signal.is_unknown);
    assert_eq!(signal.actor, "device");

    // The signal's event_id must be the rowid of the persisted row so it can
    // double as a replay cursor.
    let rowid: i64 = sqlx::query_scalar("SELECT rowid FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(signal.event_id, rowid, "signal event_id must be the events.rowid cursor");
}

#[tokio::test]
async fn test_unknown_event_signal_carries_flag_and_degraded_level() {
    let pool = test_pool().await;
    // Template defines only "known_event" → "mystery" is flagged unknown.
    seed_test_workspace(&pool, "tenant-1", "ws-u").await;
    sqlx::query(
        "INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES ('test-cat', '{}', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO thing_templates (id, name, display_name, version, category, device_type, events, created_at, updated_at)
         VALUES ('tpl-u', 'tpl-u', 'T', '1.0', 'test-cat', 'sensor', '[{\"name\":\"known_event\"}]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO devices (id, name, workspace_id, template_id) VALUES ('dev-u', 'D', 'ws-u', 'tpl-u')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();
    let mut rx = bus.subscribe();

    route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-u", "ws-u", "mystery", EventLevel::Error),
    )
    .await;

    let signal = rx.recv().await.expect("signal");
    assert!(signal.is_unknown, "unknown event flag must reach the signal");
    assert_eq!(
        signal.level,
        EventLevel::Info.to_numeric(),
        "unknown events degrade to info — signal must carry the effective level"
    );
}

#[tokio::test]
async fn test_actor_agent_persisted_and_signaled() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-a", "ws-a").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();
    let mut rx = bus.subscribe();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "agent",
        input("dev-a", "ws-a", "action_result", EventLevel::Info),
    )
    .await;
    assert!(!result.malformed);

    let actor: String = sqlx::query_scalar("SELECT actor FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actor, "agent", "actor must persist on the events row");

    let signal = rx.recv().await.expect("signal");
    assert_eq!(signal.actor, "agent", "signal must carry the agent mark (resonance guard)");
}

#[tokio::test]
async fn test_actor_defaults_to_device_for_device_events() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-d", "ws-d").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new();

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-d", "ws-d", "ping", EventLevel::Info),
    )
    .await;

    let actor: String = sqlx::query_scalar("SELECT actor FROM events WHERE id = ?")
        .bind(&result.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actor, "device");
}

#[tokio::test]
async fn test_no_subscriber_still_persists() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-n", "ws-n").await;
    let throttle = ThrottleState::new(60);
    let bus = ThingEventBus::new(); // no subscribers

    let result = route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-n", "ws-n", "ping", EventLevel::Info),
    )
    .await;
    assert!(!result.malformed, "broadcast send failure (no subscribers) must not fail routing");
    assert!(!result.event_id.is_empty());
}

#[tokio::test]
async fn test_replay_events_since_filters_cursor_and_min_level() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-r", "ws-r").await;
    let bus = Arc::new(ThingEventBus::new());
    let host = CloudThingAgentHost::new(pool.clone(), bus.clone());

    let mut first_rowid = 0i64;
    for (name, level) in [
        ("lvl_info", EventLevel::Info),
        ("lvl_warn", EventLevel::Warning),
        ("lvl_err", EventLevel::Error),
        ("lvl_crit", EventLevel::Critical),
    ] {
        let throttle = ThrottleState::new(60);
        let r = route_thing_event(
            &pool,
            &throttle,
            None,
            &bus,
            "device",
            input("dev-r", "ws-r", name, level),
        )
        .await;
        assert!(!r.malformed);
        if first_rowid == 0 {
            first_rowid = sqlx::query_scalar("SELECT rowid FROM events WHERE id = ?")
                .bind(&r.event_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        }
    }

    // min_level filter: only error(4) and critical(5).
    let min_level = EventLevel::Error.to_numeric();
    let replayed = host.replay_events_since(0, min_level).await.expect("replay");
    assert_eq!(replayed.len(), 2, "min_level=error must drop info/warning");
    assert_eq!(replayed[0].event_name, "lvl_err");
    assert_eq!(replayed[1].event_name, "lvl_crit");
    assert!(replayed[0].event_id < replayed[1].event_id, "replay must be rowid-ordered");
    assert!(replayed.iter().all(|s| s.level >= min_level));
    assert!(replayed.iter().all(|s| s.workspace_id == "ws-r" && s.thing_id == "dev-r"));

    // Cursor filter: strictly greater than the first rowid.
    let after_first = host.replay_events_since(first_rowid, 1).await.expect("replay");
    assert_eq!(after_first.len(), 3, "cursor must exclude rows at or below it");
    assert!(after_first.iter().all(|s| s.event_id > first_rowid));
}

#[tokio::test]
async fn test_replay_skips_non_thing_events() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-x", "ws-x").await;
    let bus = Arc::new(ThingEventBus::new());
    let host = CloudThingAgentHost::new(pool.clone(), bus.clone());

    // A non-thing event row (e.g. status upsert from the legacy pipeline).
    sqlx::query(
        "INSERT INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, source_id, device_id, title, content, metadata, created_at, workspace_id, actor)
         VALUES ('evt-legacy', 'device', 'prop_change', 4, datetime('now'), 'device_property', 'dev-x:temperature', 'dev-x', 't', '{}', '{}', datetime('now'), 'ws-x', 'device')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let throttle = ThrottleState::new(60);
    route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-x", "ws-x", "real_thing_event", EventLevel::Info),
    )
    .await;

    let replayed = host.replay_events_since(0, 1).await.expect("replay");
    assert_eq!(replayed.len(), 1, "replay must only return thing-sourced events");
    assert_eq!(replayed[0].event_name, "real_thing_event");
}

#[tokio::test]
async fn test_host_subscribe_events_receives_broadcast() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-h", "ws-h").await;
    let bus = Arc::new(ThingEventBus::new());
    let host = CloudThingAgentHost::new(pool.clone(), bus.clone());
    let mut rx = host.subscribe_events();

    let throttle = ThrottleState::new(60);
    route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "device",
        input("dev-h", "ws-h", "via_host", EventLevel::Info),
    )
    .await;

    let signal = rx.recv().await.expect("host subscription must receive the signal");
    assert_eq!(signal.event_name, "via_host");
}

#[tokio::test]
async fn test_replay_actor_round_trip() {
    let pool = test_pool().await;
    insert_device(&pool, "dev-aa", "ws-aa").await;
    let bus = Arc::new(ThingEventBus::new());
    let host = CloudThingAgentHost::new(pool.clone(), bus.clone());

    let throttle = ThrottleState::new(60);
    route_thing_event(
        &pool,
        &throttle,
        None,
        &bus,
        "agent",
        input("dev-aa", "ws-aa", "agent_made", EventLevel::Warning),
    )
    .await;

    let replayed = host.replay_events_since(0, 1).await.expect("replay");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].actor, "agent", "replayed signal must carry the persisted actor");
    assert!(!replayed[0].is_unknown);
    assert_eq!(replayed[0].data, serde_json::json!({"value": 42}));
}
