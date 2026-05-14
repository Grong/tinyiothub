use std::sync::Arc;

use tinyiothub_core::models::device::CreateDeviceRequest;
use tinyiothub_core::repository::device::DeviceRepository;
use tinyiothub_storage::sqlite::device::SqliteDeviceRepository;
use tinyiothub_storage::sqlite::{Database, DatabaseConfig, create_pool};

use tinyiothub_edge::modules::device::DeviceService;

const DEVICES_TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT,
    device_type TEXT,
    address TEXT,
    description TEXT,
    position TEXT,
    driver_name TEXT,
    device_model TEXT,
    protocol_type TEXT,
    factory_name TEXT,
    linked_data TEXT,
    driver_options TEXT,
    state INTEGER NOT NULL DEFAULT 0,
    parent_id TEXT,
    product_id TEXT,
    workspace_id TEXT,
    linked_gateway TEXT,
    fingerprint TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
)
"#;

async fn setup_test_repo() -> Result<(Arc<Database>, Arc<SqliteDeviceRepository>), Box<dyn std::error::Error>> {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        ..Default::default()
    };
    let pool = create_pool(&config).await?;
    let db = Arc::new(Database::new(pool));

    // Create devices table
    db.execute(DEVICES_TABLE_DDL).await?;

    let repo = Arc::new(SqliteDeviceRepository::new(db.as_ref().clone()));

    Ok((db, repo))
}

fn make_create_request(name: &str, driver_name: &str) -> CreateDeviceRequest {
    CreateDeviceRequest {
        name: name.to_string(),
        driver_name: Some(driver_name.to_string()),
        device_type: Some("sensor".to_string()),
        ..Default::default()
    }
}

// ── list_devices ──────────────────────────────────────────────

#[tokio::test]
async fn test_list_devices_empty() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let devices = svc.list_devices(None).await.unwrap();
    assert!(devices.is_empty());
}

#[tokio::test]
async fn test_list_devices_with_driver_filter() {
    let (_db, repo) = setup_test_repo().await.unwrap();

    // Insert devices via the repo directly
    repo.create(&make_create_request("dev-a", "modbus")).await.unwrap();
    repo.create(&make_create_request("dev-b", "onvif")).await.unwrap();
    repo.create(&make_create_request("dev-c", "modbus")).await.unwrap();

    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let all = svc.list_devices(None).await.unwrap();
    assert_eq!(all.len(), 3, "expected 3 devices total");

    let modbus = svc.list_devices(Some("modbus")).await.unwrap();
    assert_eq!(modbus.len(), 2, "expected 2 modbus devices");

    let onvif = svc.list_devices(Some("onvif")).await.unwrap();
    assert_eq!(onvif.len(), 1, "expected 1 onvif device");

    let none = svc.list_devices(Some("nonexistent")).await.unwrap();
    assert!(none.is_empty(), "expected 0 devices for unknown driver");
}

// ── get_device ────────────────────────────────────────────────

#[tokio::test]
async fn test_get_device_found() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let created = repo.create(&make_create_request("my-device", "modbus")).await.unwrap();

    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let fetched = svc.get_device(&created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, "my-device");
    assert_eq!(fetched.driver_name.as_deref(), Some("modbus"));
}

#[tokio::test]
async fn test_get_device_not_found() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let result = svc.get_device("nonexistent-id").await;
    assert!(result.is_err(), "expected error for nonexistent device");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "expected 'not found' in error message, got: {}",
        err_msg
    );
}

// ── get_driver_for_device ─────────────────────────────────────

#[tokio::test]
async fn test_get_driver_for_device_has_driver() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let created = repo
        .create(&make_create_request("dev-with-driver", "snmp"))
        .await
        .unwrap();

    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let driver = svc.get_driver_for_device(&created.id).await.unwrap();
    assert_eq!(driver, "snmp");
}

#[tokio::test]
async fn test_get_driver_for_device_no_driver() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let created = repo
        .create(&CreateDeviceRequest {
            name: "no-driver-device".to_string(),
            driver_name: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let result = svc.get_driver_for_device(&created.id).await;
    assert!(result.is_err(), "expected error for device with no driver");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no driver configured"),
        "expected 'no driver configured' in error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_get_driver_for_device_not_found() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let result = svc.get_driver_for_device("nonexistent-id").await;
    assert!(result.is_err(), "expected error for nonexistent device");
}

// ── sync_from_cloud ───────────────────────────────────────────

#[tokio::test]
async fn test_sync_from_cloud_creates_devices() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let svc = DeviceService::new(repo.clone() as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let requests = vec![
        make_create_request("cloud-dev-1", "modbus"),
        make_create_request("cloud-dev-2", "onvif"),
    ];

    let created = svc.sync_from_cloud(&requests).await.unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].name, "cloud-dev-1");
    assert_eq!(created[1].name, "cloud-dev-2");

    // Verify they are persisted
    let all = svc.list_devices(None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_sync_from_cloud_empty_list() {
    let (_db, repo) = setup_test_repo().await.unwrap();
    let svc = DeviceService::new(repo as Arc<dyn tinyiothub_core::repository::device::DeviceRepository>);

    let created = svc.sync_from_cloud(&[]).await.unwrap();
    assert!(created.is_empty());

    // No devices should have been created
    let all = svc.list_devices(None).await.unwrap();
    assert!(all.is_empty());
}
