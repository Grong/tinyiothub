use std::sync::Arc;
use tinyiothub_edge::config::EdgeConfig;
use tinyiothub_edge::modules::config_mgmt::ConfigService;
use tinyiothub_storage::sqlite::{create_pool, Database, DatabaseConfig};

async fn test_db() -> Arc<Database> {
    let config = DatabaseConfig {
        url: "sqlite::memory:".into(),
        ..Default::default()
    };
    let pool = create_pool(&config).await.unwrap();
    // Create config_meta table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS config_meta (
        key TEXT PRIMARY KEY,
        cloud_version TEXT,
        local_version TEXT,
        updated_at INTEGER NOT NULL
    )",
    )
    .execute(&pool)
    .await
    .unwrap();
    Arc::new(Database::new(pool))
}

#[tokio::test]
async fn test_get_merged_config_returns_defaults() {
    let db = test_db().await;
    let config = EdgeConfig::default();
    let svc = ConfigService::new(db, config);

    svc.load_defaults().await;
    let merged = svc.get_merged_config().await.unwrap();

    assert!(merged.contains_key("telemetry_interval_secs"));
    assert_eq!(
        merged["telemetry_interval_secs"],
        serde_json::Value::from(30)
    );
    assert_eq!(
        merged["intelligence_interval_secs"],
        serde_json::Value::from(60)
    );
}

#[tokio::test]
async fn test_apply_cloud_config_merges_fields() {
    let db = test_db().await;
    let tmp_dir = std::env::temp_dir();
    let config_path = tmp_dir.join("test_merge_config.yaml");
    let mut config = EdgeConfig::default();
    config.config_file = config_path.clone();

    let svc = ConfigService::new(db, config);

    let cloud_config = serde_json::json!({
        "telemetry_interval_secs": 60,
        "new_custom_key": "custom_value"
    });

    svc.apply_cloud_config(&cloud_config).await.unwrap();
    let merged = svc.get_merged_config().await.unwrap();

    assert_eq!(
        merged["telemetry_interval_secs"],
        serde_json::Value::from(60)
    );
    assert_eq!(
        merged["new_custom_key"],
        serde_json::Value::String("custom_value".into())
    );

    std::fs::remove_file(&config_path).ok();
}

#[tokio::test]
async fn test_last_write_wins_overwrite() {
    let db = test_db().await;
    let tmp_dir = std::env::temp_dir();
    let config_path = tmp_dir.join("test_lww_config.yaml");
    let mut config = EdgeConfig::default();
    config.config_file = config_path.clone();

    let svc = ConfigService::new(db, config);

    // First write
    svc.apply_cloud_config(&serde_json::json!({"key": "v1"}))
        .await
        .unwrap();
    assert_eq!(
        svc.get_merged_config().await.unwrap()["key"],
        serde_json::Value::String("v1".into())
    );

    // Second write overwrites
    svc.apply_cloud_config(&serde_json::json!({"key": "v2"}))
        .await
        .unwrap();
    assert_eq!(
        svc.get_merged_config().await.unwrap()["key"],
        serde_json::Value::String("v2".into())
    );

    std::fs::remove_file(&config_path).ok();
}

#[tokio::test]
async fn test_version_comparison_skip_if_older() {
    let db = test_db().await;
    let config = EdgeConfig::default();
    let svc = ConfigService::new(db, config);

    svc.set_local_version("v3").await;

    // Cloud has older version — should return false
    let should_sync = svc.cloud_version_is_newer("v2").await;
    assert!(!should_sync);

    // Cloud has newer version — should return true
    let should_sync = svc.cloud_version_is_newer("v4").await;
    assert!(should_sync);
}

#[tokio::test]
async fn test_atomic_write_creates_file() {
    let db = test_db().await;
    let tmp_dir = std::env::temp_dir();
    let config_path = tmp_dir.join("test_atomic_config.yaml");
    let mut config = EdgeConfig::default();
    config.config_file = config_path.clone();

    let svc = ConfigService::new(db, config);
    svc.apply_cloud_config(&serde_json::json!({"test_key": "test_value"}))
        .await
        .unwrap();

    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("test_key"));
    std::fs::remove_file(&config_path).ok();
}
