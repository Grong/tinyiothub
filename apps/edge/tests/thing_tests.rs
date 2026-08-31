use std::sync::Arc;

use tinyiothub_core::models::thing::CreateThingRequest;
use tinyiothub_storage::{DatabaseConfig, Db, create_pool_without_migrations};

use tinyiothub_edge::modules::thing::ThingService;

async fn setup_test_repo() -> Result<Arc<Db>, Box<dyn std::error::Error>> {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        ..Default::default()
    };
    let pool = create_pool_without_migrations(&config).await?;
    let db = Arc::new(Db::new(pool));

    // Create things table
    db.ensure_things_table().await?;

    Ok(db)
}

fn make_create_request(name: &str, driver_name: &str) -> CreateThingRequest {
    CreateThingRequest {
        name: name.to_string(),
        driver_name: Some(driver_name.to_string()),
        category: Some("sensor".to_string()),
        ..Default::default()
    }
}

// ── list_things ──────────────────────────────────────────────

#[tokio::test]
async fn test_list_things_empty() {
    let db = setup_test_repo().await.unwrap();
    let svc = ThingService::new(db);

    let things = svc.list_things(None).await.unwrap();
    assert!(things.is_empty());
}

#[tokio::test]
async fn test_list_things_with_driver_filter() {
    let db = setup_test_repo().await.unwrap();

    // Insert things via the db directly
    db.create_thing(None, &make_create_request("dev-a", "modbus"))
        .await
        .unwrap();
    db.create_thing(None, &make_create_request("dev-b", "onvif"))
        .await
        .unwrap();
    db.create_thing(None, &make_create_request("dev-c", "modbus"))
        .await
        .unwrap();

    let svc = ThingService::new(db);

    let all = svc.list_things(None).await.unwrap();
    assert_eq!(all.len(), 3, "expected 3 things total");

    let modbus = svc.list_things(Some("modbus")).await.unwrap();
    assert_eq!(modbus.len(), 2, "expected 2 modbus things");

    let onvif = svc.list_things(Some("onvif")).await.unwrap();
    assert_eq!(onvif.len(), 1, "expected 1 onvif thing");

    let none = svc.list_things(Some("nonexistent")).await.unwrap();
    assert!(none.is_empty(), "expected 0 things for unknown driver");
}

// ── get_thing ────────────────────────────────────────────────

#[tokio::test]
async fn test_get_thing_found() {
    let db = setup_test_repo().await.unwrap();
    let created = db
        .create_thing(None, &make_create_request("my-thing", "modbus"))
        .await
        .unwrap();

    let svc = ThingService::new(db);

    let fetched = svc.get_thing(&created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, "my-thing");
    assert_eq!(fetched.driver_name.as_deref(), Some("modbus"));
}

#[tokio::test]
async fn test_get_thing_not_found() {
    let db = setup_test_repo().await.unwrap();
    let svc = ThingService::new(db);

    let result = svc.get_thing("nonexistent-id").await;
    assert!(result.is_err(), "expected error for nonexistent thing");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "expected 'not found' in error message, got: {}",
        err_msg
    );
}

// ── get_driver_for_thing ─────────────────────────────────────

#[tokio::test]
async fn test_get_driver_for_thing_has_driver() {
    let db = setup_test_repo().await.unwrap();
    let created = db
        .create_thing(None, &make_create_request("dev-with-driver", "snmp"))
        .await
        .unwrap();

    let svc = ThingService::new(db);

    let driver = svc.get_driver_for_thing(&created.id).await.unwrap();
    assert_eq!(driver, "snmp");
}

#[tokio::test]
async fn test_get_driver_for_thing_no_driver() {
    let db = setup_test_repo().await.unwrap();
    let created = db
        .create_thing(
            None,
            &CreateThingRequest {
                name: "no-driver-thing".to_string(),
                driver_name: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let svc = ThingService::new(db);

    let result = svc.get_driver_for_thing(&created.id).await;
    assert!(result.is_err(), "expected error for thing with no driver");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no driver configured"),
        "expected 'no driver configured' in error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_get_driver_for_thing_not_found() {
    let db = setup_test_repo().await.unwrap();
    let svc = ThingService::new(db);

    let result = svc.get_driver_for_thing("nonexistent-id").await;
    assert!(result.is_err(), "expected error for nonexistent thing");
}

// ── sync_from_cloud ───────────────────────────────────────────

#[tokio::test]
async fn test_sync_from_cloud_creates_things() {
    let db = setup_test_repo().await.unwrap();
    let svc = ThingService::new(db.clone());

    let requests = vec![
        make_create_request("cloud-dev-1", "modbus"),
        make_create_request("cloud-dev-2", "onvif"),
    ];

    let created = svc.sync_from_cloud(&requests).await.unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].name, "cloud-dev-1");
    assert_eq!(created[1].name, "cloud-dev-2");

    // Verify they are persisted
    let all = svc.list_things(None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_sync_from_cloud_empty_list() {
    let db = setup_test_repo().await.unwrap();
    let svc = ThingService::new(db);

    let created = svc.sync_from_cloud(&[]).await.unwrap();
    assert!(created.is_empty());

    // No things should have been created
    let all = svc.list_things(None).await.unwrap();
    assert!(all.is_empty());
}
