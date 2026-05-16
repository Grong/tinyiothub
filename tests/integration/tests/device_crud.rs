//! Integration test: device CRUD lifecycle through the real storage layer.

use integration_tests::database;
use sqlx::Row;

#[tokio::test]
async fn device_create_read_update_delete() {
    let pool = database::create_test_pool().await;
    database::seed_test_workspace(&pool, "tenant-1", "ws-1").await;

    let device_id = uuid::Uuid::new_v4().to_string();
    let now = "2025-06-01T12:00:00Z";

    // CREATE
    sqlx::query(
        "INSERT INTO devices (id, name, device_type, protocol_type, state, workspace_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&device_id)
    .bind("Test Sensor")
    .bind("sensor")
    .bind("modbus")
    .bind(1i32)
    .bind("ws-1")
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // READ
    let row =
        sqlx::query("SELECT id, name, device_type, protocol_type, state, workspace_id FROM devices WHERE id = ?1")
            .bind(&device_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.get::<String, _>("name"), "Test Sensor");
    assert_eq!(row.get::<String, _>("device_type"), "sensor");
    assert_eq!(row.get::<String, _>("protocol_type"), "modbus");
    assert_eq!(row.get::<i32, _>("state"), 1);
    assert_eq!(row.get::<String, _>("workspace_id"), "ws-1");

    // UPDATE
    sqlx::query("UPDATE devices SET name = ?1, updated_at = ?2 WHERE id = ?3")
        .bind("Updated Sensor")
        .bind("2025-06-01T13:00:00Z")
        .bind(&device_id)
        .execute(&pool)
        .await
        .unwrap();

    let updated = sqlx::query_scalar::<_, String>("SELECT name FROM devices WHERE id = ?1")
        .bind(&device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(updated, "Updated Sensor");

    // DELETE
    sqlx::query("DELETE FROM devices WHERE id = ?1")
        .bind(&device_id)
        .execute(&pool)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = ?1")
        .bind(&device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
