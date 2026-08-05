use std::sync::Arc;

use tinyiothub_edge::modules::driver::DriverService;
use tinyiothub_edge::shared::error::EdgeError;

async fn test_db() -> Arc<tinyiothub_storage::sqlite::Database> {
    use tinyiothub_storage::sqlite::{DatabaseConfig, create_pool};
    let config = DatabaseConfig {
        url: "sqlite::memory:".into(),
        ..Default::default()
    };
    let pool = create_pool(&config).await.unwrap();
    Arc::new(tinyiothub_storage::sqlite::Database::new(pool))
}

#[tokio::test]
async fn test_scan_all_returns_ok() {
    let svc = DriverService::new(test_db().await, 10);
    let result = svc.scan_all().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_concurrent_scan_returns_busy_error() {
    let svc = DriverService::new(test_db().await, 10);

    let (r1, r2) = tokio::join!(svc.scan_all(), svc.scan_all());
    assert!(
        r1.is_ok() ^ r2.is_ok(),
        "Exactly one concurrent scan must succeed, got r1={:?} r2={:?}",
        r1,
        r2
    );
    let err = r1.err().or(r2.err()).unwrap();
    assert!(matches!(err, EdgeError::ScanBusy), "Expected ScanBusy, got {:?}", err);
}

#[tokio::test]
async fn test_sha256_verification_rejects_mismatch() {
    let svc = DriverService::new(test_db().await, 10);

    // Provide data that doesn't match the expected SHA256
    let data = b"not a valid .so file";
    let expected = "0000000000000000000000000000000000000000000000000000000000000000";

    let result = svc.load_dynamic_driver("test_driver", data, expected).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("SHA256") || err.contains("mismatch") || err.contains("hash"),
        "Error should mention SHA256/hash mismatch, got: {}",
        err
    );
}

#[tokio::test]
async fn test_sha256_verification_accepts_match() {
    use sha2::{Digest, Sha256};
    let svc = DriverService::new(test_db().await, 10);

    let data = b"valid .so content";
    let mut hasher = Sha256::new();
    hasher.update(data);
    let expected_hash = format!("{:x}", hasher.finalize());

    let result = svc.load_dynamic_driver("test_driver", data, &expected_hash).await;
    // May fail due to libloading (not a real .so), but should NOT fail with SHA256 mismatch
    if let Err(e) = &result {
        assert!(
            !e.to_string().contains("SHA256") && !e.to_string().contains("mismatch"),
            "Should not be SHA256 error for matching hash, got: {}",
            e
        );
    }
}

#[tokio::test]
async fn test_list_drivers_returns_list() {
    let svc = DriverService::new(test_db().await, 10);
    let drivers = svc.list_drivers().await;
    assert!(drivers.is_ok());
}
