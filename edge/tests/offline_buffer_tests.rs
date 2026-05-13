use tinyiothub_edge::modules::offline::{OfflineBuffer, BufferMessage, BufferPriority};
use tinyiothub_edge::config::EdgeConfig;
use std::sync::Arc;

async fn test_db() -> Arc<tinyiothub_storage::sqlite::Database> {
    use tinyiothub_storage::sqlite::{DatabaseConfig, create_pool};
    let config = DatabaseConfig {
        url: "sqlite::memory:".into(),
        ..Default::default()
    };
    let pool = create_pool(&config).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS offline_buffer (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        msg_type TEXT NOT NULL,
        topic TEXT,
        payload BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        retry_count INTEGER DEFAULT 0,
        priority INTEGER DEFAULT 0
    )").execute(&pool).await.unwrap();
    Arc::new(tinyiothub_storage::sqlite::Database::new(pool))
}

fn test_config() -> EdgeConfig {
    EdgeConfig::default()
}

#[tokio::test]
async fn test_write_and_count() {
    let db = test_db().await;
    let config = test_config();
    let buffer = OfflineBuffer::new(db.clone(), config);

    for i in 0..10 {
        buffer.write(BufferMessage {
            msg_type: "telemetry".into(),
            topic: "test/topic".into(),
            payload: format!(r#"{{"value":{}}}"#, i).into_bytes(),
            priority: BufferPriority::Normal,
        }).await.unwrap();
    }

    let status = buffer.get_status().await;
    assert_eq!(status.total_telemetry, 10);
}

#[tokio::test]
async fn test_fifo_eviction_at_capacity() {
    let db = test_db().await;
    let mut config = test_config();
    config.offline_buffer_max_telemetry = 5;
    let buffer = OfflineBuffer::new(db.clone(), config);

    // Write 6 telemetry messages — the first should be evicted
    for i in 0..6 {
        buffer.write(BufferMessage {
            msg_type: "telemetry".into(),
            topic: "test/topic".into(),
            payload: format!(r#"{{"value":{}}}"#, i).into_bytes(),
            priority: BufferPriority::Normal,
        }).await.unwrap();
    }

    let status = buffer.get_status().await;
    assert_eq!(status.total_telemetry, 5, "Should evict oldest to stay at capacity");
}

#[tokio::test]
async fn test_alarm_never_evicted() {
    let db = test_db().await;
    let mut config = test_config();
    config.offline_buffer_max_telemetry = 3;
    let buffer = OfflineBuffer::new(db.clone(), config);

    // Write 5 alarms (Permanent priority)
    for i in 0..5 {
        buffer.write(BufferMessage {
            msg_type: "alarm".into(),
            topic: "test/alarm".into(),
            payload: format!(r#"{{"alarm":{}}}"#, i).into_bytes(),
            priority: BufferPriority::Permanent,
        }).await.unwrap();
    }

    // Write 10 telemetry (should evict, but never alarms)
    for i in 0..10 {
        buffer.write(BufferMessage {
            msg_type: "telemetry".into(),
            topic: "test/topic".into(),
            payload: format!(r#"{{"value":{}}}"#, i).into_bytes(),
            priority: BufferPriority::Normal,
        }).await.unwrap();
    }

    let status = buffer.get_status().await;
    assert_eq!(status.total_alarms, 5, "Alarms must never be evicted");
    assert_eq!(status.total_telemetry, 3, "Telemetry FIFO eviction at capacity");
}

#[tokio::test]
async fn test_flush_batch_returns_deferred_ids() {
    let db = test_db().await;
    let config = test_config();
    let buffer = OfflineBuffer::new(db.clone(), config);

    buffer.write(BufferMessage {
        msg_type: "telemetry".into(), topic: "t1".into(),
        payload: b"p1".to_vec(), priority: BufferPriority::Normal,
    }).await.unwrap();
    buffer.write(BufferMessage {
        msg_type: "telemetry".into(), topic: "t2".into(),
        payload: b"p2".to_vec(), priority: BufferPriority::Normal,
    }).await.unwrap();

    // Call the simpler flush_batch signature
    let count = buffer.flush_batch(100).await.unwrap();
    assert!(count > 0);
}
